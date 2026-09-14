//! What each command does: the context every command is handed, and dispatch.
//!
//! One file per family. `sitting` is the drill, `enroll` is everything that
//! takes or drops a secret, `dialog` is the terminal side those share, `reading`
//! is the tables, `health` is what `assay` and the shell read.

pub mod dialog;
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
use crate::corpus::Corpus;
use crate::corpus::chain::Chains;
use crate::exit::CLEAN;
use crate::ladder::{Ladder, Standing};
use crate::machine::{Flagship, MachineId};
use crate::render::{Table, agent, human};
use crate::store::{Cache, Clock, Env, Paths, Store};
use crate::verifier::file::Verifiers;

/// What every command is handed.
pub struct Context {
    /// The environment, read once.
    pub env: Env,
    /// The config file, or its defaults.
    pub config: Config,
    /// The three homes.
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

    /// This machine's identity.
    ///
    /// Resolved on demand rather than at startup: it costs a subprocess, and
    /// the reminder — which runs before every shell prompt — never needs it.
    ///
    /// # Errors
    ///
    /// When the platform offers no stable identifier.
    pub fn machine(&self) -> Result<MachineId> {
        crate::machine::resolve(self.env.machine.as_deref())
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
        env.now.unwrap_or_else(jiff::Timestamp::now),
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

/// Take the lock and the flagship gate, for a command that writes.
///
/// # Errors
///
/// When this machine may not write, or the corpus cannot be read.
pub fn open_store(ctx: &Context) -> Result<Store> {
    Store::open(ctx.paths.clone(), ctx.machine()?, ctx.env.host.clone())
}

/// Read the corpus without taking the lock, for a command that only looks.
///
/// # Errors
///
/// When a chain cannot be read.
pub fn read_chains(ctx: &Context) -> Result<Chains> {
    Chains::load(&ctx.paths.chains_dir())
}

/// Dispatch.
///
/// # Errors
///
/// When a command cannot do what it was asked.
pub fn run(cli: &Cli) -> Result<u8> {
    // These three describe the tool rather than read the drill, so they answer
    // before there is a corpus to open.
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
            | Command::Machines
            | Command::Practice(_)
            | Command::Enroll(_)
            | Command::Attach(_)
            | Command::Rotate(_)
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
        Some(Command::Machines) => reading::machines(&ctx),
        Some(Command::Enroll(args)) => enroll::enroll(&ctx, args),
        Some(Command::Attach(args)) => enroll::attach(&ctx, args),
        Some(Command::Rotate(args)) => enroll::rotate(&ctx, args),
        Some(Command::Retire(args)) => enroll::retire(&ctx, args),
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

/// Project the schedule down to what the reminder reads.
///
/// A dormant lineage is counted apart from a due one rather than among them: it
/// cannot be drilled at all, so calling it due would be asking for something
/// that is not on offer.
fn write_cache(
    ctx: &Context,
    corpus: &Corpus,
    verifiers: &Verifiers,
    today: Date,
    ladder: &Ladder,
) -> Result<()> {
    project(ctx, corpus, verifiers, today, ladder).save(&ctx.paths.cache())
}

/// The projection itself, so the reminder can redo it when its own copy has
/// stopped answering for what is on disk.
pub(super) fn project(
    ctx: &Context,
    corpus: &Corpus,
    verifiers: &Verifiers,
    today: Date,
    ladder: &Ladder,
) -> Cache {
    let mut due = Vec::new();
    let mut dormant = 0usize;
    for lineage in corpus.active() {
        let Some(dossier) = lineage.current() else {
            continue;
        };
        if !verifiers.holds(&dossier.engram) {
            dormant = dormant.saturating_add(1);
            continue;
        }
        due.push(match dossier.standing(today, ladder) {
            Standing::Due => today,
            Standing::Waiting { until } => until,
        });
    }
    Cache {
        v: crate::corpus::record::SCHEMA,
        due,
        dormant,
        witness: crate::store::Witness::of(&ctx.paths),
        built: Some(today),
    }
}

/// Whether this machine may write, without taking the lock.
///
/// # Errors
///
/// When the marker exists and cannot be read.
fn flagship(ctx: &Context, machine: &MachineId) -> Result<Flagship> {
    Flagship::read(&ctx.paths.marker, machine)
}

/// A titled table in whichever shape the run is in.
fn block(ctx: &Context, heading: &str, table: &Table, notes: &[String]) -> String {
    match ctx.format {
        Format::Human => human::block(heading, table, notes, ctx.style),
        Format::Agent | Format::Json => agent::block(heading, table, notes),
    }
}
