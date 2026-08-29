use camino::Utf8PathBuf;

use clap::{Args, Parser, Subcommand};
use clap_complete::Shell;

use relic_core::ui::{ColorChoice, Format};

pub const ROOT_AFTER_LONG_HELP: &str = "\
mantra guide modes|schedule|writing = doctrine
mantra help triggers|metadata|hooks = reference topics";

#[derive(Parser)]
#[command(
    name = "mantra",
    version,
    about = "Session modes: what a directive says, and when it is said again.",
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

    /// Resolve modes as if the session were in this directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<Utf8PathBuf>,
}

#[derive(Subcommand)]
pub enum Command {
    /// List every mode a +token can reach, and when each one is said.
    #[command(visible_alias = "ls")]
    List,

    /// Report why each mode of a session did or did not fire.
    #[command(after_long_help = "\
With no --session, the most recently written state is used, which in a live
session is that session's own.")]
    Explain(ExplainArgs),

    /// Print what activating these tokens would inject, without injecting it.
    #[command(name = "dry-run")]
    DryRun(DryRunArgs),

    /// Answer one Claude Code hook, reading its payload on stdin.
    #[command(after_long_help = "\
The payload names its own event, so one wiring serves every boundary. Silent
whenever there is nothing to say, and silent on every failure: this writes into
a model's context, and an error message there is an instruction.")]
    Hook,

    /// Forget state for sessions nothing has written to in a while.
    Gc(GcArgs),

    /// Check the mode corpus, the hook wiring and the state directory.
    Doctor,

    /// Reference topics.
    Help(TopicArgs),

    /// Doctrine.
    Guide(TopicArgs),

    /// Shell completions.
    Completions(CompletionArgs),
}

#[derive(Args)]
pub struct ExplainArgs {
    /// The session id. Defaults to the most recently written state.
    #[arg(long, value_name = "ID")]
    pub session: Option<String>,
}

#[derive(Args)]
pub struct DryRunArgs {
    /// Mode names, without the leading +.
    #[arg(value_name = "TOKEN", required = true)]
    pub tokens: Vec<String>,
}

#[derive(Args)]
pub struct GcArgs {
    /// Keep state written within this many days.
    #[arg(long, value_name = "DAYS", default_value_t = crate::state::MAX_AGE_DAYS)]
    pub days: u64,

    /// Report what would go, and remove nothing.
    #[arg(short = 'n', long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct TopicArgs {
    /// Omit for the whole thing.
    #[arg(value_name = "TOPIC")]
    pub topics: Vec<String>,
}

#[derive(Args)]
pub struct CompletionArgs {
    /// The shell to generate for.
    #[arg(value_enum)]
    pub shell: Shell,
}
