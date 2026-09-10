//! What each command does: the context every command is handed, and dispatch.
//!
//! One file per family. `sitting` is the drill, `enroll` is everything that
//! takes or removes a secret, `reading` is the three tables, `health` is what
//! `assay` and the shell read.

mod enroll;
mod health;
mod reading;
mod sitting;

use anyhow::{Result, bail};
use jiff::civil::Date;
use relic_core::style::Style;
use relic_core::ui::Format;

use crate::cli::{Cli, Command, Global};
use crate::config::Config;
use crate::exit::CLEAN;
use crate::ladder::Standing;
use crate::log::{Digest, Event, Record, SCHEMA};
use crate::model::State;
use crate::render::{Table, agent, human};
use crate::store::{Cache, Clock, Env, Paths};

/// What every command is handed.
pub struct Context {
    /// The environment, read once.
    pub env: Env,
    /// The config file, or its defaults.
    pub config: Config,
    /// The two homes.
    pub paths: Paths,
    /// The clock and the drill day.
    pub clock: Clock,
    /// Output shape.
    pub format: Format,
    /// Whether to colour, carried as the thing that spends it.
    pub style: Style,
    /// Whether to print only what was asked for.
    pub quiet: bool,
}

impl Context {
    /// The drill day.
    pub fn today(&self) -> Date {
        self.clock.today()
    }
}

/// Assemble the context. The only place the process environment is read.
///
/// # Errors
///
/// When the config will not parse, or a default path cannot be resolved.
pub fn open_context(global: &Global) -> Result<Context> {
    let env = Env::from_process();
    let config = Config::read(&env.config_path()?)?;
    let paths = Paths::resolve(&env, &config)?;
    let format = if global.json {
        Format::Json
    } else {
        Format::from_process(global.format, "ROTE_UI")
    };
    let clock = Clock::new(
        jiff::Timestamp::now(),
        jiff::tz::TimeZone::system(),
        config.rollover_hour(),
    );
    Ok(Context {
        env,
        config,
        paths,
        clock,
        format,
        style: global.color.style(format),
        quiet: global.quiet,
    })
}

/// Dispatch.
///
/// # Errors
///
/// When a command cannot do what it was asked.
pub fn run(cli: &Cli) -> Result<u8> {
    // These three describe the tool rather than read the drill, so they answer
    // before there is a store to open.
    match &cli.command {
        Some(Command::Completions(args)) => {
            completions(args.shell);
            return Ok(CLEAN);
        }
        Some(Command::Help(args)) => {
            help_topic(args.topic.as_deref())?;
            return Ok(CLEAN);
        }
        Some(Command::Guide(args)) => {
            println!("{}", crate::guide::render(&args.topics)?);
            return Ok(CLEAN);
        }
        None
        | Some(
            Command::Status(_)
            | Command::Stats(_)
            | Command::Log(_)
            | Command::Practice(_)
            | Command::Add(_)
            | Command::Probe(_)
            | Command::Rekey(_)
            | Command::Retire(_)
            | Command::Doctor
            | Command::Banner,
        ) => {}
    }

    let ctx = open_context(&cli.global)?;
    match &cli.command {
        None => sitting::daily(&ctx, cli.aided),
        Some(Command::Practice(args)) => sitting::practice(&ctx, args, cli.aided),
        Some(Command::Status(args)) => reading::status(&ctx, args),
        Some(Command::Stats(args)) => reading::stats(&ctx, args),
        Some(Command::Log(args)) => reading::log(&ctx, args),
        Some(Command::Add(args)) => enroll::add(&ctx, args),
        Some(Command::Rekey(args)) => enroll::rekey(&ctx, args),
        Some(Command::Retire(args)) => enroll::retire(&ctx, args),
        Some(Command::Probe(args)) => enroll::probe(&ctx, args),
        Some(Command::Doctor) => health::doctor(&ctx),
        Some(Command::Banner) => health::banner(&ctx),
        Some(Command::Completions(_) | Command::Help(_) | Command::Guide(_)) => {
            unreachable!("handled above")
        }
    }
}

fn completions(shell: clap_complete::Shell) {
    use clap::CommandFactory as _;
    let mut command = Cli::command();
    clap_complete::generate(shell, &mut command, "rote", &mut std::io::stdout());
}

fn help_topic(topic: Option<&str>) -> Result<()> {
    use clap::CommandFactory as _;
    let mut command = Cli::command();
    let Some(name) = topic else {
        println!("{}", command.render_long_help());
        return Ok(());
    };
    if name == "topics" {
        println!("{}", crate::help::topic_names());
        return Ok(());
    }
    if let Some(body) = crate::help::topic(name) {
        println!("{body}");
        return Ok(());
    }
    if let Some(sub) = command.find_subcommand_mut(name) {
        println!("{}", sub.render_long_help());
        return Ok(());
    }
    bail!(
        "no topic or command called {name:?}. Topics: {}",
        crate::help::topic_names()
    )
}

/// One record, stamped with this run's instant, day and host. The chain digest
/// is the store's to fill in.
fn stamped(ctx: &Context, today: Date, event: Event) -> Record {
    Record {
        v: SCHEMA,
        at: ctx
            .clock
            .now()
            .round(jiff::Unit::Second)
            .unwrap_or(ctx.clock.now()),
        day: today,
        host: ctx.env.host.clone(),
        prev: Digest::GENESIS,
        event,
    }
}

/// Project the schedule down to what the reminder reads.
fn write_cache(ctx: &Context, state: &State, today: Date) -> Result<()> {
    let due = state
        .scheduled()
        .into_iter()
        .map(|slug| match slug.standing(today) {
            Standing::Due | Standing::Probe => today,
            Standing::Waiting { until } | Standing::Held { until } => until,
        })
        .collect();
    Cache { v: SCHEMA, due }.save(&ctx.paths.cache())
}

/// A titled table in whichever shape the run is in.
fn block(ctx: &Context, heading: &str, table: &Table, notes: &[String]) -> String {
    match ctx.format {
        Format::Human => human::block(heading, table, notes, ctx.style),
        Format::Agent | Format::Json => agent::block(heading, table, notes),
    }
}
