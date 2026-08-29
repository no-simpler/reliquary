//! One function per command.

use std::io::Read;

use anyhow::{Context, Result, anyhow, bail};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Command as ClapCommand;
use fs_err as fs;

use relic_core::ui::Format;

use crate::cli::{CompletionArgs, DryRunArgs, ExplainArgs, GcArgs, Global, TopicArgs};
use crate::hook::{self, Env};
use crate::inject::{self, Block, Occasion};
use crate::mode::{self, Mode, Read as ReadMode};
use crate::render::{self, Clause, Explain, ModeRow};
use crate::schedule;
use crate::state;
use crate::{guide, help, resolve};

/// Everything a command needs that is not its own arguments.
pub struct Ctx {
    /// How to print.
    pub format: Format,
    /// Whether the shape and the environment both allow colour.
    pub color: bool,
    /// Where state lives.
    pub root: Utf8PathBuf,
    /// Every place a mode may be, in order.
    pub roots: Vec<Utf8PathBuf>,
}

/// Resolves the machine before any command that reads it runs.
///
/// # Errors
///
/// When `HOME` is unset or unspellable, or the project override is not a path.
pub fn open(global: &Global) -> Result<Ctx> {
    let format = if global.json {
        Format::Json
    } else {
        Format::from_process(global.format, "MANTRA_UI")
    };
    let home = relic_core::path::home().ok_or_else(|| anyhow!("HOME is unset or not UTF-8"))?;
    let project = match &global.project {
        Some(path) => Some(path.clone()),
        None => Some(relic_core::path::cwd()?),
    };
    Ok(Ctx {
        format,
        color: global.color.use_color(format),
        root: state::root()?,
        roots: resolve::roots(&home, project.as_deref()),
    })
}

/// Every mode a `+token` can reach, readable or not.
fn corpus(roots: &[Utf8PathBuf]) -> Vec<ReadMode> {
    resolve::all(roots)
        .into_iter()
        .map(|(name, path)| mode::read(&name, &path))
        .collect()
}

/// # Errors
///
/// When the output cannot be written.
pub fn list(ctx: &Ctx) -> Result<()> {
    let rows: Vec<ModeRow> = corpus(&ctx.roots)
        .into_iter()
        .map(|read| match read {
            Ok(mode) => ModeRow {
                name: mode.name,
                triggers: mode.triggers.iter().map(|t| t.label()).collect(),
                refrain: mode.refrain,
                path: mode.path.into_string(),
                broken: None,
            },
            Err(broken) => ModeRow {
                name: broken.name,
                triggers: Vec::new(),
                refrain: None,
                path: broken.path.into_string(),
                broken: Some(broken.why),
            },
        })
        .collect();
    render::list(ctx.format, ctx.color, &rows)
}

/// # Errors
///
/// When no session state can be found, or the output cannot be written.
pub fn explain(ctx: &Ctx, args: &ExplainArgs) -> Result<()> {
    let id = match &args.session {
        Some(id) => id.clone(),
        None => newest(&ctx.root)?,
    };
    let session = state::load(&ctx.root, &id).ok_or_else(|| {
        anyhow!(
            "no state for session {id}. mantra keeps it under {}",
            ctx.root
        )
    })?;
    let mut clauses = Vec::new();
    for active in &session.modes {
        let Some(path) = resolve::find(&active.name, &ctx.roots) else {
            clauses.push(Clause {
                mode: active.name.clone(),
                fires: active.fires,
                trigger: "—".to_owned(),
                standing: "the mode file is gone".to_owned(),
            });
            continue;
        };
        let Ok(mode) = mode::read(&active.name, &path) else {
            clauses.push(Clause {
                mode: active.name.clone(),
                fires: active.fires,
                trigger: "—".to_owned(),
                standing: "the mode file no longer reads".to_owned(),
            });
            continue;
        };
        for standing in schedule::standing(&mode, active, session.tokens) {
            clauses.push(Clause {
                mode: active.name.clone(),
                fires: active.fires,
                trigger: standing.trigger.label(),
                standing: standing.note,
            });
        }
    }
    render::explain(
        ctx.format,
        ctx.color,
        &Explain {
            session: id,
            generation: session.generation,
            turns: session.turns,
            tokens: session.tokens,
            clauses,
        },
    )
}

/// The session whose state was written last. In a live session that is this
/// one, which is what lets `mantra explain` work without a session id it has no
/// way to learn.
fn newest(root: &Utf8Path) -> Result<String> {
    let mut best: Option<(std::time::SystemTime, String)> = None;
    for entry in fs::read_dir(root)
        .with_context(|| format!("no state yet: {root} does not exist"))?
        .flatten()
    {
        let Ok(path) = relic_core::path::utf8(entry.path()) else {
            continue;
        };
        if path.extension() != Some("json") {
            continue;
        }
        let Ok(at) = entry.metadata().and_then(|m| m.modified()) else {
            continue;
        };
        let Some(id) = path.file_stem() else {
            continue;
        };
        if best.as_ref().is_none_or(|(seen, _)| at > *seen) {
            best = Some((at, id.to_owned()));
        }
    }
    best.map(|(_, id)| id)
        .ok_or_else(|| anyhow!("no session state under {root}"))
}

/// # Errors
///
/// When a token names no mode, or that mode does not read.
pub fn dry_run(ctx: &Ctx, args: &DryRunArgs) -> Result<()> {
    let mut modes: Vec<Mode> = Vec::new();
    for name in &args.tokens {
        let path = resolve::find(name, &ctx.roots)
            .ok_or_else(|| anyhow!("no mode called {name:?} on any search path"))?;
        match mode::read(name, &path) {
            Ok(mode) => modes.push(mode),
            Err(broken) => bail!("{}: {}", broken.path, broken.why),
        }
    }
    let blocks: Vec<Block<'_>> = modes
        .iter()
        .map(|mode| Block {
            name: &mode.name,
            text: mode.full(),
        })
        .collect();
    if let Some(text) = inject::render(&[(Occasion::Activate, blocks)]) {
        print!("{text}");
    }
    Ok(())
}

/// Answers one hook. Reads stdin, writes at most one JSON envelope, and never
/// fails: this writes into a model's context, where an error message would be
/// read as an instruction.
///
/// # Errors
///
/// Never. The signature matches its siblings so the dispatch table stays one
/// shape.
pub fn hook() -> Result<()> {
    let mut input = String::new();
    if std::io::stdin().read_to_string(&mut input).is_err() {
        return Ok(());
    }
    let Ok(home) = relic_core::path::home().ok_or(()) else {
        return Ok(());
    };
    let Ok(root) = state::root() else {
        return Ok(());
    };
    let Some(injection) = hook::run(&input, &Env { home, root }) else {
        return Ok(());
    };
    let envelope = serde_json::json!({
        "hookSpecificOutput": {
            "hookEventName": injection.event,
            "additionalContext": injection.context,
        }
    });
    println!("{envelope}");
    Ok(())
}

/// # Errors
///
/// When the state directory cannot be read.
pub fn gc(ctx: &Ctx, args: &GcArgs) -> Result<()> {
    if args.dry_run {
        let stale = state::stale(&ctx.root, args.days)?;
        for id in &stale {
            println!("{id}");
        }
        println!(
            "{} of {} sessions are stale",
            stale.len(),
            state::count(&ctx.root)
        );
        return Ok(());
    }
    let swept = state::gc(&ctx.root, args.days)?;
    println!("swept {swept}");
    Ok(())
}

/// Reports whether the machine can do what it says. Read-only.
///
/// # Errors
///
/// When the output cannot be written.
pub fn doctor(ctx: &Ctx) -> Result<bool> {
    let mut sound = true;
    let mut say = |ok: bool, line: &str| {
        sound &= ok;
        println!("{} {line}", if ok { "ok  " } else { "FAIL" });
    };

    let read = corpus(&ctx.roots);
    let broken: Vec<_> = read.iter().filter_map(|r| r.as_ref().err()).collect();
    say(
        broken.is_empty(),
        &format!("{} modes read, {} do not", read.len(), broken.len()),
    );
    for one in broken {
        println!("     {}: {}", one.path, one.why);
    }

    for (event, wired) in wiring() {
        say(wired, &format!("{event} is wired to mantra"));
    }

    let writable = ctx.root.is_dir() || ctx.root.parent().is_some_and(Utf8Path::is_dir);
    say(
        writable,
        &format!("state directory {} is reachable", ctx.root),
    );
    Ok(sound)
}

/// Which hook events name mantra in the settings this machine reads.
///
/// Fails **open**: a settings file that cannot be read or parsed is reported as
/// wired, because a doctor that fails on an unreadable file teaches its reader
/// to ignore it.
fn wiring() -> Vec<(&'static str, bool)> {
    const EVENTS: [&str; 3] = ["SessionStart", "UserPromptSubmit", "PostToolBatch"];
    let settings = relic_core::path::home()
        .map(|home| home.join(".claude").join("settings.json"))
        .and_then(|path| fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok());
    let Some(settings) = settings else {
        return EVENTS.into_iter().map(|event| (event, true)).collect();
    };
    EVENTS
        .into_iter()
        .map(|event| {
            let wired = settings
                .pointer(&format!("/hooks/{event}"))
                .is_some_and(|node| node.to_string().contains("mantra"));
            (event, wired)
        })
        .collect()
}

/// # Errors
///
/// When a topic is not one this knows.
pub fn help_topic(args: &TopicArgs, command: &mut ClapCommand) -> Result<()> {
    let Some(name) = args.topics.first() else {
        command.print_long_help()?;
        return Ok(());
    };
    match help::topic(name) {
        Some(body) => println!("{body}"),
        None => bail!(
            "no help topic called {name:?}. Topics: {}",
            help::topic_names()
        ),
    }
    Ok(())
}

/// # Errors
///
/// When a topic is not one this knows.
pub fn guide_topic(args: &TopicArgs) -> Result<()> {
    println!("{}", guide::render(&args.topics)?);
    Ok(())
}

/// # Errors
///
/// Never; the signature matches its siblings.
pub fn completions(args: &CompletionArgs, command: &mut ClapCommand) -> Result<()> {
    clap_complete::generate(args.shell, command, "mantra", &mut std::io::stdout());
    Ok(())
}
