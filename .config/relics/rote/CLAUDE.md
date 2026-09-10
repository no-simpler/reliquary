# `rote` — in-house (Stage-2) relic

The binary documents itself, across two namespaces that never overlap.
**Reference** — flags, the record schema, the files, exit codes — is `rote help`.
**Doctrine** — the ladder, irregularity, custody, probes — is `rote guide`.
**Do not restate any of that here, in a README, or in a code comment.** This
relic evolves; a second copy would be wrong within a week.

There is **no skill** at `~/.claude/skills/rote/`, and there should not be. No
agent operates `rote`: it refuses to run wherever `relic_core::ui::Format`
resolves to anything but `Human`, which `CLAUDECODE` alone triggers. A stub
whose only effect would be to advertise a tool an agent cannot use is noise.

## Prose rules for everything the binary prints

- **No backticks.** Help, guide, notes and errors are read in a terminal.
- **Canonical vocabulary**, one word per concept across the flag, the record
  field and the prose: *slug*, *step*, *review*, *practice*, *probe*, *lapse*,
  *stretch*, *cold*, *horizon*, *gate*.
- Full stops end sentences and fragments; bare listings take none.
- Refer to command help as `rote help <command>`, never `rote <command> --help`.
- Columns run in order of increasing variability, so the widest cell is last and
  is never padded.

## Layout

```
cli.rs          clap derive; doc comments are the help text
cmd.rs          dispatch, and what each command does
config.rs       ~/.config/rote/config.toml, or its defaults
doctor.rs       findings in relic_core's vocabulary, and their human shape
drill/mod.rs    the plan, and the sitting loop
drill/screen.rs the card, as a pure function from a frame to lines
drill/term.rs   crossterm: raw mode, the alternate screen, restoration
guide.rs        doctrine
help.rs         reference
ladder.rs       intervals, standing, the cutover gate — pure
log.rs          the record types, the hash chain, total parsing
model.rs        the projection: what the log adds up to
render/         one row model, three renderers
secret.rs       the typed value, and the only type that holds one
slug.rs         a validated name
stats.rs        retention, latency, punctuality — pure
store.rs        the two homes, the clock, locking, appending
verifier.rs     argon2id in PHC string format
```

## The two homes, and why they are two

`~/Trove/ark/rote/log.jsonl` holds outcomes, timings and intervals. Not
regenerable, no secret material, restic's to keep.

`~/.local/state/rote/verifiers.toml` holds the PHC strings, and never leaves the
machine. An argon2id verifier is a confirmation oracle — unlimited offline
guessing against an unambiguous success signal — and that is an accepted shape
for a ninety-bit phrase and a poor one for a login password. Putting one in
`ark` would replicate it to two providers, into every snapshot and into a month
of prior versions, where no rotation can reach it. A verifier is regenerable by
re-enrolment and carries no history, so it belongs where losing the local copy
loses nothing, which is the placement rule `POSTURE`'s `design/layout.md` states.

A slug with no verifier is therefore a **first-class state**, not a corruption:
it is what a restore onto a new machine looks like. Never add a fallback that
invents one.

A rewritable file is also what lets a rotation *forget*. An append-only
structure could not, and a retired verifier is an oracle for a retired secret.

## Constraints

Things a future edit must not undo.

- **The typed value lives only in `secret::Secret`.** Its buffer never
  reallocates, it zeroizes its whole capacity, and it has no `Display` and no
  `Serialize`. Every path to the bytes goes through `Secret::expose`, so
  auditing the crate is one grep. Deliberately **not** `secrecy`:
  `SecretString` is built through `String::into_boxed_str`, which shrinks to fit
  and so copies exactly the buffer this type exists to avoid copying.
- **No secret ever reaches argv, the environment, the clipboard, a formatter, a
  log line, an error, or the screen.** The drill is blind and there is no reveal
  toggle: it would have been the one feature that puts plaintext on a display,
  for a benefit a retry already covers.
- **`--stdin` refuses when any standard stream is a terminal.** That is what
  stops a pipe becoming a habit, because a secret typed into a shell command
  lands in shell history. The drill accepts no stdin at all.
- **`rekey` proves the current secret before accepting a new one.** Without it a
  verifier could be replaced by one somebody else knows, and the reading would
  still say memorised. `--force` is a re-enrolment and the log records it as one.
- **Bracketed paste is enabled so a paste can be refused.** A drill answered
  from a vault measures nothing.
- **The lookup is only ever offered after an entry is recorded.** There is no
  standalone verb for an aided entry and there should not be: one would let the
  vault be consulted before anything is written, and the cold attempt — the
  whole reading — would go unrecorded. `--aided` is the declaration for a
  sitting that already went that way, not a shortcut past it.
- **Everything written while raw mode is held goes through `RawLines`.** Raw
  mode takes the line discipline away, so a bare newline moves down without
  returning to column zero and whatever comes next starts under the end of the
  line before it. The writer borrows the guard, so it cannot be obtained without
  raw mode and cannot outlive it, and the translation is idempotent so a call
  site that spells the break either way produces the same bytes.
- **The terminal is put back three ways**, because each covers a way of leaving
  the others do not: an RAII guard, a panic hook, and a `signal-hook` thread. A
  default-disposition `SIGTERM` runs no destructor, and a terminal left in raw
  mode with echo off shows nothing of what is typed into it next.
- **Replay reads `step_after` rather than recomputing it.** The step a record
  carries is what the tool decided at the time, which makes history immune to a
  later change in the ladder.
- **A malformed line is kept, never dropped**, and a record from a newer schema
  is a loud refusal to write rather than a partial read.
- **`doctor` never writes.** `assay`'s registry station asserts that of every
  binary it collects, on a two-second budget, so nothing in that path hashes.
- **Every test sets `ROTE_ROOT` and `ROTE_STATE`.** They are seams, along with
  `ROTE_CONFIG`, `ROTE_UI` and `ROTE_HOST`. There is deliberately **no seam that
  lowers the KDF cost**: the suite seeds a store instead, and mints one verifier
  at the shipped parameters for the whole run.
- Publishing and testing carry no per-relic scripts. Do not reintroduce
  `scripts/publish.sh` or `scripts/test.sh`.

## Measured, so it is not re-derived

Argon2id on the flagship, 2026-09-10, release build, `m=262144 p=1`:

| `t` | one verification |
|---|---|
| 1 | 112 ms |
| 2 | 230 ms |
| 3 | 350 ms |
| **4** | **468 ms** |
| 5 | 590 ms |

`t = 4` is shipped, against `age -p`'s measured 450 ms at the same 262 MiB. `p`
buys nothing: lanes divide the same memory rather than adding passes over it.

An unoptimised build costs 6.2 s a call, which is why the workspace carries
`[profile.dev.package.argon2] opt-level = 3`. Without it the suite is unusable
rather than merely slow.

The crate's `zeroize` feature wipes the initial hash and the finalisation block.
It does **not** wipe the 256 MiB working array, so `verifier::digest_bytes`
supplies that buffer itself and zeroizes it. Inverting argon2's internal state is
as hard as inverting BLAKE2b, so this is belt rather than braces — the belt costs
one `Vec`, and the two cross-checks against the reference implementation are what
keep the hand-assembled PHC string honest.

## Format choices that are not the lane's habit

- **JSON Lines**, where the lane elsewhere uses frontmatter documents and
  whole-file JSON. An append is one `O_APPEND` line write, so no later write can
  rewrite an earlier record; a whole-file replacement re-serialises the past on
  every save, and a bug in that path rewrites history rather than failing.
- **Local civil dates**, where the lane elsewhere uses UTC instants and day
  differences. A daily ritual is a calendar concept and a UTC boundary falls in
  the small hours here. Each record freezes the day it was credited to, so a
  later change of timezone cannot re-date history.
