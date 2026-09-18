# `coop` — in-house (Stage-2) relic

The binary documents itself, across two namespaces that never overlap.
**Reference** — the tiers, the keys, the files, the wiring — is `coop help`.
**Doctrine** — what belongs in an inbox and what does not — is `coop guide`.
**Do not restate any of that here, in a README, or in a code comment.** This
relic evolves; a second copy would be wrong within a week.

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
ask.rs        the asked and read tiers: a producer's answer, taken fresh every prompt
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
records.rs    first-seen, for doctor
render.rs     one row model, three readers
source.rs     the declaration: parse, validate, compile
span.rs       durations as a declaration writes them
```

## Constraints

Things a future edit must not undo.

- **coop holds no answer of its own.** Every source is evaluated afresh before
  every prompt. A producer that needs a cache owns it, because only the
  producer knows when its truth changes; the `read` tier is how a slow one
  hands its cache over. Nothing under `~/.local/state/coop` is a producer's
  answer.
- **An asked producer is killed at `ask::ASK_BUDGET`.** Past it the source says
  nothing this prompt and `doctor` reports it `slow`. Slow means `read` or
  `when`, never a longer budget.
- **`tick` cannot fail a shell.** It returns zero whatever happened, and `main`
  discards its result deliberately. A broken declaration and a producer that
  will not run both resolve to an empty prompt, because a diagnostic printed
  into the middle of a prompt is worse than the thing it reports.
- **The card is edge-triggered, and the shell keeps the edge.** The badge file
  carries the count and the digest; the hook exports them as `COOP_BADGE` and
  `COOP_SEEN`, and the next tick draws only when the digest has moved. Per-shell
  state lives in the shell's environment for exactly as long as the shell does,
  so nothing per shell is written or swept. A nested shell inherits it and does
  not redraw, which is accepted. Not a preference, and not in `config.toml`: a
  repeated card stops being read, and fish's `fish_prompt` is a multi-subscriber
  event that promises nothing about repaints.
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
  across std releases, and this value is written to disk and into a shell's
  environment: a silent change would re-announce every notice on the machine
  after a toolchain bump.
- **A producer's escape sequences are stripped.** A text producer was written
  for a person and colours itself; the card owns its own colour, and a smuggled
  sequence would also let a producer move the cursor out of the box.
- **A failure is `doctor`'s, never the card's.** A producer that will not run
  or answers with something that is not an answer is `failing` in `coop
  sources` and a soft finding in `doctor`; the card shows nothing for it,
  because a broken producer is not the person's nag.
- **The exit status means different things across the two kinds.** A findings
  producer reports its grade through it; a text producer has no such channel,
  so there it means what it usually means.
- **A producer never reports under another source's name.** `ask::relabel`
  keeps a producer's own namespace and moves anything else onto the declared
  id, which is the rule `assay` enforces on the binaries it collects.
- Publishing and testing carry no per-relic scripts. Do not reintroduce
  `scripts/publish.sh` or `scripts/test.sh`.

## Dependencies that are not the lane's habit

`crossterm` for terminal width alone, because `unsafe_code = "forbid"` rules out
the ioctl and no width crate is in the tree. `toml` is declared here rather than
in `[workspace.dependencies]`, matching `warden`, `assay` and `rote` — four
consumers now share it against a table whose own comment says two is the
threshold, which is drift worth fixing in one edit rather than four.

## Measured, so it is not re-derived

Flagship, 2026-09-18, release build, two declared sources (`up` stat, `rote` asked):

| path | cost |
|---|---|
| warm tick, whole process, rote asked inside it | 3.3 ms |
| `rote banner --format json`, alone | 2.3 ms |
| `ske prompt`, for comparison | 10.8 ms |

`ask::ASK_BUDGET` is 50 ms per producer: an order of magnitude over the one
producer that exists, and under what a prompt can carry unnoticed.
