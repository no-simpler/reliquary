mod cli;

use std::io::{self, Write};

use anyhow::{Context, Result};
use clap::Parser;

use cli::{Cli, Command, Format};
use ernest::report::Verbosity;
use ernest::{aggregate, rank, report, walk};

/// Density exceeded `--max-density`. Distinct from 2, which means ernest could
/// not do its job at all.
const EXIT_OVER_THRESHOLD: i32 = 1;
const EXIT_ERROR: i32 = 2;

fn main() {
    let cli = Cli::parse();
    // Before any work: a refusal is a usage error, and clap's own exit renders it
    // the way every other usage error is rendered.
    if let Err(err) = cli.validate() {
        err.exit();
    }
    match run(cli) {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            warn(&format!("ernest: {err:#}"));
            std::process::exit(EXIT_ERROR);
        }
    }
}

/// Every byte the report writes goes through here: stdout is locked once rather
/// than reacquired per macro call, and flushed explicitly rather than relying on
/// `std::process::exit` to do it.
///
/// A reader that went away is not a failure of the measurement — `ernest | head`
/// must not look like a broken run, and the print macros make it one by panicking
/// on `BrokenPipe`. Returning `Ok` rather than exiting is deliberate: the
/// `--max-density` gate reads a density already in hand, so the verdict still
/// reaches the exit code with nobody left to read stdout.
fn emit(text: &str) -> Result<()> {
    let mut out = io::stdout().lock();
    match out.write_all(text.as_bytes()).and_then(|()| out.flush()) {
        Err(err) if err.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        other => other.context("writing the report"),
    }
}

/// stderr breaks the same way, and a diagnostic nobody is left to read is not
/// worth a panic on top of whatever already went wrong.
fn warn(text: &str) {
    let _ = writeln!(io::stderr(), "{text}");
}

fn run(cli: Cli) -> Result<i32> {
    let show = cli.presentation();
    let format = cli.view.format();

    // Before everything else: a completion script is not a measurement, and it
    // is the one subcommand that wants no walk, no snapshot and no flags.
    if let Some(Command::Completions { shell }) = cli.command {
        let mut command = <Cli as clap::CommandFactory>::command();
        let name = command.get_name().to_string();
        let mut script = Vec::new();
        clap_complete::generate(shell, &mut command, name, &mut script);
        emit(&String::from_utf8(script).context("generating the completion script")?)?;
        return Ok(0);
    }

    if let Some(Command::Diff { before, after }) = cli.command {
        let before = report::json::load(&before)?;
        let after = report::json::load(&after)?;
        match format {
            // Refused in `Cli::validate`: a diff is not a measurement, so there
            // is no snapshot to write.
            Format::Json => unreachable!("refused before the walk"),
            Format::Value => emit(&report::diff::quiet(&before, &after)?)?,
            Format::Text => emit(&report::diff::render(&before, &after, show)?)?,
        }
        return Ok(0);
    }

    let options = cli.measure;
    for path in &options.paths {
        if !walk::is_readable_root(path) {
            anyhow::bail!("no such path: {}", path.display());
        }
    }

    let unit = options.unit.into();
    // Per-path diagnostics are unbounded, so they are collected only for the rung
    // that prints them.
    let keep_paths = show.verbosity >= Verbosity::Debug;
    let survey = walk::collect(
        &options.paths,
        options.scope.into(),
        options.lang.as_deref(),
        keep_paths,
    );
    let selection = rank::Selection::build(
        &options.focus,
        options.changed.as_deref(),
        &options.paths,
        options.scope.into(),
    )?;
    let (report, diagnostics) = aggregate::run(&survey, unit, show.views, selection.as_ref());

    match format {
        // The pretty-printer supplies no trailing newline, and a snapshot that
        // ends mid-line is awkward in every reader that is not a parser.
        Format::Json => emit(&format!("{}\n", report::json::render(&report)?))?,
        Format::Value => emit(&report::human::value(&report))?,
        Format::Text => emit(&report::human::render(&report, &diagnostics, show))?,
    }

    if let Some(limit) = options.max_density
        && let Some(density) = report.headline().density
    {
        let measured = density * 100.0;
        if measured > limit {
            warn(&format!(
                "ernest: prose density {measured:.1}% exceeds --max-density {limit:.1}%"
            ));
            return Ok(EXIT_OVER_THRESHOLD);
        }
    }

    Ok(0)
}
