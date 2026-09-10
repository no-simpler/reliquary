//! The command line. Doc comments here are the help text, so there is one copy
//! by construction.

use clap::{Args, Parser, Subcommand};
use relic_core::ui::{ColorChoice, Format};

use crate::slug::Slug;

/// Both namespaces, advertised where a reader will look for them.
pub const ROOT_AFTER_LONG_HELP: &str = "\
rote guide ladder|irregularity|custody = doctrine
rote help intervals|records|exit = reference topics";

/// The daily drill.
#[derive(Parser)]
#[command(
    name = "rote",
    version,
    about = "Spaced-repetition drill for the passwords you must hold in your head.",
    long_about = "Spaced-repetition drill for the passwords you must hold in your head.\n\n\
                  Called bare, rote runs today's session: what is due, plus whatever the filler \
                  policy adds as unscored practice. It records what was actually done rather than \
                  insisting on a schedule, so drilling more often, or less, both stay measurable.\n\n\
                  The secret is held in no recoverable form. rote can say wrong; it can never say \
                  what the right answer was.",
    after_long_help = ROOT_AFTER_LONG_HELP,
    disable_help_subcommand = true,
    infer_subcommands = true
)]
pub struct Cli {
    /// Bare, this runs today's session.
    #[command(subcommand)]
    pub command: Option<Command>,

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
    /// Show the schedule, the ladder position and the cutover gate.
    #[command(visible_alias = "due")]
    Status(ScheduleArgs),

    /// Show retention, latency, lapses and punctuality.
    Stats(MeasurementArgs),

    /// Show recent records.
    Log(LogArgs),

    /// Drill without scoring, whatever the schedule says.
    Practice(PracticeArgs),

    /// Enrol a slug. The secret is typed twice and stored only as a verifier.
    Add(AddArgs),

    /// Hold a slug out of the reminder until a chosen day, then take it cold.
    Probe(ProbeArgs),

    /// Replace a slug's verifier after a rotation.
    Rekey(RekeyArgs),

    /// Take a slug off the schedule. Its history stays; its verifier does not.
    Retire(RetireArgs),

    /// Report the state of the drill, for assay and for yadm doctor.
    Doctor,

    /// One line for the shell, or nothing. Reads only the reminder cache.
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
    /// Include retired slugs.
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

    /// Only this slug.
    #[arg(long, value_name = "SLUG")]
    pub slug: Option<Slug>,
}

/// Arguments for voluntary practice.
#[derive(Args)]
pub struct PracticeArgs {
    /// Which slugs. All of them when none is named.
    #[arg(value_name = "SLUG")]
    pub slugs: Vec<Slug>,
}

/// Arguments for enrolment.
#[derive(Args)]
pub struct AddArgs {
    /// The slug, which by convention matches the 1Password item title.
    #[arg(value_name = "SLUG")]
    pub slug: Slug,

    /// Losing this one is unrecoverable rather than inconvenient. Criticals are
    /// asked about first.
    #[arg(long)]
    pub critical: bool,

    /// Read the secret from a pipe instead of a terminal. Refused when any
    /// standard stream is a terminal, so it cannot become a shell-history habit.
    #[arg(long)]
    pub stdin: bool,
}

/// Arguments for a stretch horizon.
#[derive(Args)]
pub struct ProbeArgs {
    /// The slug.
    #[arg(value_name = "SLUG")]
    pub slug: Slug,

    /// How far out the horizon sits, in days. Thirty to ninety is the useful
    /// range.
    #[arg(long = "in", value_name = "DAYS", conflicts_with = "clear")]
    pub days: Option<u32>,

    /// Drop a horizon that is no longer wanted.
    #[arg(long)]
    pub clear: bool,
}

/// Arguments for a rotation.
#[derive(Args)]
pub struct RekeyArgs {
    /// The slug.
    #[arg(value_name = "SLUG")]
    pub slug: Slug,

    /// Replace without proving the current secret. This is a re-enrolment, and
    /// the log records it as one.
    #[arg(long)]
    pub force: bool,

    /// Read from a pipe: the current secret, then the new one, one per line.
    #[arg(long)]
    pub stdin: bool,
}

/// Arguments for retirement.
#[derive(Args)]
pub struct RetireArgs {
    /// The slug.
    #[arg(value_name = "SLUG")]
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
    fn bare_rote_is_the_daily_session() {
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
    fn a_horizon_is_either_set_or_cleared_and_never_both() {
        assert!(Cli::try_parse_from(["rote", "probe", "a", "--in", "45", "--clear"]).is_err());
        assert!(Cli::try_parse_from(["rote", "probe", "a", "--in", "45"]).is_ok());
    }

    #[test]
    fn a_bad_slug_is_refused_at_the_edge() {
        assert!(Cli::try_parse_from(["rote", "add", "Not A Slug"]).is_err());
    }
}
