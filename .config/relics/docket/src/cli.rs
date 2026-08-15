use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::item::Kind;
use crate::ui::{ColorChoice, Format};

pub const ROOT_LONG_ABOUT: &str = "\
Outstanding agentic work for one project, kept out of the project itself.

A docket item is a handoff, a relay, or a spec, and promotion runs one way:

    handoff ──▶ relay ──▶ spec:design ──▶ spec:implementation
       └────────────────▶

A handoff is read once and closed. A relay is read and owes a successor, so a
chain of sessions can plan one step at a time. A spec is a multi-session
initiative that designs first and implements second.

Items live under ~/.claude/docket, grouped by project, and never inside the
project itself — so they can be written for a directory that does not exist
yet, and they never reach a commit. They are transient by design: write one,
act on it, close it. Ids are four characters and unique across every project on
this machine, so `docket show <id>` works from anywhere.";

pub const ROOT_AFTER_LONG_HELP: &str = "\
Commands by purpose:

  inspect   docket (bare), list, show, path
  create    create, relay
  advance   promote, set, reorder, close, delete
  maintain  doctor, announce, completions, help

Topics:

  docket help ladder     the three kinds and every promotion between them
  docket help metadata   the frontmatter schema, kind by kind
  docket help keys       how a project directory becomes a docket
  docket help agent      writing an item body, and output modes

Getting started:

  docket                                     what is outstanding here
  docket create handoff --title '...' --description '...'
  docket show b71c                           read one
  docket close b71c                          done with it";

#[derive(Parser)]
#[command(
    name = "docket",
    version,
    about = "Outstanding agentic work, per project, bridging sessions.",
    long_about = ROOT_LONG_ABOUT,
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
    /// Output shape. Defaults to human at a terminal and agent everywhere else,
    /// including under Claude Code.
    #[arg(long, global = true, value_enum)]
    pub format: Option<Format>,

    /// Shorthand for --format json.
    #[arg(long, global = true, conflicts_with = "format")]
    pub json: bool,

    /// When to colour. Honours NO_COLOR and CLICOLOR_FORCE.
    #[arg(long, global = true, value_enum, default_value = "auto")]
    pub color: ColorChoice,

    /// Act on this project's docket instead of the one for the working
    /// directory.
    #[arg(long, global = true, value_name = "PATH")]
    pub project: Option<PathBuf>,

    /// Print only what was asked for, with no confirmations.
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
  docket list --invalid          only items whose frontmatter will not parse
  docket list --archived         what has been closed here"
    )]
    List(ListArgs),

    /// Open a new item and print where to write its body.
    #[command(
        long_about = "\
Opens an item and prints its id and the file to write the body into. The CLI
owns placement and metadata; the body is ordinary Markdown you write with your
editor or your file tools.

Title and description are both required. The title is the line a listing shows;
the description is the abstract that tells a future session whether this is the
thing it came for. Either may be `-` to read from standard input.",
        after_long_help = "\
Examples:

  docket create handoff --title 'Settle the Postgres intent' \\
      --description 'Two candidate intents, neither committed to.'

  docket create spec --title 'Rosetta messenger integration' --description -

  docket create handoff --title '...' --description '...' \\
      --to ~/Developer/new-thing --allow-missing"
    )]
    Create(CreateArgs),

    /// Print an item's body.
    Show(IdArgs),

    /// Print an item's file path, for writing or editing its body.
    #[command(after_long_help = "\
Example:

  docket path b71c            /Users/you/.claude/docket/<project>/handoffs/b71c-....md")]
    Path(IdArgs),

    /// Change an item's descriptive metadata.
    #[command(
        long_about = "\
Rewrites frontmatter in canonical order, which also repairs an item whose
frontmatter was hand-edited into something that no longer parses.

Use `promote` to change kind or spec stage; this command never moves an item
along the ladder.",
        after_long_help = "\
Examples:

  docket set b71c --title 'Settle what the connection is for'
  docket set b71c --blocked 'Awaiting the upstream fix in rvben/rumdl#812.'
  docket set b71c --clear-blocked
  docket set b71c --tags ci,postgres"
    )]
    Set(SetArgs),

    /// Change where an item sits in the order.
    #[command(
        long_about = "\
Position is what a person reorders by: it is the number in the first column of
a listing, counting from one.

Give an item and one placement, or give --sequence to reorder in bulk. Items
named in --sequence move to the front in the order given; everything else keeps
its relative order behind them.",
        after_long_help = "\
Examples:

  docket reorder b71c --top
  docket reorder b71c --position 2
  docket reorder b71c --after a3f9
  docket reorder --sequence k7m2,b71c,a3f9"
    )]
    Reorder(ReorderArgs),

    /// Advance an item one rung along the ladder.
    #[command(
        long_about = "\
Promotion runs forward only:

    handoff ──▶ relay ──▶ spec:design ──▶ spec:implementation
       └────────────────▶

With no flag, an item advances one step — including a spec moving from design
to implementation. Use --to spec on a handoff when the relay rung is not the
right intermediate step.

Promotion is additive: every field an item already carries survives, so a spec
reached through a relay keeps its whole chain provenance.",
        after_long_help = "\
Examples:

  docket promote b71c              one rung
  docket promote b71c --to spec    straight to a spec, skipping the relay rung"
    )]
    Promote(PromoteArgs),

    /// Consume a relay: open its successor and archive it.
    #[command(
        long_about = "\
A relay owes a successor. This mints it — same chain, next hop, superseding the
item it came from — and archives the predecessor in one step, so a chain can
never end by accident or double up.

Only a relay can be relayed. Promote a handoff first.",
        after_long_help = "\
Example:

  docket relay a3f9 --title 'Wave 2: migrate the remaining suites' \\
      --description 'Wave 1 landed green. Xdebug is wired; the fixtures are not.'"
    )]
    Relay(RelayArgs),

    /// Archive an item whose work is done.
    Close(IdArgs),

    /// Remove an item outright, leaving no archive copy.
    #[command(after_long_help = "\
Prefer `close`, which archives. This is for an item opened by mistake.")]
    Delete(DeleteArgs),

    /// Check the depot for damage.
    #[command(long_about = "\
Reports items whose frontmatter will not parse, items whose recorded project no
longer matches where they sit, stale items, and whether the session-start
announcement is wired up.

Read-only. Nothing here changes the depot.")]
    Doctor,

    /// Emit the session-start announcement.
    #[command(long_about = "\
Prints the outstanding work for the working directory's project. With --hook it
emits the JSON a Claude Code SessionStart hook consumes, stays silent when
nothing is outstanding, and always exits zero.")]
    Announce(AnnounceArgs),

    /// Explain a topic, or a command.
    Help(HelpArgs),

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

    /// Only items whose frontmatter will not parse.
    #[arg(long)]
    pub invalid: bool,

    /// What has been closed, instead of what is open.
    #[arg(long)]
    pub archived: bool,
}

#[derive(Args)]
pub struct CreateArgs {
    /// Which rung to open at.
    #[arg(value_enum)]
    pub kind: Kind,

    /// One line, shown in every listing. `-` reads standard input.
    #[arg(long)]
    pub title: String,

    /// The abstract a future session reads first. `-` reads standard input.
    #[arg(long)]
    pub description: String,

    /// Open it for another project. Defaults to this one.
    #[arg(long, value_name = "PATH")]
    pub to: Option<PathBuf>,

    /// Allow a target directory that does not exist yet.
    #[arg(long)]
    pub allow_missing: bool,

    /// Body to write, instead of leaving the file for you to fill in. `-`
    /// reads standard input.
    #[arg(long)]
    pub body: Option<String>,
}

#[derive(Args)]
pub struct IdArgs {
    /// Four-character item id, as printed by any listing.
    pub id: String,
}

#[derive(Args)]
pub struct DeleteArgs {
    /// Four-character item id, as printed by any listing.
    pub id: String,

    /// Skip the confirmation.
    #[arg(short, long)]
    pub force: bool,
}

#[derive(Args)]
pub struct SetArgs {
    /// Four-character item id, as printed by any listing.
    pub id: String,

    /// Replace the title. `-` reads standard input.
    #[arg(long)]
    pub title: Option<String>,

    /// Replace the description. `-` reads standard input.
    #[arg(long)]
    pub description: Option<String>,

    /// Record what must clear before this item can move. `-` reads standard
    /// input.
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

    /// Jump to a rung instead of advancing one step.
    #[arg(long, value_enum)]
    pub to: Option<Kind>,
}

#[derive(Args)]
pub struct RelayArgs {
    /// The relay being consumed.
    pub id: String,

    /// Title of the successor. `-` reads standard input.
    #[arg(long)]
    pub title: String,

    /// Description of the successor. `-` reads standard input.
    #[arg(long)]
    pub description: String,

    /// Body of the successor. `-` reads standard input.
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
pub struct CompletionArgs {
    /// Shell to generate for.
    #[arg(value_enum)]
    pub shell: clap_complete::Shell,
}
