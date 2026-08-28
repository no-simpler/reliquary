use camino::Utf8PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::note::{Kind, Status};
use relic_core::ui::{ColorChoice, Format};

pub const ROOT_AFTER_LONG_HELP: &str = "\
midden guide file|drain = doctrine
midden help metadata|dedup|retention = reference topics";

#[derive(Parser)]
#[command(
    name = "midden",
    version,
    about = "What the harness cost an agent, filed as it happened and dug through later.",
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

    /// Honours NO_COLOR and CLICOLOR_FORCE.
    #[arg(long, global = true, value_enum, default_value = "auto")]
    pub color: ColorChoice,

    /// Narrow to one project. The corpus is machine-wide by default, which is
    /// what lets one cause show up as one note across every repository.
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<Utf8PathBuf>,

    /// Print only what was asked for.
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// List open notes, most-seen first.
    #[command(
        visible_alias = "ls",
        after_long_help = "\
Examples:

  midden list                       every project, open notes
  midden list --project .           this project only
  midden list --kind stale          directives naming something that moved
  midden list --status dismissed    what was considered and declined
  midden list --invalid             only notes whose metadata will not parse
  midden list --archived            what has been retired"
    )]
    List(ListArgs),

    /// File one friction observation. Recurrences fold into the existing note.
    #[command(after_long_help = "\
Examples:

  midden file --kind hunt --title 'brewfile scopes are undocumented' \\
    --target ~/.config/CLAUDE.md --detail 'Four greps to find bbs applies them.'

  midden file --kind rebuff --title 'committed without staging every M line' \\
    --target ~/.config/CLAUDE.md --body -

Filing the same kind, target and claim twice bumps the count instead of
writing a second note.")]
    File(FileArgs),

    /// Print a note's evidence.
    Show(IdArgs),

    /// Print a note's file path.
    Path(IdArgs),

    /// Edit a note's metadata. Changing kind, target or title re-files it.
    Set(SetArgs),

    /// Close a note: the fix landed, or it will not be made.
    Resolve(ResolveArgs),

    /// Retire a note without judging it.
    Archive(IdArgs),

    /// Open notes grouped by where their fix would land, heaviest group first.
    #[command(after_long_help = "\
The excavation surface. One section is one file to open, so a review reads as
a list of proposed edits rather than as a diary.

Examples:

  midden digest                     everything open
  midden digest --since 30          only what has been seen in the last month
  midden digest --kind conflict")]
    Digest(DigestArgs),

    /// Apply retention: drop what is spent, retire what went quiet.
    #[command(after_long_help = "\
Runs from scripts/update.sh on every `up`, so the corpus is pruned without a
schedule of its own. See midden help retention for the boundaries.")]
    Gc(GcArgs),

    /// Report invalid metadata, and a corpus that has stopped being drained.
    Doctor,

    /// Explain a topic, or a command.
    Help(HelpArgs),

    /// Doctrine: filing one, and draining the corpus.
    Guide(GuideArgs),

    /// Print a shell completion script.
    #[command(after_long_help = "\
Examples:

  midden completions zsh  > ~/.config/zsh/completion/_midden
  midden completions fish > ~/.config/fish/completions/midden.fish")]
    Completions(CompletionArgs),
}

#[derive(Args, Default)]
pub struct ListArgs {
    /// Every status, not just what is open.
    #[arg(long)]
    pub all: bool,

    /// Only this kind.
    #[arg(long, value_enum)]
    pub kind: Option<Kind>,

    /// Only this status.
    #[arg(long, value_enum, conflicts_with = "all")]
    pub status: Option<Status>,

    /// Only notes seen within this many days.
    #[arg(long, value_name = "DAYS")]
    pub since: Option<i64>,

    /// Only notes whose metadata will not parse.
    #[arg(long)]
    pub invalid: bool,

    /// What has been retired, instead of what is live.
    #[arg(long)]
    pub archived: bool,
}

#[derive(Args)]
pub struct FileArgs {
    /// What kind of friction this was. See midden guide file.
    #[arg(long, value_enum)]
    pub kind: Kind,

    /// The cause in one line, at most 72 characters. Use - for standard input.
    #[arg(long)]
    pub title: String,

    /// Why it happened, in a sentence or two, at most 200 characters. Use -
    /// for standard input.
    #[arg(long)]
    pub detail: Option<String>,

    /// Where the fix would live: a file, a mode, a skill. Use - for standard
    /// input.
    #[arg(long, value_name = "PATH")]
    pub target: Option<String>,

    /// The evidence: what concretely happened, quoted and short. Use - for
    /// standard input.
    #[arg(long)]
    pub body: Option<String>,

    /// File it against another project. Defaults to the one you are in.
    #[arg(long, value_name = "PATH")]
    pub to: Option<Utf8PathBuf>,
}

#[derive(Args)]
pub struct IdArgs {
    /// Four-character note id, as printed by any listing.
    pub id: String,
}

#[derive(Args)]
pub struct SetArgs {
    /// Four-character note id, as printed by any listing.
    pub id: String,

    /// Replace the kind.
    #[arg(long, value_enum)]
    pub kind: Option<Kind>,

    /// Replace the title. Use - for standard input.
    #[arg(long)]
    pub title: Option<String>,

    /// Replace the detail. Use - for standard input.
    #[arg(long, conflicts_with = "clear_detail")]
    pub detail: Option<String>,

    /// Drop the detail.
    #[arg(long)]
    pub clear_detail: bool,

    /// Replace where the fix would land. Use - for standard input.
    #[arg(long, value_name = "PATH", conflicts_with = "clear_target")]
    pub target: Option<String>,

    /// Drop the target.
    #[arg(long)]
    pub clear_target: bool,

    /// Replace the evidence. Use - for standard input.
    #[arg(long)]
    pub body: Option<String>,
}

#[derive(Args)]
pub struct ResolveArgs {
    /// Four-character note id, as printed by any listing.
    pub id: String,

    /// The fix landed. A recurrence will reopen it.
    #[arg(long, group = "verdict")]
    pub actioned: bool,

    /// No fix is coming. Recurrences still count, silently.
    #[arg(long, group = "verdict")]
    pub dismissed: bool,

    /// Put it back in play.
    #[arg(long, group = "verdict")]
    pub reopen: bool,
}

#[derive(Args)]
pub struct DigestArgs {
    /// Only this kind.
    #[arg(long, value_enum)]
    pub kind: Option<Kind>,

    /// Only notes seen within this many days.
    #[arg(long, value_name = "DAYS")]
    pub since: Option<i64>,
}

#[derive(Args)]
pub struct GcArgs {
    /// Report what would go, and change nothing.
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Args)]
pub struct HelpArgs {
    /// A topic, or a command name.
    pub topic: Option<String>,
}

#[derive(Args)]
pub struct GuideArgs {
    /// Either of file, drain. Omit for orientation alone.
    pub topics: Vec<String>,
}

#[derive(Args)]
pub struct CompletionArgs {
    /// Shell to generate for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}
