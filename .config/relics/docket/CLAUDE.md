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
  so one word covers all three.
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
a relay cannot lack its chain. `src/store.rs` the depot: project keys, metadata,
atomic writes, locking. `src/render/` one module per output shape, over the
shared row model. `src/cmd.rs` the commands. `src/help.rs` reference topics;
`src/guide.rs` doctrine.

## Constraints

`install_on_path` copies the built binary, so nothing may be read from beside
the executable at runtime.

`scripts/publish.sh` overrides the default because the entrypoint symlink dangles
until `cargo build --release` has run — see the note in the script.

`relic::test` dispatches on `RUNTIME`, and `RUNTIME="rust"` runs
`scripts/test.sh`: format, then clippy at `-D warnings`, then the suite.

Every test must set `DOCKET_ROOT` to a scratch directory. A test that forgets it
writes into the live depot.
