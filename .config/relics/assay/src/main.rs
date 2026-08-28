//! `assay` — the machine's verification surface.

use std::io::Write as _;
use std::process::ExitCode;

use anyhow::{Context as _, Result};
use camino::Utf8PathBuf;
use clap::Parser;

use assay::render::{self, Style};
use assay::station::Context;
use assay::{roster, run};
use relic_core::ui::{ColorChoice, Format};

/// The environment variable that names an output shape for this relic alone.
const UI_VAR: &str = "ASSAY_UI";

#[derive(Parser, Debug)]
#[command(
    name = "assay",
    about = "Verify the machine: one finding shape over every check.",
    long_about = "Runs every station and grades what they found.\n\n\
                  Exit 0 when nothing is wrong, 1 when the machine is degraded, \
                  2 when it is no longer reproducible from the repo or a guard \
                  is disarmed.",
    version
)]
struct Cli {
    /// Stations to run. Every one of them when none is named.
    #[arg(value_name = "STATION")]
    stations: Vec<String>,

    /// Print the roster and run nothing.
    #[arg(long)]
    list: bool,

    /// Print nothing when the machine is clean.
    #[arg(long, short)]
    quiet: bool,

    /// Also run the checks that cost the network, a passphrase or real time.
    #[arg(long)]
    deep: bool,

    /// The home directory to verify. This machine's, by default.
    #[arg(long, value_name = "DIR")]
    home: Option<Utf8PathBuf>,

    /// Output shape.
    #[arg(long, value_enum)]
    format: Option<Format>,

    /// Whether to use colour.
    #[arg(long, value_enum, default_value_t = ColorChoice::Auto)]
    color: ColorChoice,
}

fn main() -> ExitCode {
    match dispatch() {
        Ok(code) => code,
        Err(error) => {
            let mut stderr = std::io::stderr();
            let _ = writeln!(stderr, "assay: {error:#}");
            // Distinct from a graded run: the tool itself failed, so nothing was
            // verified and neither 0 nor 1 nor 2 would be true.
            ExitCode::from(3)
        }
    }
}

fn dispatch() -> Result<ExitCode> {
    let cli = Cli::parse();
    let format = Format::from_process(cli.format, UI_VAR);
    let mut stdout = std::io::stdout().lock();

    if cli.list {
        let all = roster::roster();
        let rows: Vec<(&str, &str)> = all
            .iter()
            .map(|station| (station.id().as_str(), station.title()))
            .collect();
        render::list(&mut stdout, &rows, format)?;
        return Ok(ExitCode::SUCCESS);
    }

    let home = match cli.home {
        Some(home) => home,
        None => relic_core::path::home().context("$HOME is unset or not utf-8")?,
    };
    let mut cx = Context::new(home, Context::ambient_path());
    if cli.deep {
        cx = cx.deeply();
    }

    let stations = roster::select(roster::roster(), &cli.stations)?;
    let reports = run::run(&stations, &cx);
    let grade = run::grade(&reports);

    render::report(
        &mut stdout,
        &reports,
        Style {
            format,
            color: cli.color.use_color(format),
            quiet: cli.quiet,
        },
    )?;

    Ok(ExitCode::from(grade.exit_code()))
}
