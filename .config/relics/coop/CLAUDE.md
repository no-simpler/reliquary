# `coop` — in-house (Stage-2) relic

The binary documents itself, across two namespaces that never overlap.
**Reference** — the tiers, the keys, the files, the wiring — is `coop help`.
**Doctrine** — what belongs in an inbox and what does not — is `coop guide`.
**Do not restate any of that here, in a README, or in a code comment.** This
relic evolves; a second copy would be wrong within a week.

There is **no skill** at `~/.claude/skills/coop/`, and there should not be. The
card is drawn only when `relic_core::ui::Format` resolves to `Human`, which
`CLAUDECODE` alone rules out, so no agent ever sees one. What an agent might
need — how to add a source — is `coop help tiers`, reachable without a stub
advertising a tool it cannot use.

## Prose rules for everything the binary prints

- **No backticks** in `help` and `guide` topic bodies. They are read in a
  terminal, where a backtick is a backtick. Clap doc comments are the exception
  and not a choice: `clippy::doc_markdown` requires them, and `--help` already
  renders relic-core's own backticked docs.
- **Canonical vocabulary**, one word per concept across the flag, the field and
  the prose: *source*, *declaration*, *notice*, *tier*, *key*, *card*, *badge*,
  *tick*, *dormant*.
- Full stops end sentences and fragments; bare listings take none.

## Layout

```
ask.rs        the asked tier: running a producer, and never on the prompt path
cache.rs      what a producer last said, and the terms on which it is reused
card.rs       the card, as a pure function from notices to lines
cli.rs        clap derive; doc comments are the help text
cmd.rs        dispatch, and what each command does
config.rs     style and width, and nothing that is not a preference
doctor.rs     findings in relic-core's vocabulary, and their human shape
guide.rs      doctrine
help.rs       reference
notice.rs     the outstanding set, its identity, and its digest
paths.rs      the two trees, and tilde expansion
predicate.rs  the stat tier — pure, clock injected
render.rs     one row model, three readers
session.rs    what one shell has already been shown
source.rs     the declaration: parse, validate, compile
span.rs       durations as a declaration writes them
state.rs      first-seen and the tick measurement, for doctor
```

## Constraints

Things a future edit must not undo.

- **The prompt path never waits on a producer.** A cached answer is rendered
  while its replacement is fetched in a detached child. The one exception is a
  genuinely cold cache, capped by `ask::COLD_BUDGET`, which happens once per
  source per machine and is the difference between a first shell that says
  something and one that says nothing.
- **`tick` cannot fail a shell.** It returns zero whatever happened, and `main`
  discards its result deliberately. A broken declaration, an unreadable cache
  and a producer that will not run all resolve to an empty prompt, because a
  diagnostic printed into the middle of a prompt is worse than the thing it
  reports.
- **The card is edge-triggered.** Not a preference, and not in `config.toml`:
  a repeated card stops being read, and fish's `fish_prompt` is a
  multi-subscriber event that promises nothing about repaints, so a
  level-triggered card would redraw on a window resize.
- **Every shell hook saves and restores the exit status.** oh-my-posh reads
  `$?` and `$PIPESTATUS` at the top of its own hook, which runs last. This was
  measured, not assumed — and fish, also measured, restores `$status` for
  `fish_prompt` by itself, which is why its twin carries no guard.
- **A `Note` never reaches the card.** relic-core defines it as read but not
  graded, which is the opposite of dismissible. It is dropped in
  `notice::actionable`, not filtered by a configurable floor, because a floor is
  something somebody eventually lowers.
- **A stat declaration without a fix is refused at compile time.** The whole
  admission test for this inbox is that a notice can be got rid of today.
- **The digest is FNV-1a, hand-rolled.** `DefaultHasher` is not promised stable
  across std releases, and this value is written to disk: a silent change would
  re-announce every notice on the machine after a toolchain bump.
- **A producer's escape sequences are stripped.** A text producer was written
  for a person and colours itself; the card owns its own colour, and a smuggled
  sequence would also let a producer move the cursor out of the box.
- **A failed ask is cached like a successful one.** Otherwise the cache stays
  stale and a reliably broken producer forks a background refresh from every
  prompt on the machine, forever. It is stored as `Outcome::Skipped`, so it
  carries no findings to the card and `doctor` reports it instead — a producer
  that will not run is not the person's nag.
- **The exit status means different things across the two kinds.** A findings
  producer reports its grade through it; a text producer has no such channel,
  so there it means what it usually means.
- **A producer never reports under another source's name.** `ask::relabel`
  keeps a producer's own namespace and moves anything else onto the declared
  id, which is the rule `assay` enforces on the binaries it collects.
- **The budget measures the silent path only.** A tick that warmed a cold
  producer or drew a card is not the steady state, and timing either would
  report a hook that is slow when it is not.
- **A session id from the environment is untrusted.** It chooses a filename, so
  `session::sanitize` reduces it to characters that cannot leave the directory.
- Publishing and testing carry no per-relic scripts. Do not reintroduce
  `scripts/publish.sh` or `scripts/test.sh`.

## Dependencies that are not the lane's habit

`crossterm` for terminal width alone, because `unsafe_code = "forbid"` rules out
the ioctl and no width crate is in the tree. `toml` is declared here rather than
in `[workspace.dependencies]`, matching `warden`, `assay` and `rote` — four
consumers now share it against a table whose own comment says two is the
threshold, which is drift worth fixing in one edit rather than four.

## Measured, so it is not re-derived

Flagship, 2026-09-10, release build, two declared sources:

| path | cost |
|---|---|
| warm tick, whole process | 2.9 ms |
| warm tick, in-process work | 0.18 ms |
| `ske prompt`, for comparison | 10.8 ms |

The budget in `state::TICK_BUDGET_MICROS` is set against the in-process figure,
which is the only part this relic controls; process spawn is the shell's.
