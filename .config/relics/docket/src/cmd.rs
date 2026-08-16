use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::cli::*;
use crate::field;
use crate::git;
use crate::guide;
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

/// Held for the whole of a mutating command: the depot lock, and the depot's
/// history when git is present.
///
/// Opening one first commits whatever was edited outside docket — bodies are
/// authored through the path docket prints, so a change must never land on top
/// of unrecorded work. The command's own change is then a second, typed commit.
struct Mutation<'a> {
    ctx: &'a Ctx,
    repo: Option<git::Repo>,
    _lock: File,
}

impl<'a> Mutation<'a> {
    fn open(ctx: &'a Ctx) -> Result<Mutation<'a>> {
        let lock = ctx.depot.lock_depot()?;
        let repo = git::Repo::ensure(&ctx.depot.root).unwrap_or_else(|error| {
            ctx.note(&format!("history unavailable: {error:#}"));
            None
        });
        let mutation = Mutation {
            ctx,
            repo,
            _lock: lock,
        };
        if let Some(repo) = &mutation.repo
            && let Err(error) = snapshot_drift(repo)
        {
            ctx.note(&format!("history not updated: {error:#}"));
        }
        Ok(mutation)
    }

    /// Records the command's own change.
    fn record(&self, message: &str) {
        let Some(repo) = &self.repo else { return };
        if let Err(error) = repo.snapshot(message) {
            self.ctx.note(&format!("history not updated: {error:#}"));
        }
    }

    /// Removes an item and records the removal, refusing unless history already
    /// holds it. The one place the git layer is load-bearing rather than
    /// additive: without it, closing would be deletion with nothing behind it.
    fn close(&self, footprint: &Path, message: &str) -> Result<String> {
        let Some(repo) = &self.repo else {
            bail!(
                "closing needs git, because the depot's history is what keeps a closed item. \
                 Put git on PATH, or remove {} by hand",
                footprint.display()
            );
        };
        if !repo.is_recorded(footprint)? {
            bail!(
                "history does not hold {} yet, so closing it would lose it. \
                 Run docket doctor",
                footprint.display()
            );
        }
        repo.remove(footprint, message)
    }
}

/// Commits what was edited outside docket, under its own message, naming the
/// items it touched. Best-effort at every call site: a depot that cannot be
/// committed is a matter for docket doctor, never a reason to refuse the work
/// in hand.
fn snapshot_drift(repo: &git::Repo) -> Result<()> {
    let ids = repo.drifted_ids()?;
    if ids.is_empty() {
        return Ok(());
    }
    let named: Vec<String> = ids.iter().map(Id::to_string).collect();
    repo.snapshot(&format!("edit: {}", named.join(", ")))?;
    Ok(())
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

pub fn list(ctx: &Ctx, args: &ListArgs) -> Result<()> {
    let projects = if args.all {
        ctx.depot.projects()
    } else {
        vec![ctx.project.clone()]
    };

    for (index, project) in projects.iter().enumerate() {
        let mut records = ctx.depot.list(project);
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
    let mutation = Mutation::open(ctx)?;
    let target = match &args.to {
        Some(path) => crate::store::project_key(path),
        None => ctx.project.clone(),
    };
    if !target.exists() && !args.allow_missing {
        bail!(
            "{} does not exist. If you are about to create it, say so: \
             docket create {} --to {} --allow-missing ...",
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
    mutation.record(&format!("create {}: {}", item.id, item.title));
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
        ctx.note("write the body at that path; the metadata above it belongs to docket");
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
    let mutation = Mutation::open(ctx)?;
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
                "--blocked says what must clear. To drop the block: docket set {id} --clear-blocked",
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
    mutation.record(&format!("set {}", item.id));
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
            .unwrap_or_else(|| "Recovered from damaged metadata.".to_owned()),
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
    let mutation = Mutation::open(ctx)?;
    let mut ids: Vec<Id> = ctx
        .depot
        .list(&ctx.project)
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
        mutation.record(&format!(
            "reorder {}",
            crate::store::slug_for_path(&ctx.project)
        ));
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
    mutation.record(&format!(
        "reorder {}",
        crate::store::slug_for_path(&ctx.project)
    ));
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
    let mutation = Mutation::open(ctx)?;
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    let mut item = record
        .item
        .as_ref()
        .map_err(|error| {
            anyhow!(
                "{} will not parse ({error}), so it cannot be promoted. Repair it first: docket set {}",
                record.id,
                record.id
            )
        })?
        .clone();

    let was = record.kind;
    let step = item.promote(args.to)?;
    item.updated = now();
    let path = ctx.depot.save(&record, &item)?;
    mutation.record(&format!("promote {}: {was} to {}", item.id, item.kind()));
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
    let mutation = Mutation::open(ctx)?;
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    let item = record.item.as_ref().map_err(|error| {
        anyhow!(
            "{} will not parse ({error}), so it cannot be relayed. Repair it first: docket set {}",
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

    // The predecessor is closed after the successor exists, and both land in
    // one commit: a chain that lost a hop to a half-finished relay would be
    // unrecoverable.
    let successor = item.successor(ctx.depot.mint_id(), title, tagline)?;
    let path = ctx.depot.create(&successor, &body)?;
    let footprint = ctx.depot.footprint(&record)?;
    let commit = mutation.close(
        &footprint,
        &format!("relay {} to {}", record.id, successor.id),
    )?;

    println!("{}\t{}", successor.id, path.display());
    ctx.note(&format!(
        "{} closed at {commit}; {} carries the chain on to hop {}",
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
    let mutation = Mutation::open(ctx)?;
    let record = ctx.depot.find(parse_id(&args.id)?)?;
    let title = record
        .item
        .as_ref()
        .map(|item| item.title.clone())
        .unwrap_or_else(|_| "invalid metadata".to_owned());
    let footprint = ctx.depot.footprint(&record)?;
    let commit = mutation.close(&footprint, &format!("close {}: {title}", record.id))?;

    ctx.note(&format!("{} closed at {commit}", record.id));
    if ctx.format == Format::Json {
        println!(
            "{}",
            serde_json::json!({
                "id": record.id.to_string(),
                "closed": true,
                "commit": commit,
            })
        );
    }
    Ok(())
}

pub fn doctor(ctx: &Ctx) -> Result<bool> {
    let mut problems = 0usize;
    let mut report = |line: String| {
        problems += 1;
        println!("{line}");
    };

    for project in ctx.depot.projects() {
        for record in ctx.depot.list(&project) {
            match &record.item {
                Err(error) => report(format!(
                    "invalid  {} {}\n         {error}\n         repair: docket set {}",
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
                                 repair: docket set {} --{label} '...'",
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

    for line in history_problems(ctx) {
        report(line);
    }

    if problems == 0 {
        println!(
            "docket: {}, nothing wrong",
            plural(ctx.depot.projects().len(), "project", "projects")
        );
    }
    Ok(problems == 0)
}

/// What stands between the depot and a history it can be closed against.
fn history_problems(ctx: &Ctx) -> Vec<String> {
    let mut found = Vec::new();

    let Some(repo) = git::Repo::open(&ctx.depot.root) else {
        if git::detect().is_none() {
            found.push(
                "ungit    git is not on PATH, so the depot has no history\n         \
                 and no item can be closed"
                    .to_owned(),
            );
        } else if ctx.depot.root.is_dir() {
            found.push(format!(
                "unversioned {} has no repository yet\n         \
                 the next command that writes will create one",
                ctx.depot.root.display()
            ));
        }
        return found;
    };

    for nested in repo.nested_repositories() {
        found.push(format!(
            "nested   {} is a repository of its own\n         \
             git records it as a link rather than as content, so nothing under it has history",
            nested.display()
        ));
    }

    // The shelf is retired by the first repository the depot gets. One that
    // survived was left by a machine that had no git when items were closed.
    for project in ctx.depot.projects() {
        let shelf = ctx.depot.project_dir(&project).join("archive");
        if shelf.is_dir() {
            found.push(format!(
                "legacy   {} is a retired archive shelf\n         \
                 nothing lists it: move what it holds back, or delete it",
                shelf.display()
            ));
        }
    }

    found
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
    // The one read that writes. Announcing brackets a session at its opening,
    // which is the cheapest moment to record what the last one left behind, and
    // the only moment guaranteed to arrive. Silent throughout, and never the
    // thing that brings a depot into being: a machine that has never opened an
    // item gets no repository from a session starting.
    if ctx.depot.root.is_dir()
        && let Some(lock) = ctx.depot.try_lock_depot()
    {
        if let Ok(Some(repo)) = git::Repo::ensure(&ctx.depot.root) {
            let _ = snapshot_drift(&repo);
        }
        drop(lock);
    }

    let records = ctx.depot.list(&ctx.project);
    if records.is_empty() {
        return Ok(());
    }

    let rows: Vec<_> = records
        .iter()
        .enumerate()
        .map(|(index, record)| render::row(index + 1, record))
        .collect();
    let (lines, head) = render::aligned(&rows, "  ");
    let pad = " ".repeat(head);
    let invalid = records.iter().filter(|r| r.item.is_err()).count();

    let mut out = format!(
        "{} on the docket (see: docket):\n",
        plural(records.len(), "item", "items")
    );
    for (line, row) in lines.iter().zip(&rows) {
        out.push_str(line);
        out.push('\n');
        for note in &row.notes {
            out.push_str(&format!("{pad}{note}\n"));
        }
    }
    out.push_str("Items ordered, top normally first.\n");
    if invalid > 0 {
        out.push_str(&format!(
            "{} (see: docket doctor)\n",
            plural(invalid, "invalid item", "invalid items")
        ));
    }
    out.push_str("See: docket guide handoff|relay|spec\n");

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

pub fn guide_topic(args: &GuideArgs) -> Result<()> {
    println!("{}", guide::render(&args.topics)?);
    Ok(())
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
    let project = crate::store::project_key(&match &global.project {
        Some(path) => path.clone(),
        None => std::env::current_dir().context("reading the working directory")?,
    });
    Ok(Ctx {
        depot,
        format,
        color: ui::use_color(global.color, format),
        quiet: global.quiet,
        project,
    })
}
