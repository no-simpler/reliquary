//! `warden` — the commit guard.
//!
//! Thin by construction: everything it decides lives in the library, so the
//! thing that invokes it can change without the logic moving.
//!
//! Fail-closed. A guard that cannot run must refuse and name the remedy, never
//! pass quietly — the failure it exists to prevent is unrecoverable, and the
//! one it causes by refusing is a retry.

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;

use warden::{Config, Definition, Finding, Verdict, scan, staged};

/// The guard over what is being committed.
#[derive(Debug, Parser)]
#[command(
    name = "warden",
    about = "Refuse staged content that must never reach a public tree",
    version
)]
struct Cli {
    /// Check these paths instead of the staged set. Relative to the work tree.
    #[arg(value_name = "PATH")]
    paths: Vec<Utf8PathBuf>,

    /// The definition to test against. Defaults to the one beside the hook.
    #[arg(long, value_name = "FILE")]
    definition: Option<Utf8PathBuf>,

    /// The configuration. Defaults to ~/.config/warden/config.toml.
    #[arg(long, value_name = "FILE")]
    config: Option<Utf8PathBuf>,

    /// The work tree the paths are relative to. Defaults to the home directory.
    #[arg(long, value_name = "DIR")]
    root: Option<Utf8PathBuf>,

    /// Say what was examined even when nothing was found.
    #[arg(long, short)]
    verbose: bool,
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(true) => std::process::ExitCode::SUCCESS,
        Ok(false) => std::process::ExitCode::FAILURE,
        Err(error) => {
            eprintln!("warden: {error:#}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<bool> {
    let cli = Cli::parse();
    let home = relic_core::path::home().context("no home directory to resolve paths against")?;
    let root = cli.root.clone().unwrap_or_else(|| home.clone());

    let definition = match &cli.definition {
        Some(path) => Definition::load(path),
        None => Definition::discover(&home),
    }
    .context("the guard's definition")?;

    let config = match &cli.config {
        Some(path) => Config::load(path),
        None => Config::discover(&home),
    }
    .context("the guard's configuration")?;

    let derived = cli.paths.is_empty();
    let paths = if derived {
        staged::paths().context("reading the staged set")?
    } else {
        cli.paths.clone()
    };

    // A guard that examined nothing must say so. Not a refusal — an empty
    // commit is legal — but the same output would follow from git having
    // answered for the wrong repository, and that must never pass quietly.
    if derived && paths.is_empty() {
        eprintln!("warden: nothing is staged, so nothing was guarded.");
    }

    let verdict = examine(&root, &paths, &definition, &config);
    report(&verdict, cli.verbose);
    Ok(verdict.clean())
}

fn examine(
    root: &Utf8Path,
    paths: &[Utf8PathBuf],
    definition: &Definition,
    config: &Config,
) -> Verdict {
    let mut verdict = Verdict::default();
    for path in paths {
        if config.allows_binary(path) {
            continue;
        }
        let absolute = root.join(path);
        // A staged path that is gone is a race with the working tree, not a
        // finding: git has the content either way, and the next run sees it.
        let Ok(bytes) = fs_err::read(absolute.as_std_path()) else {
            continue;
        };
        verdict.examined += 1;
        verdict
            .findings
            .extend(scan::file(path, &bytes, definition));
    }
    verdict
}

fn report(verdict: &Verdict, verbose: bool) {
    for finding in &verdict.findings {
        let label = match finding {
            Finding::Term { .. } | Finding::Characters { .. } => "refused",
            Finding::Unreadable { .. } => "unreadable",
        };
        eprintln!("warden {label}: {finding}");
    }
    if verdict.clean() {
        if verbose {
            eprintln!(
                "warden: {} file(s) examined, nothing found",
                verdict.examined
            );
        }
        return;
    }
    eprintln!(
        "\nwarden: {} finding(s) across {} file(s) — commit refused.",
        verdict.findings.len(),
        verdict.examined
    );
}
