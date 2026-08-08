use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use ernest::aggregate::Views;
use ernest::span::Unit;
use ernest::walk::Scope;

#[derive(Debug, Parser)]
#[command(
    name = "ernest",
    version,
    about = "Measure prose density — the share of a codebase's text that is prose rather than code.",
    long_about = "Measure prose density: prose / (prose + code), counting non-whitespace \
                  characters. Unavoidable text — open tags, shebangs, tooling directives — is \
                  counted toward neither side.\n\n\
                  ernest is a helper, not a gate. Measure before, measure after, then look at \
                  where the difference came from."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub measure: Measure,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Compare two --json snapshots and show where the prose moved.
    Diff {
        /// Snapshot taken before the change.
        before: PathBuf,
        /// Snapshot taken after it.
        after: PathBuf,
    },
}

#[derive(Debug, Args)]
pub struct Measure {
    /// Directories or files to measure.
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// Emit a machine-readable snapshot instead of a report.
    #[arg(long)]
    pub json: bool,

    /// Extra rows to carry, most prose first. Sections are a documentation view.
    #[arg(long, value_enum, value_delimiter = ',', value_name = "VIEW")]
    pub by: Vec<ViewArg>,

    /// What to count. Characters are canonical; lines are the familiar proxy.
    #[arg(long, value_enum, default_value_t = UnitArg::Chars)]
    pub unit: UnitArg,

    /// Measure only one language.
    #[arg(long)]
    pub lang: Option<String>,

    /// How far to reach. Dependency and build directories are excluded at every
    /// level.
    #[arg(long, value_enum, default_value_t = ScopeArg::Local, value_name = "LEVEL")]
    pub scope: ScopeArg,

    /// Exit 1 when density exceeds this percentage. A convenience, not a gate.
    #[arg(long, value_name = "PCT")]
    pub max_density: Option<f64>,
}

impl Measure {
    pub fn views(&self) -> Views {
        Views {
            by_file: self.by.contains(&ViewArg::File),
            by_section: self.by.contains(&ViewArg::Section),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum UnitArg {
    Chars,
    Lines,
}

impl From<UnitArg> for Unit {
    fn from(value: UnitArg) -> Self {
        match value {
            UnitArg::Chars => Unit::Chars,
            UnitArg::Lines => Unit::Lines,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ViewArg {
    /// One row per file.
    File,
    /// One row per innermost heading of a document.
    Section,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ScopeArg {
    /// Only what a fresh clone would see.
    Shared,
    /// Plus files excluded on this machine alone — the second brain.
    Local,
    /// Plus gitignored files.
    All,
}

impl From<ScopeArg> for Scope {
    fn from(value: ScopeArg) -> Self {
        match value {
            ScopeArg::Shared => Scope::Shared,
            ScopeArg::Local => Scope::Local,
            ScopeArg::All => Scope::All,
        }
    }
}
