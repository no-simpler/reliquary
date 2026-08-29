# docket

The binary documents itself, across two namespaces that never overlap.
**Reference** is `docket help`, which serves both a topic and any command.
**Doctrine** — what an item is for, and which kind it belongs at — is
`docket guide`.

**Do not restate any of that here, in a README, or in a code comment.** This
relic evolves; a second copy would be wrong within a week. To learn the tool, run
it. To change what it teaches, edit the clap definitions in `src/cli.rs`, the
reference topics in `src/help.rs`, and the doctrine in `src/guide.rs` — doc
comments on the command structs *are* the help text, so there is one copy by
construction.

The machine-wide skill at `~/.claude/skills/docket/` is a trigger stub and
nothing more: its `description` is the only channel that reaches an agent's
context without a tool call, and its body points at `docket guide`. Doctrine
never lives there, because the skill is a Claude Code artifact and this binary is
not.

Both namespaces are kept at the floor of what an agent needs to act correctly.
Prose that will not fit belongs to a different owner, not to a longer page.

## Prose rules for everything the binary prints

These bind help text, guide text, notes and error messages alike.

- **No backticks.** Structure signals what is a command and what is prose; a
  backtick in a terminal is a stray character.
- **Canonical vocabulary.** *metadata*, never frontmatter. *kind*, never rung
  or type. It is the metadata key, the `--kind` flag and the `create` argument,
  so one word covers all three. *name*, never title — it is the metadata key,
  the `--name` flag, half of every filename, and a handle that resolves an item
  wherever an id does.
- **Full stops** end sentences and sentence fragments. Bare listings — command
  lines, values, key tables — take none.
- **Refer to command help as `docket help <command>`**, never
  `docket <command> --help`, even though clap serves both.
- **Columns align**, and the one unbounded column comes last so padding is never
  paid for at the end of a line. `src/render/mod.rs` owns that; `list` and
  `announce` render the same rows through it.

## Layout

`src/cli.rs` argument surface and help. `src/item.rs` the typed ladder — a kind
is only constructible in a valid shape, so `stage` cannot exist on a handoff and
a relay cannot lack its chain. `src/store.rs` the depot: metadata, atomic
writes, locking. `src/git.rs` the depot's history. `src/query.rs` what a
listing asks the depot for. `src/render/` one module per output shape, over the
shared row model. `src/cmd.rs` the commands. `src/help.rs` reference topics;
`src/guide.rs` doctrine.

## The git layer

`src/git.rs` is the only module that names git, and nothing else may shell out
to it. It owns the **depot's history** — one repository over the whole depot.

*How* git is invoked is not docket's: `relic-core::git` owns the constructor that
strips the ambient `GIT_*` environment, and `src/git.rs` re-exports `Git` and
`detect` so callers here are unchanged. Project keys come from
`relic_core::path::project_key`, shared with `midden` so a note and an item about
one repository cannot key differently. Neither may be reimplemented here.

It is **additive** — ask `git::detect` and take the ungit path when it answers
nothing — with exactly one exception. `close` is refused without a repository,
because closing removes the item and history is the only thing that keeps it.
That asymmetry is deliberate; do not smooth it out.

What it buys is the invariant the design rests on: **no docket operation can
destroy work that history does not already hold.** Every command but `close`
preserves the body byte for byte, so an edit made outside docket cannot be lost
by one — and `close` is gated on `is_recorded`. That is a conditional-write
precondition, not a lock, and it is why nothing needs to observe an outside edit
as it happens. The drift snapshot serves the log, not safety; do not redesign
around its latency.

Three properties are load-bearing, and each is one line away from being lost:

- **Every invocation goes through `Git::command`**, which strips the inherited
  `GIT_*` environment. docket run from inside a git hook would otherwise answer
  for that hook's repository rather than the one under its own feet.
- **The depot's identity and `commit.gpgsign=false` live in its own config**,
  written once at init. This machine signs through a 1Password-backed program,
  so a signed depot commit means a Touch ID prompt on every mutation. Precedent:
  `~/Developer/clc/clc.sh`, `store_init`.
- **The depot lock is taken before the per-project lock**, never the other way
  round. `cmd::Mutation` holds it for a whole command; `git::Repo` takes no lock
  of its own.

A mutating command commits twice: what was edited outside docket, then its own
change. Bodies are authored through the path docket prints, so the first commit
is what keeps that work from riding along under someone else's message.
`announce` does the first alone, and takes the lock only if it is free — a
session-start hook that waits on another session is a hook that hangs a
terminal.

## Constraints

**A rename is a rename.** No serde alias, no fallback read, no dual spelling.
`Wire` denies unknown keys, so an item written under a superseded key fails to
parse, lists as invalid naming the key, and is rebuilt by `set`. Compatibility
shims read as kindness and age into a schema nobody can state in one sentence.
Leniency belongs to values — length on the way in, case and separators on a
name — never to keys.

`install_on_path` copies the built binary, so nothing may be read from beside
the executable at runtime.

Publishing and testing carry **no per-relic scripts**. `relic publish` builds
and installs from the workspace `target/release/`, and `relic test` runs format,
then clippy at `-D warnings`, then the suite — over this crate and every
`crates/*` member. Do not reintroduce `scripts/publish.sh` or `scripts/test.sh`;
the lib's rust branch is the whole story.

Every test must set `DOCKET_ROOT` to a scratch directory. A test that forgets it
writes into the live depot. `HOME` points at the same scratch tree, which is
also what keeps git from reading the machine's global config.

`RELIC_GIT` is the seam, and it belongs to `relic-core`: a path overrides the
binary, and an empty value takes the ungit path. It is how the refusal to close is
tested at all.

`scripts/update.sh` overrides the default because packing the depot is a real
periodic job. It calls `relic publish` itself — see the note in the script.
