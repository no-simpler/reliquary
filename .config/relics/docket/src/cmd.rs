use std::io::{IsTerminal, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::cli::*;
use crate::field;
use crate::help;
use crate::id::Id;
use jiff::Timestamp;

use crate::item::{Chain, Item, Kind, Rung, Stage, now};
use crate::render::{self, View};
use crate::store::{Depot, Record};
use crate::ui::{self, Format};

pub struct Ctx {
    pub depot: Depot,
    pub format: Format,
    pub color: bool,
    pub quiet: bool,
    pub project: PathBuf,
}

impl Ctx {
    fn note(&self, message: &str) {
        if !self.quiet {
            eprintln!("{message}");
        }
    }
}

/// `-` means standard input, the only way to hand a long value to a CLI without
/// worrying about what a shell will do to it.
fn text(value: &str) -> Result<String> {
    if value != "-" {
        return Ok(value.to_owned());
    }
    let mut buffer = String::new();
    std::io::stdin().read_to_string(&mut buffer)?;
    Ok(buffer.trim_end().to_owned())
}

fn parse_id(raw: &str) -> Result<Id> {
    raw.parse()
}

fn tilde(path: &Path) -> String {
    let display = path.display().to_string();
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() && display.starts_with(&home) => {
            format!("~{}", &display[home.len()..])
        }
        _ => display,
    }
}

pub fn list(ctx: &Ctx, args: &ListArgs) -> Result<()> {
    let projects = if args.all {
        ctx.depot.projects()
    } else {
        vec![ctx.project.clone()]
    };

    for (index, project) in projects.iter().enumerate() {
        let mut records = ctx.depot.list(project, args.archived);
        records.retain(|record| match &record.item {
            Ok(item) => {
                !args.invalid
                    && args.kind.is_none_or(|k| k == item.kind())
                    && (!args.blocked || item.is_blocked())
            }
            Err(_) => args.kind.is_none() && !args.blocked,
        });
        if args.all && records.is_empty() {
            continue;
        }
        if index > 0 {
            println!();
        }
        render::list(
            &View {
                project,
                records: &records,
                color: ctx.color,
            },
            ctx.format,
        )?;
    }
    Ok(())
}

pub fn create(ctx: &Ctx, args: &CreateArgs) -> Result<()> {
    let target = match &args.to {
        Some(path) => crate::store::resolve_lenient(path),
        None => ctx.project.clone(),
    };
    if !target.exists() && !args.allow_missing {
        bail!(
            "{} does not exist. If you are about to create it, say so: \
             `docket create {} --to {} --allow-missing ...`",
            target.display(),
            args.kind,
            target.display()
        );
    }

    let title = field::one_line("--title", &text(&args.title)?, field::TITLE_MAX)?;
    let tagline = field::one_line("--tagline", &text(&args.tagline)?, field::TAGLINE_MAX)?;
    let body = match &args.body {
        Some(raw) => text(raw)?,
        None => String::new(),
    };

    let id = ctx.depot.mint_id();
    let now = now();
    let item = Item {
        id,
        title,
        tagline,
        project: target.clone(),
        created: now,
        updated: now,
        order: ctx.depot.next_order(&target),
        rung: match args.kind {
            Kind::Handoff => Rung::Handoff,
            Kind::Relay => Rung::Relay(Chain {
                chain: id,
                hop: 1,
                supersedes: None,
            }),
            Kind::Spec => Rung::Spec {
                stage: Stage::Design,
                chain: None,
            },
        },
        blocked: None,
        origin: (target != ctx.project).then(|| ctx.project.clone()),
        tags: Vec::new(),
    };

    let path = ctx.depot.create(&item, &body)?;
    report_created(ctx, &item, &path, body.is_empty());
    Ok(())
}

fn report_created(ctx: &Ctx, item: &Item, path: &Path, empty: bool) {
    if ctx.format == Format::Json {
        let mut value = render::json::item_json(item);
        value["path"] = serde_json::json!(path);
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
        return;
    }
    println!("{}\t{}", item.id, path.display());
    if empty {
        ctx.note("write the body at that path; the frontmatter above it belongs to docket");
    }
}

pub fn show(ctx: &Ctx, args: &IdArgs) -> Result<()> {
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    let text = std::fs::read_to_string(&record.path)?;
    let (_, body) = crate::store::split(&text)?;
    if body.trim().is_empty() {
        ctx.note(&format!(
            "{} has no body yet — write one at {}",
            record.id,
            record.path.display()
        ));
        return Ok(());
    }
    print!("{body}");
    Ok(())
}

pub fn path(ctx: &Ctx, args: &IdArgs) -> Result<()> {
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    println!("{}", record.path.display());
    Ok(())
}

pub fn set(ctx: &Ctx, args: &SetArgs) -> Result<()> {
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    let mut item = match &record.item {
        Ok(item) => item.clone(),
        Err(error) => {
            ctx.note(&format!(
                "{} was invalid ({error}) — rebuilding it",
                record.id
            ));
            recover(&record)?
        }
    };

    if let Some(value) = &args.title {
        item.title = field::one_line("--title", &text(value)?, field::TITLE_MAX)?;
    }
    if let Some(value) = &args.tagline {
        item.tagline = field::one_line("--tagline", &text(value)?, field::TAGLINE_MAX)?;
    }
    if let Some(value) = &args.blocked {
        let value = text(value)?;
        if value.trim().is_empty() {
            bail!(
                "--blocked says what must clear. To drop the block: `docket set {id} --clear-blocked`",
                id = record.id
            );
        }
        item.blocked = Some(field::one_line("--blocked", &value, field::BLOCKED_MAX)?);
    }
    if args.clear_blocked {
        item.blocked = None;
    }
    if let Some(tags) = &args.tags {
        item.tags = tags
            .iter()
            .filter_map(|t| field::tag(t).transpose())
            .collect::<Result<_>>()?;
    }
    item.updated = now();

    let path = ctx.depot.save(&record, &item)?;
    ctx.note(&format!("{} updated", item.id));
    if ctx.format == Format::Json {
        let mut value = render::json::item_json(&item);
        value["path"] = serde_json::json!(path);
        println!("{}", serde_json::to_string_pretty(&value)?);
    }
    Ok(())
}

/// Rebuilds an item whose frontmatter stopped deserialising, keeping every
/// field that still reads and filling the rest from the file's own location.
fn recover(record: &Record) -> Result<Item> {
    use serde_yaml_ng::Value;

    let raw = std::fs::read_to_string(&record.path)?;
    let (front, _) = crate::store::split(&raw)?;
    let map: Value = serde_yaml_ng::from_str(front).map_err(|e| {
        anyhow!(
            "{} is damaged past automatic repair ({e}). Edit it directly: {}",
            record.id,
            record.path.display()
        )
    })?;
    let get = |key: &str| -> Option<String> {
        map.get(key).and_then(|v| {
            v.as_str()
                .map(str::to_owned)
                .or_else(|| v.as_i64().map(|n| n.to_string()))
        })
    };

    let kind = get("kind")
        .and_then(|k| match k.as_str() {
            "handoff" => Some(Kind::Handoff),
            "relay" => Some(Kind::Relay),
            "spec" => Some(Kind::Spec),
            _ => None,
        })
        .unwrap_or(record.kind);
    let now = now();
    let stamp = |key: &str| {
        get(key)
            .and_then(|v| v.parse::<Timestamp>().ok())
            .unwrap_or(now)
    };

    Ok(Item {
        id: record.id,
        title: get("title")
            .map(|t| field::clamp(&t, field::TITLE_MAX))
            .unwrap_or_else(|| format!("recovered {}", record.id)),
        tagline: get("tagline")
            .or_else(|| get("description"))
            .map(|t| field::clamp(&t, field::TAGLINE_MAX))
            .unwrap_or_else(|| "Recovered from damaged frontmatter.".to_owned()),
        project: record.project.clone(),
        created: stamp("created"),
        updated: now,
        order: get("order").and_then(|o| o.parse().ok()).unwrap_or(0),
        rung: match kind {
            Kind::Handoff => Rung::Handoff,
            Kind::Relay => Rung::Relay(Chain {
                chain: get("chain")
                    .and_then(|c| c.parse().ok())
                    .unwrap_or(record.id),
                hop: get("hop").and_then(|h| h.parse().ok()).unwrap_or(1),
                supersedes: get("supersedes").and_then(|s| s.parse().ok()),
            }),
            Kind::Spec => Rung::Spec {
                stage: match get("stage").as_deref() {
                    Some("implementation") => Stage::Implementation,
                    _ => Stage::Design,
                },
                chain: None,
            },
        },
        blocked: get("blocked").map(|b| field::clamp(&b, field::BLOCKED_MAX)),
        origin: None,
        tags: Vec::new(),
    })
}

pub fn reorder(ctx: &Ctx, args: &ReorderArgs) -> Result<()> {
    let mut ids: Vec<Id> = ctx
        .depot
        .list(&ctx.project, false)
        .iter()
        .map(|record| record.id)
        .collect();

    if let Some(sequence) = &args.sequence {
        let wanted: Vec<Id> = sequence
            .iter()
            .map(|s| parse_id(s))
            .collect::<Result<_>>()?;
        for id in &wanted {
            if !ids.contains(id) {
                bail!("{id} is not on this project's docket");
            }
        }
        let mut ordered = wanted.clone();
        ordered.extend(ids.into_iter().filter(|id| !wanted.contains(id)));
        let touched = ctx.depot.resequence(&ctx.project, &ordered)?;
        ctx.note(&format!("reordered ({touched} moved)"));
        return Ok(());
    }

    let id = parse_id(args.id.as_deref().unwrap_or_default())?;
    let from = ids
        .iter()
        .position(|candidate| *candidate == id)
        .ok_or_else(|| anyhow!("{id} is not on this project's docket"))?;

    let to = if args.top {
        0
    } else if args.bottom {
        ids.len() - 1
    } else if let Some(position) = args.position {
        position.saturating_sub(1).min(ids.len() - 1)
    } else if let Some(anchor) = &args.before {
        anchor_index(&ids, parse_id(anchor)?)?
    } else if let Some(anchor) = &args.after {
        anchor_index(&ids, parse_id(anchor)?)? + 1
    } else {
        bail!("say where it goes: --top, --bottom, --position N, --before ID or --after ID");
    };

    ids.remove(from);
    ids.insert(to.min(ids.len()), id);
    ctx.depot.resequence(&ctx.project, &ids)?;
    ctx.note(&format!(
        "{id} is now at position {}",
        ids.iter().position(|c| *c == id).unwrap_or(0) + 1
    ));
    Ok(())
}

fn anchor_index(ids: &[Id], anchor: Id) -> Result<usize> {
    ids.iter()
        .position(|candidate| *candidate == anchor)
        .ok_or_else(|| anyhow!("{anchor} is not on this project's docket"))
}

pub fn promote(ctx: &Ctx, args: &PromoteArgs) -> Result<()> {
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    let mut item = record
        .item
        .as_ref()
        .map_err(|error| {
            anyhow!(
                "{} will not parse ({error}), so it cannot be promoted. Repair it first: `docket set {}`",
                record.id,
                record.id
            )
        })?
        .clone();

    let step = item.promote(args.to)?;
    item.updated = now();
    let path = ctx.depot.save(&record, &item)?;
    println!(
        "{}\t{}\t{}",
        item.id,
        render::kind_badge(&item),
        path.display()
    );
    ctx.note(&format!("{step}"));
    Ok(())
}

pub fn relay(ctx: &Ctx, args: &RelayArgs) -> Result<()> {
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    let item = record
        .item
        .as_ref()
        .map_err(|error| {
            anyhow!(
                "{} will not parse ({error}), so it cannot be relayed. Repair it first: `docket set {}`",
                record.id,
                record.id
            )
        })?;

    let title = field::one_line("--title", &text(&args.title)?, field::TITLE_MAX)?;
    let tagline = field::one_line("--tagline", &text(&args.tagline)?, field::TAGLINE_MAX)?;
    let body = match &args.body {
        Some(raw) => text(raw)?,
        None => String::new(),
    };

    let successor = item.successor(ctx.depot.mint_id(), title, tagline)?;
    let path = ctx.depot.create(&successor, &body)?;
    ctx.depot.archive(&record)?;

    println!("{}\t{}", successor.id, path.display());
    ctx.note(&format!(
        "{} archived; {} carries the chain on to hop {}",
        record.id,
        successor.id,
        successor.rung.chain().map(|c| c.hop).unwrap_or_default()
    ));
    if body.is_empty() {
        ctx.note("write the successor's body at that path");
    }
    Ok(())
}

pub fn close(ctx: &Ctx, args: &IdArgs) -> Result<()> {
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    let archived = ctx.depot.archive(&record)?;
    ctx.note(&format!("{} closed", record.id));
    if ctx.format == Format::Json {
        println!(
            "{}",
            serde_json::json!({ "id": record.id.to_string(), "archived": archived })
        );
    }
    Ok(())
}

pub fn delete(ctx: &Ctx, args: &DeleteArgs) -> Result<()> {
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    if !args.force && std::io::stdin().is_terminal() {
        eprint!(
            "delete {} ({}) with no archive copy? [y/N] ",
            record.id,
            record.title()
        );
        std::io::stderr().flush()?;
        let mut answer = String::new();
        std::io::stdin().read_line(&mut answer)?;
        if !matches!(answer.trim().to_ascii_lowercase().as_str(), "y" | "yes") {
            ctx.note("left alone");
            return Ok(());
        }
    }
    ctx.depot.delete(&record)?;
    ctx.note(&format!("{} deleted", record.id));
    Ok(())
}

pub fn doctor(ctx: &Ctx) -> Result<bool> {
    let mut problems = 0usize;
    let mut report = |line: String| {
        problems += 1;
        println!("{line}");
    };

    for project in ctx.depot.projects() {
        for record in ctx.depot.list(&project, false) {
            match &record.item {
                Err(error) => report(format!(
                    "invalid  {} {}\n         {error}\n         repair: `docket set {}`",
                    record.id,
                    record.path.display(),
                    record.id
                )),
                Ok(item) => {
                    if item.project != project {
                        report(format!(
                            "misfiled {} claims {} but sits under {}",
                            record.id,
                            item.project.display(),
                            project.display()
                        ));
                    }
                    let body = std::fs::read_to_string(&record.path).unwrap_or_default();
                    if crate::store::split(&body)
                        .map(|(_, b)| b.trim().is_empty())
                        .unwrap_or(false)
                    {
                        report(format!(
                            "empty    {} has no body — {}",
                            record.id,
                            record.path.display()
                        ));
                    }
                    for (label, value, max) in [
                        ("title", Some(item.title.as_str()), field::TITLE_MAX),
                        ("tagline", Some(item.tagline.as_str()), field::TAGLINE_MAX),
                        ("blocked", item.blocked.as_deref(), field::BLOCKED_MAX),
                    ] {
                        let Some(value) = value else { continue };
                        if field::is_overlong(value, max) {
                            report(format!(
                                "overlong {} has a {} of {} characters, over the limit of {max}\n         \
                                 repair: `docket set {} --{label} '...'`",
                                record.id,
                                label,
                                value.chars().count(),
                                record.id
                            ));
                        }
                    }
                    if ui::age_days(item.created) > 60 {
                        report(format!(
                            "stale    {} has been open {} — {}",
                            record.id,
                            ui::age(item.created),
                            item.title
                        ));
                    }
                }
            }
        }
    }

    if !hook_is_wired() {
        report(
            "unwired  no SessionStart hook mentions docket in ~/.claude/settings.json,\n         \
             so nothing announces outstanding work at session start"
                .to_owned(),
        );
    }

    if problems == 0 {
        println!(
            "docket: {}, nothing wrong",
            plural(ctx.depot.projects().len(), "project", "projects")
        );
    }
    Ok(problems == 0)
}

fn hook_is_wired() -> bool {
    let Some(home) = std::env::var_os("HOME") else {
        return true;
    };
    let settings = PathBuf::from(home).join(".claude").join("settings.json");
    let Ok(text) = std::fs::read_to_string(settings) else {
        return true;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text) else {
        return true;
    };
    value
        .pointer("/hooks/SessionStart")
        .map(|hooks| hooks.to_string().contains("docket"))
        .unwrap_or(false)
}

pub fn announce(ctx: &Ctx, args: &AnnounceArgs) -> Result<()> {
    let records = ctx.depot.list(&ctx.project, false);
    if records.is_empty() {
        return Ok(());
    }

    let mut lines = Vec::new();
    let mut specs = Vec::new();
    let mut invalid = 0usize;

    for record in &records {
        match &record.item {
            Err(_) => invalid += 1,
            Ok(item) if item.kind() == Kind::Spec => {
                let stage = match &item.rung {
                    Rung::Spec { stage, .. } => stage.to_string(),
                    _ => String::new(),
                };
                let blocked = if item.is_blocked() { ", blocked" } else { "" };
                specs.push(format!("[{}] {} ({stage}{blocked})", item.id, item.title));
            }
            Ok(item) => {
                let mut marks = vec![ui::age(item.created)];
                if let Rung::Relay(chain) = &item.rung {
                    marks.push(format!("relay hop={}", chain.hop));
                }
                if item.is_blocked() {
                    marks.push("blocked".to_owned());
                }
                lines.push(format!(
                    "  [{}] {}   {}",
                    item.id,
                    item.title,
                    marks.join("  ")
                ));
            }
        }
    }

    let mut out = String::new();
    if !lines.is_empty() {
        out.push_str(&format!(
            "{} on the docket for {} (`docket` to list):\n{}\n",
            plural(lines.len(), "item", "items"),
            tilde(&ctx.project),
            lines.join("\n")
        ));
    }
    if !specs.is_empty() {
        out.push_str(&format!("Specs: {}\n", specs.join(", ")));
    }
    if invalid > 0 {
        out.push_str(&format!(
            "{} — run `docket doctor`\n",
            plural(invalid, "invalid item", "invalid items")
        ));
    }
    out.push_str("Workflow directives: /docket\n");

    if args.hook {
        println!(
            "{}",
            serde_json::json!({
                "hookSpecificOutput": {
                    "hookEventName": "SessionStart",
                    "additionalContext": out,
                }
            })
        );
    } else {
        print!("{out}");
    }
    Ok(())
}

fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

pub fn help_topic(args: &HelpArgs, command: &mut clap::Command) -> Result<()> {
    let Some(name) = args.topic.as_deref() else {
        println!("{}", command.render_long_help());
        return Ok(());
    };
    if name == "topics" {
        println!("{}", help::topic_names());
        return Ok(());
    }
    if let Some(body) = help::topic(name) {
        println!("{body}");
        return Ok(());
    }
    if let Some(sub) = command.find_subcommand_mut(name) {
        println!("{}", sub.render_long_help());
        return Ok(());
    }
    bail!(
        "no topic or command called {name:?}. Topics: {}",
        help::topic_names()
    )
}

pub fn completions(args: &CompletionArgs, command: &mut clap::Command) -> Result<()> {
    clap_complete::generate(args.shell, command, "docket", &mut std::io::stdout().lock());
    Ok(())
}

pub fn open_context(global: &Global) -> Result<Ctx> {
    let depot = Depot::open()?;
    let format = if global.json {
        Format::Json
    } else {
        ui::resolve_format(global.format)
    };
    let project = match &global.project {
        Some(path) => crate::store::resolve_lenient(path),
        None => crate::store::project_key(
            &std::env::current_dir().context("reading the working directory")?,
        ),
    };
    Ok(Ctx {
        depot,
        format,
        color: ui::use_color(global.color, format),
        quiet: global.quiet,
        project,
    })
}
