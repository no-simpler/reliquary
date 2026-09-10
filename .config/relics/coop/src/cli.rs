use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::config::Style;
use relic_core::ui::{ColorChoice, Format};

pub const ROOT_AFTER_LONG_HELP: &str = "\
coop guide sources|cadence = doctrine
coop help wiring|tiers|keys = reference topics";

#[derive(Parser)]
#[command(
    name = "coop",
    version,
    about = "What wants your attention, drawn once and gone when you act on it.",
    after_long_help = ROOT_AFTER_LONG_HELP,
    disable_help_subcommand = true,
    infer_subcommands = true
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    pub global: Global,
}

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

    /// Override how the card is drawn.
    #[arg(long, global = true, value_enum)]
    pub style: Option<Style>,

    /// Print only what was asked for.
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// The prompt hook: draw the card only if the outstanding set has moved.
    #[command(after_long_help = "\
Not for hands. Every interactive shell runs this before every prompt, so it
draws only on an edge — the first prompt of a shell, and any prompt after the
outstanding set changes. Run bare coop to see the card on demand.

It also writes the badge file the prompt segment reads, which is why one exec
serves both.")]
    Tick,

    /// The prompt segment: how many are outstanding, or nothing at all.
    Prompt,

    /// One line per outstanding notice.
    #[command(visible_alias = "ls")]
    List,

    /// One notice, with whatever detail its producer supplied.
    Show(ShowArgs),

    /// Every declared source, and what it is doing.
    Sources,

    /// Run asked sources now, rather than when their cache next lapses.
    #[command(after_long_help = "\
Examples:

  coop refresh              every asked source
  coop refresh --source rote  one of them

The tick path spawns this for itself in the background. Running it by hand is
for when you would rather not wait for a cache to lapse.")]
    Refresh(RefreshArgs),

    /// What coop knows about its own health.
    Doctor,

    /// Reference topics.
    Help(TopicArgs),

    /// Doctrine.
    Guide(TopicArgs),

    /// Shell completions.
    Completions(CompletionArgs),
}

#[derive(Args)]
pub struct ShowArgs {
    /// The source id.
    pub id: String,
}

#[derive(Args)]
pub struct RefreshArgs {
    /// Just this one.
    #[arg(long, value_name = "ID")]
    pub source: Option<String>,
}

#[derive(Args)]
pub struct TopicArgs {
    /// Topics to print. All of them when none is named.
    pub topics: Vec<String>,
}

#[derive(Args)]
pub struct CompletionArgs {
    /// The shell to generate for.
    #[arg(value_enum)]
    pub shell: Shell,
}
