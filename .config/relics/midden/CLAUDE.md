# midden

The binary documents itself, across two namespaces that never overlap.
**Reference** is `midden help`, which serves both a topic and any command.
**Doctrine** — what earns a note, and what to do with the heap — is
`midden guide`.

**Do not restate any of that here, in a README, or in a code comment.** This
relic evolves; a second copy would be wrong within a week. To learn the tool, run
it. To change what it teaches, edit the clap definitions in `src/cli.rs`, the
reference topics in `src/help.rs`, and the doctrine in `src/guide.rs` — doc
comments on the command structs *are* the help text, so there is one copy by
construction.

There is deliberately no skill. Nothing announces midden ambiently: it is
reached for through the `+midden` mode, or not at all. A trigger stub would put
it in front of every session, and a corpus filled reflexively is one nobody
reads.

Both namespaces are kept at the floor of what an agent needs to act correctly.
Prose that will not fit belongs to a different owner, not to a longer page.

## Prose rules for everything the binary prints

These bind help text, guide text, notes and error messages alike.

- **No backticks.** Structure signals what is a command and what is prose; a
  backtick in a terminal is a stray character.
- **Canonical vocabulary.** *metadata*, never frontmatter. *kind*, never type or
  category. *note*, never entry or finding. *cause*, never issue. It is the
  metadata key and the `--kind` flag, so one word covers both.
- **Full stops** end sentences and sentence fragments. Bare listings — command
  lines, values, key tables — take none.
- **Refer to command help as `midden help <command>`**, never
  `midden <command> --help`, even though clap serves both.
- **Columns align**, and the one unbounded column comes last so padding is never
  paid for at the end of a line. `src/render/mod.rs` owns that; `list` and
  `digest` render the same rows through it.

## Layout

`src/cli.rs` argument surface and help. `src/note.rs` the note itself: the
closed kind taxonomy, the status lifecycle, and the fingerprint that decides
whether two observations are one cause. `src/store.rs` the corpus: a flat shelf,
atomic writes, locking, and the retention boundaries. `src/render/` one module
per output shape, over the shared row model. `src/cmd.rs` the commands.
`src/help.rs` reference topics; `src/guide.rs` doctrine.

## Constraints

The corpus is **flat and machine-wide**, unlike a docket depot. Cross-project
pattern detection is the point: `project` is a field, not a directory, and the
fingerprint deliberately ignores it.

Retention is enforced by `midden gc`, which `scripts/update.sh` runs on every
`up`. That override exists only for that call — keep it non-interactive and
time-bounded, or it stalls the machine-wide update.

`install_on_path` copies the built binary, so nothing may be read from beside
the executable at runtime.

`scripts/publish.sh` overrides the default because the entrypoint symlink dangles
until `cargo build --release` has run — see the note in the script.

`relic::test` dispatches on `RUNTIME`, and `RUNTIME="rust"` runs
`scripts/test.sh`: format, then clippy at `-D warnings`, then the suite.

Every test must set `MIDDEN_ROOT` to a scratch directory. A test that forgets it
writes into the live corpus.

Ages are whole days by truncation, so a test that backdates by exactly a
retention boundary reads one day short of it. Backdate clear of the boundary.
