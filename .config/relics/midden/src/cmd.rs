use camino::{Utf8Path, Utf8PathBuf};
use std::collections::BTreeMap;
use std::io::Read;

use anyhow::{Context, Result, anyhow, bail};

use crate::cli::{
    CompletionArgs, DigestArgs, FileArgs, GcArgs, Global, GuideArgs, HelpArgs, IdArgs, ListArgs,
    ResolveArgs, SetArgs,
};
use crate::field;
use crate::guide;
use crate::help;
use crate::id::Id;
use crate::note::{Note, Status, fingerprint, now, tidy_target};
use crate::render::{self, Digest, Group, NO_TARGET, View};
use crate::store::{Corpus, OPEN_CEILING, Record};
use relic_core::fmt::age_days;
use relic_core::ui::Format;

pub struct Ctx {
    pub corpus: Corpus,
    pub format: Format,
    pub color: bool,
    pub quiet: bool,
    /// The project a new note is filed against.
    pub project: Utf8PathBuf,
    /// The project a listing is narrowed to, when --project was given.
    pub scope: Option<Utf8PathBuf>,
}

impl Ctx {
    fn say(&self, message: &str) {
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

fn within(days: Option<i64>, at: jiff::Timestamp, now: jiff::Timestamp) -> bool {
    days.is_none_or(|limit| age_days(at, now) <= limit)
}

pub fn list(ctx: &Ctx, args: &ListArgs) -> Result<()> {
    let wanted = if args.status.is_some() {
        args.status
    } else if args.all || args.archived {
        None
    } else {
        Some(Status::Open)
    };

    let now = jiff::Timestamp::now();
    let mut records = ctx.corpus.list(args.archived);
    records.retain(|record| match &record.note {
        Ok(note) => {
            !args.invalid
                && args.kind.is_none_or(|kind| kind == note.kind)
                && wanted.is_none_or(|status| status == note.status)
                && within(args.since, note.updated, now)
                && ctx
                    .scope
                    .as_ref()
                    .is_none_or(|scope| *scope == note.project)
        }
        // A note that will not parse has no fields to filter on, so it shows
        // whenever nothing was filtered — never silently, and never wrongly.
        Err(_) => {
            args.invalid
                || (args.kind.is_none()
                    && args.status.is_none()
                    && args.since.is_none()
                    && ctx.scope.is_none())
        }
    });

    render::list(
        &View {
            scope: ctx.scope.as_deref(),
            records: &records,
            color: ctx.color,
            now: jiff::Timestamp::now(),
        },
        ctx.format,
    )
}

pub fn file(ctx: &Ctx, args: &FileArgs) -> Result<()> {
    let project = match &args.to {
        Some(path) => relic_core::path::project_key(path)?,
        None => ctx.project.clone(),
    };

    let title = field::one_line("--title", &text(&args.title)?, field::TITLE_MAX)?;
    let detail = match &args.detail {
        Some(raw) => field::optional("--detail", &text(raw)?, field::DETAIL_MAX)?,
        None => None,
    };
    let target = match &args.target {
        Some(raw) => field::optional("--target", &text(raw)?, field::TARGET_MAX)?
            .map(|value| tidy_target(&value)),
        None => None,
    };
    let body = match &args.body {
        Some(raw) => field::body(&text(raw)?)?,
        None => String::new(),
    };

    let print = fingerprint(args.kind, target.as_deref(), &title);
    let at = now();

    if let Some(existing) = ctx.corpus.by_fingerprint(&print) {
        let folded = ctx.corpus.bump(&existing, at)?;
        return report(ctx, &folded, &existing.path, Filed::Folded);
    }

    let cwd = relic_core::path::cwd()?;
    let note = Note {
        id: ctx.corpus.mint_id(),
        kind: args.kind,
        title,
        detail,
        target,
        status: Status::Open,
        occurrences: 1,
        project,
        cwd: Some(cwd.clone()),
        branch: relic_core::git::detect().and_then(|git| git.branch(&cwd)),
        session: std::env::var("CLAUDE_CODE_SESSION_ID")
            .ok()
            .filter(|value| !value.is_empty()),
        created: at,
        updated: at,
        seen: vec![at],
        fingerprint: print,
    };

    let path = ctx.corpus.create(&note, &body)?;
    report(ctx, &note, &path, Filed::New)?;
    Ok(())
}

#[derive(Clone, Copy)]
enum Filed {
    New,
    Folded,
}

fn report(ctx: &Ctx, note: &Note, path: &Utf8Path, how: Filed) -> Result<()> {
    if ctx.format == Format::Json {
        let mut value = render::json::note_json(note);
        if let Some(map) = value.as_object_mut() {
            map.insert("path".into(), serde_json::json!(path));
            map.insert(
                "folded".into(),
                serde_json::json!(matches!(how, Filed::Folded)),
            );
        }
        println!("{}", serde_json::to_string_pretty(&value)?);
        return Ok(());
    }
    println!("{}", note.id);
    match how {
        Filed::New => ctx.say(&format!("midden: filed {} — {}", note.id, note.title)),
        Filed::Folded => ctx.say(&format!(
            "midden: folded into {}, seen {} times — {}",
            note.id, note.occurrences, note.title
        )),
    }
    Ok(())
}

pub fn show(ctx: &Ctx, args: &IdArgs) -> Result<()> {
    let record = ctx.corpus.find(parse_id(&args.id)?)?;
    let body = crate::store::read_body(&record.path)?;
    if body.trim().is_empty() {
        ctx.say(&format!("midden: {} carries no evidence", record.id));
        return Ok(());
    }
    print!("{body}");
    Ok(())
}

pub fn path(ctx: &Ctx, args: &IdArgs) -> Result<()> {
    let record = ctx.corpus.find(parse_id(&args.id)?)?;
    println!("{}", record.path);
    Ok(())
}

pub fn set(ctx: &Ctx, args: &SetArgs) -> Result<()> {
    let record = ctx.corpus.find(parse_id(&args.id)?)?;
    // A note whose metadata stopped parsing is repaired from what can still be
    // read, so `set` is the way back rather than a second failure.
    let mut note = match record.note.clone() {
        Ok(note) => note,
        Err(error) => bail!(
            "{} will not parse: {error}\n\
             repair it in {} — midden help metadata lists the keys",
            record.id,
            record.path
        ),
    };

    let mut refile = false;
    if let Some(kind) = args.kind {
        note.kind = kind;
        refile = true;
    }
    if let Some(raw) = &args.title {
        note.title = field::one_line("--title", &text(raw)?, field::TITLE_MAX)?;
        refile = true;
    }
    if args.clear_detail {
        note.detail = None;
    } else if let Some(raw) = &args.detail {
        note.detail = field::optional("--detail", &text(raw)?, field::DETAIL_MAX)?;
    }
    if args.clear_target {
        note.target = None;
        refile = true;
    } else if let Some(raw) = &args.target {
        note.target = field::optional("--target", &text(raw)?, field::TARGET_MAX)?
            .map(|value| tidy_target(&value));
        refile = true;
    }

    if refile {
        note.refingerprint();
        if let Some(other) = ctx.corpus.by_fingerprint(&note.fingerprint)
            && other.id != note.id
        {
            bail!(
                "that claim is already {}. Fold them by hand: midden archive {}",
                other.id,
                note.id
            );
        }
    }

    note.updated = now();

    if let Some(raw) = &args.body {
        let body = field::body(&text(raw)?)?;
        let rendered = crate::store::render(&note, &body)?;
        fs_err::write(&record.path, rendered)
            .with_context(|| format!("writing {}", record.path))?;
    } else {
        ctx.corpus.save(&record, &note)?;
    }

    println!("{}", note.id);
    ctx.say(&format!("midden: updated {} — {}", note.id, note.title));
    Ok(())
}

pub fn resolve(ctx: &Ctx, args: &ResolveArgs) -> Result<()> {
    let record = ctx.corpus.find(parse_id(&args.id)?)?;
    let mut note = record
        .note
        .clone()
        .map_err(|error| anyhow!("{} will not parse: {error}", record.id))?;

    note.status = if args.actioned {
        Status::Actioned
    } else if args.dismissed {
        Status::Dismissed
    } else if args.reopen {
        Status::Open
    } else {
        bail!("say which: --actioned, --dismissed or --reopen");
    };
    note.updated = now();
    ctx.corpus.save(&record, &note)?;

    println!("{}", note.id);
    ctx.say(&format!("midden: {} is {}", note.id, note.status));
    Ok(())
}

pub fn archive(ctx: &Ctx, args: &IdArgs) -> Result<()> {
    let record = ctx.corpus.find(parse_id(&args.id)?)?;
    if record.archived {
        bail!("{} is already archived", record.id);
    }
    let target = ctx.corpus.archive(&record)?;
    println!("{target}");
    ctx.say(&format!("midden: archived {}", record.id));
    Ok(())
}

pub fn digest(ctx: &Ctx, args: &DigestArgs) -> Result<()> {
    let now = jiff::Timestamp::now();
    let records: Vec<Record> = ctx
        .corpus
        .list(false)
        .into_iter()
        .filter(|record| match &record.note {
            Ok(note) => {
                note.status == Status::Open
                    && args.kind.is_none_or(|kind| kind == note.kind)
                    && within(args.since, note.updated, now)
                    && ctx
                        .scope
                        .as_ref()
                        .is_none_or(|scope| *scope == note.project)
            }
            Err(_) => false,
        })
        .collect();

    let mut by_target: BTreeMap<String, Vec<&Record>> = BTreeMap::new();
    for record in &records {
        let target = record
            .note
            .as_ref()
            .ok()
            .and_then(|note| note.target.clone())
            .unwrap_or_else(|| NO_TARGET.to_owned());
        by_target.entry(target).or_default().push(record);
    }

    let mut groups: Vec<Group<'_>> = by_target
        .into_iter()
        .map(|(target, records)| Group { target, records })
        .collect();
    // Heaviest first, so the file that has cost the most sessions the most time
    // is the one a reader opens first. Notes with nowhere to land sink to the
    // bottom whatever their weight — they are not yet a worklist. Ties break
    // alphabetically, which keeps two runs over an unchanged corpus identical.
    groups.sort_by_key(|group| {
        (
            group.target == NO_TARGET,
            std::cmp::Reverse(group.weight()),
            group.target.clone(),
        )
    });

    render::digest(
        &Digest {
            scope: ctx.scope.as_deref(),
            groups: &groups,
            color: ctx.color,
            now,
        },
        ctx.format,
    )
}

pub fn gc(ctx: &Ctx, args: &GcArgs) -> Result<()> {
    let swept = ctx.corpus.sweep(args.dry_run)?;
    if ctx.format == Format::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "dry_run": args.dry_run,
                "swept": swept
                    .iter()
                    .map(|entry| serde_json::json!({
                        "id": entry.id.to_string(),
                        "title": entry.title,
                        "status": entry.status.to_string(),
                        "idle_days": entry.idle,
                        "action": entry.action.as_str(),
                    }))
                    .collect::<Vec<_>>(),
            }))?
        );
        return Ok(());
    }

    if swept.is_empty() {
        ctx.say("midden: nothing to sweep");
        return Ok(());
    }
    for entry in &swept {
        println!(
            "{:<9} {}  {} — quiet {}d as {}",
            entry.action.as_str(),
            entry.id,
            entry.title,
            entry.idle,
            entry.status
        );
    }
    if args.dry_run {
        ctx.say(&format!(
            "midden: {} would go; nothing was changed",
            swept.len()
        ));
    }
    Ok(())
}

pub fn doctor(ctx: &Ctx) -> Result<bool> {
    let mut problems = 0usize;
    let mut report = |line: String| {
        problems += 1;
        println!("{line}");
    };

    let live = ctx.corpus.list(false);
    let mut claims: BTreeMap<&str, Id> = BTreeMap::new();
    let mut open = 0usize;

    for record in &live {
        match &record.note {
            Err(error) => report(format!(
                "invalid  {} will not parse: {error}\n         {}",
                record.id, record.path
            )),
            Ok(note) => {
                if note.status == Status::Open {
                    open += 1;
                }
                if let Some((label, length, max)) = note.overlong() {
                    report(format!(
                        "overlong {} has a {label} of {length} characters, over the limit of {max}\n         \
                         repair: midden set {} --{label} '...'",
                        record.id, record.id
                    ));
                }
                let expected = fingerprint(note.kind, note.target.as_deref(), &note.title);
                if expected != note.fingerprint {
                    report(format!(
                        "drifted  {} carries a fingerprint that no longer matches its claim\n         \
                         repair: midden set {} --title '{}'",
                        record.id, record.id, note.title
                    ));
                }
                match claims.get(note.fingerprint.as_str()) {
                    Some(first) => report(format!(
                        "doubled  {} claims the same cause as {first}\n         \
                         repair: midden archive {}",
                        record.id, record.id
                    )),
                    None => {
                        claims.insert(note.fingerprint.as_str(), record.id);
                    }
                }
                if let Some(target) = note.target.as_deref()
                    && let Some(missing) = absent_target(target)
                {
                    report(format!(
                        "moved    {} points at {missing}, which is not there\n         \
                         repair: midden set {} --target '...'",
                        record.id, record.id
                    ));
                }
            }
        }
    }

    if open > OPEN_CEILING {
        report(format!(
            "swollen  {open} open notes, over the ceiling of {OPEN_CEILING}\n         \
             drain it: midden digest"
        ));
    }

    if problems == 0 {
        println!(
            "midden: {}, nothing wrong",
            relic_core::fmt::plural(live.len(), "note", "notes")
        );
    }
    Ok(problems == 0)
}

/// A target that names a path on this machine and is not there any more. Only
/// paths are checked: a target may equally name a mode, a skill or a section.
fn absent_target(target: &str) -> Option<String> {
    if !(target.starts_with('~') || target.starts_with('/')) {
        return None;
    }
    // A target may point into a file, so only the leading path is tested.
    let head = target.split_whitespace().next().unwrap_or(target);
    let resolved = relic_core::path::resolve_lenient(Utf8Path::new(head)).ok()?;
    (!resolved.exists()).then(|| head.to_owned())
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
    clap_complete::generate(args.shell, command, "midden", &mut std::io::stdout().lock());
    Ok(())
}

pub fn open_context(global: &Global) -> Result<Ctx> {
    let corpus = Corpus::open()?;
    let format = if global.json {
        Format::Json
    } else {
        Format::from_process(global.format, "MIDDEN_UI")
    };
    let cwd = relic_core::path::cwd()?;
    Ok(Ctx {
        corpus,
        format,
        color: global.color.use_color(format),
        quiet: global.quiet,
        project: relic_core::path::project_key(&cwd)?,
        scope: global
            .project
            .as_ref()
            .map(|path| relic_core::path::project_key(path))
            .transpose()?,
    })
}
