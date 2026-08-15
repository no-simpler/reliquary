# docket

The binary documents itself. `docket --help` carries the model and the command
groups, every subcommand's `--help` carries its contract and examples, and
`docket help ladder|metadata|keys|agent` carries what belongs to no single
command.

**Do not restate any of that here, in a README, or in a code comment.** This
relic evolves; a second copy of its usage would be wrong within a week. To learn
the tool, run it. To change what it teaches, edit the clap definitions in
`src/cli.rs` and the topic bodies in `src/help.rs` — doc comments on the command
structs *are* the help text, so there is one copy by construction.

The same rule governs the machine-wide skill at `~/.claude/skills/docket/`: it
carries workflow policy and points at `docket help` for every command shape.

## Layout

`src/cli.rs` argument surface and help. `src/item.rs` the typed ladder — a rung
is only constructible in a valid shape, so `stage` cannot exist on a handoff and
a relay cannot lack its chain. `src/store.rs` the depot: project keys,
frontmatter, atomic writes, locking. `src/render/` one module per output shape.
`src/cmd.rs` the commands. `src/help.rs` topic bodies.

## Constraints

`install_on_path` copies the built binary, so nothing may be read from beside
the executable at runtime.

`scripts/publish.sh` overrides the default because the entrypoint symlink dangles
until `cargo build --release` has run — see the note in the script.

`relic::test` dispatches on `RUNTIME`, and `RUNTIME="rust"` runs
`scripts/test.sh`: format, then clippy at `-D warnings`, then the suite.

Every test must set `DOCKET_ROOT` to a scratch directory. A test that forgets it
writes into the live depot.
