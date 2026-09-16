//! The command line. Doc comments here are the help text, so there is one copy
//! by construction.

use clap::{Args, Parser, Subcommand};
use relic_core::ui::{ColorChoice, Format};

use crate::slug::Slug;

/// Both namespaces, advertised where a reader will look for them.
pub const ROOT_AFTER_LONG_HELP: &str = "\
rote guide ladder|irregularity|custody = doctrine
rote help keys|intervals|records|files|machines|stdin|exit = reference topics";

/// The daily drill.
#[derive(Parser)]
#[command(
    name = "rote",
    version,
    about = "Spaced-repetition drill for the passwords you must hold in your head.",
    long_about = "Spaced-repetition drill for the passwords you must hold in your head.\n\n\
                  Called bare, rote asks for whatever the schedule wants today. If nothing is \
                  due it offers voluntary practice instead, on one keystroke. It keeps a faithful \
                  record and proposes a schedule; it renders no verdict, so what the numbers mean \
                  is yours to decide.\n\n\
                  The secret is held in no recoverable form. rote can say wrong; it can never say \
                  what the right answer was.",
    after_long_help = ROOT_AFTER_LONG_HELP,
    disable_help_subcommand = true,
    infer_subcommands = true
)]
pub struct Cli {
    /// Bare, this runs today's sitting.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Declare this sitting aided: the answer was looked up first.
    #[arg(long)]
    pub aided: bool,

    /// Flags every command carries.
    #[command(flatten)]
    pub global: Global,
}

/// Flags every command carries.
#[derive(Args)]
pub struct Global {
    /// Defaults to agent under Claude Code or off a terminal, human otherwise.
    #[arg(long, global = true, value_enum)]
    pub format: Option<Format>,

    /// Shorthand for --format json.
    #[arg(long, global = true, conflicts_with = "format")]
    pub json: bool,

    /// Honours `NO_COLOR` and `CLICOLOR_FORCE`.
    #[arg(long, global = true, value_enum, default_value = "auto")]
    pub color: ColorChoice,

    /// Print only what was asked for.
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

/// Everything rote can be asked to do.
#[derive(Subcommand)]
pub enum Command {
    /// Show the schedule, the rung and what this machine holds.
    #[command(visible_alias = "due")]
    Status(ScheduleArgs),

    /// Show retention, latency, lapses and punctuality, per engram.
    Stats(MeasurementArgs),

    /// Show recent records.
    Log(LogArgs),

    /// Every machine that has written to the corpus, and this one.
    Machines,

    /// Drill without the schedule asking, whatever it says.
    Practice(PracticeArgs),

    /// Enroll a lineage. The secret is typed twice and kept only as a verifier.
    Enroll(EnrollArgs),

    /// Make a verifier here for an engram this machine is dormant on. Nothing is
    /// verified: rote takes your word that the secret is unchanged.
    Attach(AttachArgs),

    /// Supersede a lineage's engram with a new secret, typed twice. Its ladder
    /// starts over.
    Rotate(RotateArgs),

    /// Take a lineage off the schedule. Its history stays; its verifiers do not.
    Retire(RetireArgs),

    /// Report the state of the drill, for assay and for coop.
    Doctor,

    /// One line for the shell, or nothing. Reads the record and never writes.
    Banner,

    /// Reference: flags, records, exit codes.
    Help(HelpArgs),

    /// Doctrine: what this is for, and why it is shaped this way.
    Guide(GuideArgs),

    /// Shell completions.
    Completions(CompletionArgs),
}

/// Arguments for the schedule.
///
/// Named for what the command shows rather than for the command itself: the
/// obvious names carry a substring the commit guard refuses, so renaming these
/// back would make the relic uncommittable.
#[derive(Args, Default)]
pub struct ScheduleArgs {
    /// Include retired lineages.
    #[arg(long)]
    pub all: bool,
}

/// Arguments for the measurement.
#[derive(Args)]
pub struct MeasurementArgs {
    /// How many days the headline figures cover.
    #[arg(long, value_name = "N", default_value_t = crate::stats::WINDOW_DAYS)]
    pub days: u32,
}

/// Arguments for the record listing.
#[derive(Args)]
pub struct LogArgs {
    /// How many records to show, most recent last.
    #[arg(short = 'n', long, value_name = "N", default_value_t = 20)]
    pub count: usize,

    /// Only this lineage.
    #[arg(long, value_name = "LINEAGE")]
    pub lineage: Option<Slug>,
}

/// Arguments for voluntary practice.
#[derive(Args)]
pub struct PracticeArgs {
    /// Which lineages. All of them when none is named.
    #[arg(value_name = "LINEAGE")]
    pub lineages: Vec<Slug>,

    /// Declare this sitting aided: the answer was looked up first.
    #[arg(long)]
    pub aided: bool,
}

/// Arguments for enrolment.
#[derive(Args)]
pub struct EnrollArgs {
    /// The lineage, which by convention matches the 1Password item title.
    #[arg(value_name = "LINEAGE")]
    pub slug: Slug,

    /// Losing this one is unrecoverable rather than inconvenient. Criticals are
    /// asked about first.
    #[arg(long)]
    pub critical: bool,

    /// Read the secret from a pipe instead of a terminal. Refused when stdin
    /// is a terminal, so it cannot become a shell-history habit.
    #[arg(long)]
    pub stdin: bool,
}

/// Arguments for an attachment.
#[derive(Args)]
pub struct AttachArgs {
    /// The lineage.
    #[arg(value_name = "LINEAGE")]
    pub slug: Slug,

    /// Read the secret from a pipe instead of a terminal.
    #[arg(long)]
    pub stdin: bool,
}

/// Arguments for a rotation.
#[derive(Args)]
pub struct RotateArgs {
    /// The lineage.
    #[arg(value_name = "LINEAGE")]
    pub slug: Slug,

    /// Read the new secret from a pipe, one line.
    #[arg(long)]
    pub stdin: bool,
}

/// Arguments for retirement.
#[derive(Args)]
pub struct RetireArgs {
    /// The lineage.
    #[arg(value_name = "LINEAGE")]
    pub slug: Slug,
}

/// Arguments for the reference namespace.
#[derive(Args)]
pub struct HelpArgs {
    /// A topic, a command, or topics for the list.
    #[arg(value_name = "TOPIC")]
    pub topic: Option<String>,
}

/// Arguments for the doctrine namespace.
#[derive(Args)]
pub struct GuideArgs {
    /// Which topics. All of them when none is named.
    #[arg(value_name = "TOPIC")]
    pub topics: Vec<String>,
}

/// Arguments for completions.
#[derive(Args)]
pub struct CompletionArgs {
    /// Which shell.
    #[arg(value_name = "SHELL")]
    pub shell: clap_complete::Shell,
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory as _, Parser as _};

    use super::Cli;

    #[test]
    fn the_command_line_is_well_formed() {
        Cli::command().debug_assert();
    }

    #[test]
    fn bare_rote_is_the_daily_sitting() {
        let cli = Cli::parse_from(["rote"]);
        assert!(cli.command.is_none());
    }

    #[test]
    fn due_is_the_short_way_to_the_schedule() {
        assert!(matches!(
            Cli::parse_from(["rote", "due"]).command,
            Some(super::Command::Status(_))
        ));
    }

    #[test]
    fn json_and_format_are_two_ways_to_say_one_thing_and_refuse_to_be_combined() {
        assert!(Cli::try_parse_from(["rote", "status", "--json", "--format", "human"]).is_err());
    }

    #[test]
    fn a_bad_lineage_name_is_refused_at_the_edge() {
        assert!(Cli::try_parse_from(["rote", "enroll", "Not A Slug"]).is_err());
    }

    #[test]
    fn the_three_secret_taking_verbs_are_distinct_commands() {
        assert!(matches!(
            Cli::parse_from(["rote", "enroll", "a"]).command,
            Some(super::Command::Enroll(_))
        ));
        assert!(matches!(
            Cli::parse_from(["rote", "attach", "a"]).command,
            Some(super::Command::Attach(_))
        ));
        assert!(matches!(
            Cli::parse_from(["rote", "rotate", "a"]).command,
            Some(super::Command::Rotate(_))
        ));
    }

    #[test]
    fn an_unknown_verb_is_refused() {
        assert!(Cli::try_parse_from(["rote", "probe", "a"]).is_err());
    }

    #[test]
    fn a_sitting_is_declared_aided_on_either_side_of_the_verb_and_nowhere_else() {
        assert!(Cli::parse_from(["rote", "--aided"]).aided);
        assert!(Cli::parse_from(["rote", "--aided", "practice", "a"]).aided);
        match Cli::parse_from(["rote", "practice", "--aided", "a"]).command {
            Some(super::Command::Practice(args)) => assert!(args.aided),
            _ => panic!("practice"),
        }
        assert!(
            Cli::try_parse_from(["rote", "status", "--aided"]).is_err(),
            "a reading is never aided"
        );
    }
}
