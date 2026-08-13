use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};

use ernest::aggregate::Views;
use ernest::report::Presentation;
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
    pub view: View,

    #[command(flatten)]
    pub measure: Measure,
}

/// How to present a report. Global, because `diff` presents one too and a view
/// flag that reached only the measurement would leave the two renderers
/// disagreeing about what `--by` means.
#[derive(Debug, Args)]
pub struct View {
    /// Which breakdowns to show. None by default: a bare run is the figure.
    #[arg(
        long,
        global = true,
        value_enum,
        value_delimiter = ',',
        value_name = "VIEW"
    )]
    pub by: Vec<ViewArg>,

    /// Rows in each ranked view, most prose first. 0 shows every row.
    #[arg(long, global = true, default_value_t = 20, value_name = "N")]
    pub top: usize,

    /// What to write. `value` is the density alone, for a caller that acts on
    /// the exit code.
    #[arg(long, global = true, value_enum, value_name = "FORMAT")]
    pub format: Option<FormatArg>,

    /// Emit a machine-readable snapshot instead of a report.
    #[arg(long, global = true, conflicts_with_all = ["format", "quiet"])]
    pub json: bool,

    /// Write the density and nothing else.
    #[arg(long, short, global = true, conflicts_with_all = ["format", "by"])]
    pub quiet: bool,
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

impl View {
    pub fn views(&self) -> Views {
        Views {
            by_cohort: self.by.contains(&ViewArg::Cohort),
            by_language: self.by.contains(&ViewArg::Language),
            by_file: self.by.contains(&ViewArg::File),
            by_section: self.by.contains(&ViewArg::Section),
        }
    }

    pub fn presentation(&self) -> Presentation {
        Presentation {
            views: self.views(),
            // 0 is "no limit". Asking for nothing is already spelled by omitting
            // `--by`, so the spare value buys the reading that has no other
            // spelling.
            top: if self.top == 0 { usize::MAX } else { self.top },
        }
    }

    /// The one place output mode is decided. The two booleans are the shorter
    /// spellings of the formats they name, and clap refuses them together.
    pub fn format(&self) -> Format {
        if self.json {
            return Format::Json;
        }
        if self.quiet {
            return Format::Value;
        }
        match self.format {
            Some(FormatArg::Json) => Format::Json,
            Some(FormatArg::Value) => Format::Value,
            Some(FormatArg::Text) | None => Format::Text,
        }
    }
}

/// The resolved output mode, once the aliases have been folded in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Text,
    Json,
    Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum FormatArg {
    /// The report.
    Text,
    /// A snapshot, for `ernest diff`.
    Json,
    /// The density alone.
    Value,
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

/// Declared most-asked-for first: the order `--help` prints and the order the
/// affordance note names them in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum ViewArg {
    /// One row per file, most prose first.
    File,
    /// One row per innermost heading of a document.
    Section,
    /// The total and the cohorts it sums.
    Cohort,
    /// The same, decomposed one level further into languages.
    Language,
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
