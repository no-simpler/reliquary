use std::path::PathBuf;

use clap::error::ErrorKind;
use clap::{ArgAction, Args, CommandFactory, Parser, Subcommand, ValueEnum};

use ernest::aggregate::Views;
use ernest::analyze::profiles::PROFILES;
use ernest::report::{Presentation, Verbosity};
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

    // Flatten order sets the order `--help` prints the headings in, and
    // measurement is what a reader looks for first. Safe to reorder: `paths` is
    // the only positional in the parser, so nothing can be reassigned an index.
    #[command(flatten)]
    pub measure: Measure,

    #[command(flatten)]
    pub view: View,
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
        value_name = "VIEW",
        help_heading = "Ranking"
    )]
    pub by: Vec<ViewArg>,

    /// Rows in each ranked view, most prose first. 0 shows every row.
    #[arg(
        long,
        global = true,
        default_value_t = 20,
        value_name = "N",
        help_heading = "Ranking"
    )]
    pub top: usize,

    /// What to write. `value` is the density alone, for a caller that acts on
    /// the exit code.
    #[arg(
        long,
        short,
        global = true,
        value_enum,
        value_name = "FORMAT",
        help_heading = "Output"
    )]
    pub format: Option<FormatArg>,

    /// Emit a machine-readable snapshot instead of a report.
    #[arg(
        long,
        global = true,
        conflicts_with = "format",
        help_heading = "Output"
    )]
    pub json: bool,

    /// Say more. Repeatable: provenance, then per-file diagnostics, then
    /// parse-level.
    #[arg(
        long,
        short,
        global = true,
        action = ArgAction::Count,
        help_heading = "Output"
    )]
    pub verbose: u8,

    /// Say less. The figure, and nothing that comments on the run.
    #[arg(
        long,
        short,
        global = true,
        action = ArgAction::Count,
        help_heading = "Output"
    )]
    pub quiet: u8,
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

    /// Write a shell completion script to stdout.
    Completions {
        /// Which shell to write for.
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },
}

#[derive(Debug, Args)]
pub struct Measure {
    // No heading: clap sorts positionals after options within one, which would
    // bury the argument every invocation starts with. Left alone it keeps its own
    // `Arguments:` section at the top, where a reader looks for it.
    /// Directories or files to measure.
    #[arg(default_value = ".")]
    pub paths: Vec<PathBuf>,

    /// What to count. Characters are canonical; lines are the familiar proxy.
    #[arg(long, value_enum, default_value_t = UnitArg::Chars, help_heading = "Measurement")]
    pub unit: UnitArg,

    /// Measure only one language.
    #[arg(long, value_parser = languages(), value_name = "LANG", help_heading = "Measurement")]
    pub lang: Option<String>,

    /// How far to reach. Dependency and build directories are excluded at every
    /// level.
    #[arg(
        long,
        value_enum,
        default_value_t = ScopeArg::Local,
        value_name = "LEVEL",
        help_heading = "Measurement"
    )]
    pub scope: ScopeArg,

    /// Exit 1 when density exceeds this percentage. A convenience, not a gate.
    #[arg(long, value_name = "PCT", help_heading = "Measurement")]
    pub max_density: Option<f64>,
}

impl Cli {
    /// Every refusal that is about what was *asked for* rather than how it was
    /// spelled, checked on the resolved `Format` so an alias and the format it
    /// names cannot disagree about what is allowed. `-q` used to refuse `--by`
    /// while `--format value` — the same output mode — accepted it.
    ///
    /// `clap::Error` rather than `anyhow`: it already exits 2, and it renders
    /// with the same prefix, usage line and `--help` hint as a parse failure, so
    /// a conflict clap catches and a conflict ernest catches look alike.
    /// `ArgGroup` would be the wrong tool — a group reasons about which args
    /// were present, which is the spelling-level check being retired.
    pub fn validate(&self) -> Result<(), clap::Error> {
        let refuse =
            |message: &str| Err(Cli::command().error(ErrorKind::ArgumentConflict, message));

        if self.view.format() == Format::Value && !self.view.by.is_empty() {
            return refuse(
                "the value format writes one number, and --by has nothing to write it to",
            );
        }

        if matches!(self.command, Some(Command::Diff { .. })) && self.view.format() == Format::Json
        {
            return refuse("diff has no --format json; compare the snapshots you already hold");
        }

        Ok(())
    }
}

/// Every language the registry names, deduplicated — a dialect that needs a
/// second grammar is still one language, and listing TypeScript twice would read
/// as a bug in the error message rather than as the fact it is.
///
/// A parser rather than a check in the run: it puts the list in `--help`, in the
/// refusal, and in the generated completions, and it cannot drift from
/// `PROFILES`. Before it, `--lang nonsense` matched nothing, reported `n/a` and
/// exited 0 — a typo indistinguishable from a repository with no prose in it.
fn languages() -> clap::builder::PossibleValuesParser {
    let mut names: Vec<&'static str> = PROFILES.iter().map(|profile| profile.language).collect();
    names.sort_unstable();
    names.dedup();
    clap::builder::PossibleValuesParser::new(names)
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
            verbosity: self.verbosity(),
        }
    }

    /// One axis, so the two counts net off before the clamp and a caller can
    /// walk from either end to the other. Both ends clamp silently.
    pub fn verbosity(&self) -> Verbosity {
        match i32::from(self.verbose) - i32::from(self.quiet) {
            ..=-1 => Verbosity::Quiet,
            0 => Verbosity::Normal,
            1 => Verbosity::Verbose,
            2 => Verbosity::Debug,
            _ => Verbosity::Trace,
        }
    }

    /// The one place output mode is decided. `--json` is the shorter spelling of
    /// the format it names, and clap refuses the two together.
    pub fn format(&self) -> Format {
        if self.json {
            return Format::Json;
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
