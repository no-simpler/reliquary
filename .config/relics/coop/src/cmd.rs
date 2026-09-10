//! Dispatch, and what each command does.

use anyhow::{Context, Result};
use jiff::Timestamp;
use relic_core::finding::Finding;
use relic_core::ui::Format;

use crate::ask::{self, State};
use crate::card;
use crate::cli::{CompletionArgs, Global, RefreshArgs, ShowArgs, TopicArgs};
use crate::config::{self, Config, Style};
use crate::doctor;
use crate::notice::{self, Notice};
use crate::paths::{self, Paths};
use crate::render;
use crate::session;
use crate::source::{self, Broken, Source, Tier};
use crate::state::{FirstSeen, Timing};

/// Everything a command needs that is not its own arguments.
pub struct Ctx {
    /// Where things live.
    pub paths: Paths,
    /// How the card is drawn.
    pub style: Style,
    /// The output shape.
    pub format: Format,
    /// Whether to paint.
    pub color: bool,
    /// Whether to say only what was asked for.
    pub quiet: bool,
    /// One reading of the clock for the whole command.
    pub now: Timestamp,
}

/// What one sweep of the declarations found.
pub struct Gathered {
    /// Every declaration that compiled.
    pub sources: Vec<Source>,
    /// Everything outstanding, worst first.
    pub notices: Vec<Notice>,
    /// Declarations that would not compile.
    pub broken: Vec<Broken>,
    /// Every asked source and what it is doing.
    pub states: Vec<(String, State)>,
    /// Whether this sweep paid for a cold producer.
    pub warmed: bool,
}

/// Build the context.
///
/// # Errors
///
/// When the trees cannot be resolved or the config will not parse.
pub fn open_context(global: &Global) -> Result<Ctx> {
    let paths = Paths::resolve()?;
    let config = Config::read(&paths.config())?;
    let format = if global.json {
        Format::Json
    } else {
        Format::from_process(global.format, paths::UI_VAR)
    };
    Ok(Ctx {
        style: global.style.unwrap_or(config.style),
        color: global.color.use_color(format),
        format,
        quiet: global.quiet,
        now: Timestamp::now(),
        paths,
    })
}

/// Read every declaration and work out what is outstanding.
///
/// `blocking` decides what happens to a source that has never answered on this
/// machine: the tick pays for it once, and a command that is merely reporting
/// does not.
///
/// # Errors
///
/// When the drop-in directory cannot be listed.
pub fn gather(ctx: &Ctx, blocking: bool) -> Result<Gathered> {
    let (sources, broken) = source::read_dir(&ctx.paths.sources_dir())?;
    let mut findings: Vec<Notice> = Vec::new();
    let mut states = Vec::new();
    let mut warmed = false;

    for source in &sources {
        match &source.tier {
            Tier::When(predicate) => {
                if let Some(vars) = predicate.eval(ctx.now) {
                    // Nothing to age means the path was never there, which the
                    // declaration may have its own words for.
                    let template = match (&vars.age, &source.summary_missing) {
                        (None, Some(missing)) => missing,
                        _ => &source.summary,
                    };
                    findings.push(stat_notice(source, &vars.apply(template)));
                }
            }
            Tier::Ask(ask) => {
                let (cached, state) = ask::peek(&ctx.paths, source, ask, ctx.now);
                let cached = match state {
                    State::Cold if blocking => {
                        warmed = true;
                        warm(ctx, source, ask).unwrap_or(cached)
                    }
                    State::Stale => {
                        nudge(source.id.as_str());
                        cached
                    }
                    State::Cold | State::Fresh | State::Dormant => cached,
                };
                states.push((source.id.as_str().to_owned(), state));
                findings.extend(cached.into_iter().map(|finding| Notice {
                    source: source.id.as_str().to_owned(),
                    finding,
                }));
            }
        }
    }
    Ok(Gathered {
        notices: notice::actionable(findings),
        sources,
        broken,
        states,
        warmed,
    })
}

fn stat_notice(source: &Source, summary: &str) -> Notice {
    let mut finding = source.id.finds(
        source.severity,
        relic_core::finding::Summary::lossy(summary),
    );
    if let Some(fix) = source.fix.clone() {
        finding = finding.fixed_by(fix);
    }
    Notice {
        source: source.id.as_str().to_owned(),
        finding,
    }
}

/// A source nobody has ever asked. Once per machine, under a hard cap.
fn warm(ctx: &Ctx, source: &Source, ask: &crate::source::Ask) -> Option<Vec<Finding>> {
    let _lock = ask::claim(&ctx.paths, source.id.as_str()).ok()??;
    let report = ask::refresh(&ctx.paths, source, ask, ctx.now, ask::COLD_BUDGET).ok()?;
    Some(report.findings().to_vec())
}

/// A source whose answer has gone off. Somebody else may already be on it.
fn nudge(id: &str) {
    let _ = ask::spawn_detached(id);
}

/// The prompt hook.
///
/// Draws on an edge and never otherwise, writes the badge, and returns zero
/// whatever happened. Nothing here is allowed to make a shell wait, print a
/// diagnostic into a prompt, or fail.
pub fn tick(ctx: &Ctx) -> Result<()> {
    // Only a person reads a card. Off a terminal, and under an agent harness,
    // the format resolves away from Human and the tick is a no-op.
    if ctx.format != Format::Human || std::env::var_os(paths::DISABLE_VAR).is_some() {
        return Ok(());
    }
    let started = std::time::Instant::now();
    let Ok(gathered) = gather(ctx, true) else {
        return Ok(());
    };
    let digest = notice::digest(&gathered.notices);

    write_badge(ctx, &gathered.notices);

    let mut drew = false;
    let session = session::from_env();
    let shown = session
        .as_deref()
        .and_then(|session| session::seen(&ctx.paths, session));
    if shown.as_deref() != Some(digest.as_str()) {
        drew = true;
        if let Some(text) = card::draw(&gathered.notices, width(ctx), ctx.style, ctx.color) {
            println!("{text}");
        }
        if let Some(session) = session.as_deref() {
            let _ = session::mark(&ctx.paths, session, &digest);
        }
        let _ = FirstSeen::update(&ctx.paths.first_seen(), &gathered.notices, ctx.now);
    }

    // The budget measures the path a prompt actually takes: silent, off a warm
    // cache. Warming a cold producer measures that producer, and drawing pays
    // for three writes on an edge — neither is the steady state, and timing
    // either would report a hook that is slow when it is not.
    if !gathered.warmed && !drew {
        let micros = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        Timing::record(&ctx.paths.timing(), micros, ctx.now);
    }
    Ok(())
}

fn write_badge(ctx: &Ctx, notices: &[Notice]) {
    let text = card::badge(notices).unwrap_or_default();
    let path = ctx.paths.badge();
    // Unchanged is the common case, and a write per prompt is exactly the cost
    // the badge exists to avoid.
    if fs_err::read_to_string(&path).ok().as_deref() == Some(text.as_str()) {
        return;
    }
    if let Some(parent) = path.parent() {
        let _ = fs_err::create_dir_all(parent);
    }
    let _ = relic_core::fs::write_atomic(&path, &text);
}

fn width(ctx: &Ctx) -> usize {
    let config = Config::read(&ctx.paths.config()).unwrap_or_default();
    config.width(config::terminal_width())
}

/// The card, on demand, regardless of what this shell has already been shown.
///
/// # Errors
///
/// When the declarations cannot be read.
pub fn show_card(ctx: &Ctx) -> Result<()> {
    let gathered = gather(ctx, true)?;
    if ctx.format != Format::Human {
        return render::list(ctx, &gathered.notices);
    }
    match card::draw(&gathered.notices, width(ctx), ctx.style, ctx.color) {
        Some(text) => println!("{text}"),
        None if !ctx.quiet => println!("the coop is empty"),
        None => {}
    }
    Ok(())
}

/// The prompt segment.
///
/// # Errors
///
/// When the declarations cannot be read.
pub fn prompt(ctx: &Ctx) -> Result<()> {
    let gathered = gather(ctx, false)?;
    if let Some(badge) = card::badge(&gathered.notices) {
        // No newline: the caller captures this into a variable.
        print!("{badge}");
    }
    Ok(())
}

/// One line per outstanding notice.
///
/// # Errors
///
/// When the declarations cannot be read.
pub fn list(ctx: &Ctx) -> Result<()> {
    let gathered = gather(ctx, true)?;
    render::list(ctx, &gathered.notices)
}

/// One notice, in full.
///
/// # Errors
///
/// When the declarations cannot be read, or nothing is outstanding under that id.
pub fn show(ctx: &Ctx, args: &ShowArgs) -> Result<bool> {
    let gathered = gather(ctx, true)?;
    let matched: Vec<&Notice> = gathered
        .notices
        .iter()
        .filter(|notice| notice.source == args.id)
        .collect();
    if matched.is_empty() {
        eprintln!("coop: nothing outstanding under {}", args.id);
        return Ok(false);
    }
    render::show(ctx, &matched)?;
    Ok(true)
}

/// Every declared source, and what it is doing.
///
/// # Errors
///
/// When the declarations cannot be read.
pub fn sources(ctx: &Ctx) -> Result<()> {
    let gathered = gather(ctx, false)?;
    render::sources(ctx, &gathered)
}

/// Run asked sources now.
///
/// # Errors
///
/// When the declarations cannot be read. A producer that fails is reported, not
/// fatal: one broken source must not stop the others being refreshed.
pub fn refresh(ctx: &Ctx, args: &RefreshArgs) -> Result<bool> {
    let (declared, _) = source::read_dir(&ctx.paths.sources_dir())?;
    let mut ok = true;
    for (source, ask) in ask::asked(&declared) {
        if args
            .source
            .as_deref()
            .is_some_and(|wanted| wanted != source.id.as_str())
        {
            continue;
        }
        let Some(_lock) = ask::claim(&ctx.paths, source.id.as_str())? else {
            continue;
        };
        match ask::refresh(&ctx.paths, source, ask, ctx.now, ask::BACKGROUND_BUDGET) {
            Ok(report) => {
                if !ctx.quiet && ctx.format == Format::Human {
                    println!(
                        "{}: {}",
                        source.id,
                        relic_core::fmt::plural(report.findings().len(), "finding", "findings")
                    );
                }
            }
            Err(error) => {
                ok = false;
                eprintln!("coop: {}: {error:#}", source.id);
            }
        }
    }
    let _ = session::sweep(&ctx.paths, ctx.now);
    let gathered = gather(ctx, false)?;
    let _ = FirstSeen::update(&ctx.paths.first_seen(), &gathered.notices, ctx.now);
    Ok(ok)
}

/// What coop knows about its own health.
///
/// # Errors
///
/// When the declarations cannot be read.
pub fn run_doctor(ctx: &Ctx) -> Result<u8> {
    let gathered = gather(ctx, false)?;
    let report = doctor::report(ctx, &gathered);
    if ctx.format == Format::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).context("serialising the report")?
        );
    } else {
        println!("{}", doctor::render(&report, ctx.color));
    }
    Ok(report.grade().exit_code())
}

/// Reference topics.
///
/// # Errors
///
/// When long help cannot be rendered.
pub fn help_topic(args: &TopicArgs, command: &mut clap::Command) -> Result<()> {
    if args.topics.is_empty() {
        println!("{}", command.render_long_help());
        return Ok(());
    }
    if args.topics.iter().any(|topic| topic == "topics") {
        for name in crate::help::topic_names() {
            println!("{name}");
        }
        return Ok(());
    }
    print!("{}", crate::help::render(&args.topics));
    Ok(())
}

/// Doctrine.
///
/// # Errors
///
/// Never; the signature matches its sibling.
pub fn guide_topic(args: &TopicArgs) -> Result<()> {
    print!("{}", crate::guide::render(&args.topics));
    Ok(())
}

/// Shell completions.
///
/// # Errors
///
/// Never; the signature matches its siblings.
pub fn completions(args: &CompletionArgs, command: &mut clap::Command) -> Result<()> {
    clap_complete::generate(args.shell, command, "coop", &mut std::io::stdout());
    Ok(())
}
