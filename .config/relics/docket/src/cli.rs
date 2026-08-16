use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::item::Kind;
use crate::ui::{ColorChoice, Format};

pub const ROOT_AFTER_LONG_HELP: &str = "\
docket guide handoff|relay|spec = doctrine
docket help ladder|metadata = reference topics";

#[derive(Parser)]
#[command(
    name = "docket",
    version,
    about = "Outstanding agentic work per project, kept out of the project itself.",
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

    /// Act on another project's docket.
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<PathBuf>,

    /// Print only what was asked for.
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

#[derive(Subcommand)]
pub enum Command {
    /// List outstanding items.
    #[command(
        visible_alias = "ls",
        after_long_help = "\
Examples:

  docket list                    this project, open items
  docket list --all              every project on this machine
  docket list --kind spec        specs only
  docket list --invalid          only items whose metadata will not parse"
    )]
    List(ListArgs),

    /// New docket item; returns path.
    Create(CreateArgs),

    /// Print an item's body.
    Show(IdArgs),

    /// Print an item's file path, for writing or editing its body.
    #[command(after_long_help = "\
Example:

  docket path b71c            /Users/you/.claude/docket/<project>/handoffs/b71c-....md")]
    Path(IdArgs),

    /// Edit docket item metadata.
    Set(SetArgs),

    /// Change order of docket items.
    Reorder(ReorderArgs),

    /// Advance docket item kind.
    Promote(PromoteArgs),

    /// Replace relay with successor.
    Relay(RelayArgs),

    /// Close a docket item whose work is done.
    #[command(after_long_help = "\
Closing removes the item. The depot's history is what keeps it, so closing
needs git:

  git -C ~/.claude/docket log --diff-filter=D --name-only")]
    Close(IdArgs),

    /// Report invalid metadata for fixing.
    Doctor,

    /// Emit banner of outstanding work, if any.
    Announce(AnnounceArgs),

    /// Explain a topic, or a command.
    Help(HelpArgs),

    /// Doctrine: what to write, and when. Name kinds to append their guidance.
    Guide(GuideArgs),

    /// Print a shell completion script.
    #[command(after_long_help = "\
Examples:

  docket completions zsh  > ~/.config/zsh/completion/_docket
  docket completions fish > ~/.config/fish/completions/docket.fish")]
    Completions(CompletionArgs),
}

#[derive(Args, Default)]
pub struct ListArgs {
    /// Every project on this machine, not just this one.
    #[arg(long)]
    pub all: bool,

    /// Only this kind.
    #[arg(long, value_enum)]
    pub kind: Option<Kind>,

    /// Only items carrying a block.
    #[arg(long)]
    pub blocked: bool,

    /// Only items whose metadata will not parse.
    #[arg(long)]
    pub invalid: bool,
}

#[derive(Args)]
pub struct CreateArgs {
    /// Which kind to open at.
    #[arg(value_enum)]
    pub kind: Kind,

    /// The item's name, at most 72 characters. Use - for standard input.
    #[arg(long)]
    pub title: String,

    /// One line under the title, at most 80 characters. Use - for standard
    /// input.
    #[arg(long)]
    pub tagline: String,

    /// Open it for another project. Defaults to this one.
    #[arg(long, value_name = "PATH")]
    pub to: Option<PathBuf>,

    /// Allow a target directory that does not exist yet.
    #[arg(long)]
    pub allow_missing: bool,

    /// Body to write, instead of leaving the file for you to fill in. Use -
    /// for standard input.
    #[arg(long)]
    pub body: Option<String>,
}

#[derive(Args)]
pub struct IdArgs {
    /// Four-character item id, as printed by any listing.
    pub id: String,
}

#[derive(Args)]
pub struct SetArgs {
    /// Four-character item id, as printed by any listing.
    pub id: String,

    /// Replace the title. Use - for standard input.
    #[arg(long)]
    pub title: Option<String>,

    /// Replace the tagline. Use - for standard input.
    #[arg(long)]
    pub tagline: Option<String>,

    /// Record what must clear before this item can move, in one line. Use -
    /// for standard input.
    #[arg(long, conflicts_with = "clear_blocked")]
    pub blocked: Option<String>,

    /// Drop the block.
    #[arg(long)]
    pub clear_blocked: bool,

    /// Replace the tags wholesale.
    #[arg(long, value_delimiter = ',')]
    pub tags: Option<Vec<String>>,
}

#[derive(Args)]
pub struct ReorderArgs {
    /// Four-character item id. Omit only with --sequence.
    #[arg(required_unless_present = "sequence")]
    pub id: Option<String>,

    /// Move it to the front.
    #[arg(long, group = "placement")]
    pub top: bool,

    /// Move it to the back.
    #[arg(long, group = "placement")]
    pub bottom: bool,

    /// Move it to this position, counting from one.
    #[arg(long, group = "placement", value_name = "N")]
    pub position: Option<usize>,

    /// Move it directly ahead of this item.
    #[arg(long, group = "placement", value_name = "ID")]
    pub before: Option<String>,

    /// Move it directly behind this item.
    #[arg(long, group = "placement", value_name = "ID")]
    pub after: Option<String>,

    /// Reorder in bulk. Listed items move to the front in this order.
    #[arg(long, value_delimiter = ',', conflicts_with_all = ["placement", "id"])]
    pub sequence: Option<Vec<String>>,
}

#[derive(Args)]
pub struct PromoteArgs {
    /// Four-character item id, as printed by any listing.
    pub id: String,

    /// Jump to a kind instead of advancing one step.
    #[arg(long, value_enum)]
    pub to: Option<Kind>,
}

#[derive(Args)]
pub struct RelayArgs {
    /// The relay being consumed.
    pub id: String,

    /// Title of the successor. Use - for standard input.
    #[arg(long)]
    pub title: String,

    /// Tagline of the successor. Use - for standard input.
    #[arg(long)]
    pub tagline: String,

    /// Body of the successor. Use - for standard input.
    #[arg(long)]
    pub body: Option<String>,
}

#[derive(Args)]
pub struct AnnounceArgs {
    /// Emit Claude Code SessionStart hook JSON.
    #[arg(long)]
    pub hook: bool,
}

#[derive(Args)]
pub struct HelpArgs {
    /// A topic, or a command name.
    pub topic: Option<String>,
}

#[derive(Args)]
pub struct GuideArgs {
    /// Any of handoff, relay, spec. Omit for orientation alone.
    pub topics: Vec<String>,
}

#[derive(Args)]
pub struct CompletionArgs {
    /// Shell to generate for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}
