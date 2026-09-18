//! Dispatch, and what each command does.

use anyhow::{Context, Result};
use jiff::Timestamp;
use relic_core::ui::Format;

use crate::ask::{self, Answer, Standing};
use crate::card;
use crate::cli::{CompletionArgs, Global, ShowArgs, TopicArgs};
use crate::config::{self, Config, Style};
use crate::doctor;
use crate::notice::{self, Notice};
use crate::paths::{self, Paths};
use crate::records::FirstSeen;
use crate::render;
use crate::source::{self, Broken, Source, Tier};

/// Everything a command needs that is not its own arguments.
pub struct Ctx {
    /// Where things live.
    pub paths: Paths,
    /// Rendering preferences, read once.
    pub config: Config,
    /// How the card is drawn.
    pub style: Style,
    /// The output shape.
    pub format: Format,
    /// Whether to paint, carried as the thing that spends it.
    pub paint: relic_core::style::Style,
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
    /// Every asked or read source, and how it went.
    pub standings: Vec<(String, Standing, Option<String>)>,
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
        paint: global.color.style(format),
        format,
        quiet: global.quiet,
        now: Timestamp::now(),
        config,
        paths,
    })
}

/// Read every declaration and work out what is outstanding.
///
/// # Errors
///
/// When the drop-in directory cannot be listed.
pub fn gather(ctx: &Ctx) -> Result<Gathered> {
    let (sources, broken) = source::read_dir(&ctx.paths.sources_dir())?;
    let mut findings: Vec<Notice> = Vec::new();
    let mut standings = Vec::new();

    for source in &sources {
        let answer = match &source.tier {
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
                continue;
            }
            Tier::Ask(ask) => ask::run(source, ask),
            Tier::Read(read) => ask::read(source, read),
        };
        let Answer {
            findings: said,
            standing,
            why,
        } = answer;
        standings.push((source.id.as_str().to_owned(), standing, why));
        findings.extend(said.into_iter().map(|finding| Notice {
            source: source.id.as_str().to_owned(),
            finding,
        }));
    }
    Ok(Gathered {
        notices: notice::actionable(findings),
        sources,
        broken,
        standings,
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

/// The prompt hook.
///
/// Draws on an edge and never otherwise, writes the badge, and returns zero
/// whatever happened. Nothing here is allowed to print a diagnostic into a
/// prompt, or fail.
pub fn tick(ctx: &Ctx) -> Result<()> {
    // Only a person reads a card. Off a terminal, and under an agent harness,
    // the format resolves away from Human and the tick is a no-op.
    if ctx.format != Format::Human || std::env::var_os(paths::DISABLE_VAR).is_some() {
        return Ok(());
    }
    let Ok(gathered) = gather(ctx) else {
        return Ok(());
    };
    let digest = notice::digest(&gathered.notices);

    write_badge(ctx, &gathered.notices, &digest);

    // The shell keeps the digest it was last shown, in its own environment,
    // for exactly as long as it lives.
    let seen = std::env::var(paths::SEEN_VAR).ok();
    if seen.as_deref() != Some(digest.as_str()) {
        if let Some(text) = card::draw(&gathered.notices, width(ctx), ctx.style, ctx.paint) {
            println!("{text}");
        }
        let _ = FirstSeen::update(&ctx.paths.first_seen(), &gathered.notices, ctx.now);
    }
    Ok(())
}

/// The badge file: the count and the digest, or nothing at all.
fn write_badge(ctx: &Ctx, notices: &[Notice], digest: &str) {
    let text = card::badge(notices).map_or_else(String::new, |count| format!("{count} {digest}"));
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
    ctx.config.width(config::terminal_width())
}

/// The card, on demand, whatever this shell has already been shown.
///
/// # Errors
///
/// When the declarations cannot be read.
pub fn show_card(ctx: &Ctx) -> Result<()> {
    let gathered = gather(ctx)?;
    if ctx.format != Format::Human {
        return render::list(ctx, &gathered.notices);
    }
    match card::draw(&gathered.notices, width(ctx), ctx.style, ctx.paint) {
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
    let gathered = gather(ctx)?;
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
    let gathered = gather(ctx)?;
    render::list(ctx, &gathered.notices)
}

/// One notice, in full.
///
/// # Errors
///
/// When the declarations cannot be read, or nothing is outstanding under that id.
pub fn show(ctx: &Ctx, args: &ShowArgs) -> Result<bool> {
    let gathered = gather(ctx)?;
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
    let gathered = gather(ctx)?;
    render::sources(ctx, &gathered)
}

/// What coop knows about its own health.
///
/// # Errors
///
/// When the declarations cannot be read.
pub fn run_doctor(ctx: &Ctx) -> Result<u8> {
    let gathered = gather(ctx)?;
    let report = doctor::report(ctx, &gathered);
    if ctx.format == Format::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report).context("serialising the report")?
        );
    } else {
        println!("{}", doctor::render(&report, ctx.paint));
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
