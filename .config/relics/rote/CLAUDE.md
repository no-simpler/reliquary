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
drill/screen.rs the drill's own layout, as a pure function from a frame to a card
tui/mod.rs      the traits, the one entry loop, and the shared prompt dialogs
tui/card.rs     the box, the palette, and the one entry line
tui/term.rs     crossterm: raw mode, the alternate screen, placement, restoration
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
re-enrollment and carries no history, so it belongs where losing the local copy
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
  still say memorised. `--force` is a re-enrollment and the log records it as one.
- **Bracketed paste is enabled so a paste can be refused.** A drill answered
  from a vault measures nothing.
- **The lookup is only ever offered after an entry is recorded.** There is no
  standalone verb for an aided entry and there should not be: one would let the
  vault be consulted before anything is written, and the cold attempt — the
  whole reading — would go unrecorded. `--aided` is the declaration for a
  sitting that already went that way, not a shortcut past it.
- **Three commands are dialogs and the rest are not.** `add`, `rekey` and the
  drill have to be typed at; everything else answers `--format json` and is a
  script's to call. A dialog opens the alternate screen, holds its outcome until
  a key is pressed, and leaves nothing in scrollback. `--stdin` is not a dialog
  and never opens one. The two paths that say one thing and ask nothing —
  nothing due, and no verifier for anything in the plan — stay inline: a screen
  that opens to say *nothing* and then demands a keypress is hostile on the most
  frequent path there is.
- **One entry loop, one field, one place a secret becomes pixels.**
  `tui::read_secret` is the only loop that accepts a typed secret, `Card::entry`
  the only thing that draws one, and `Reveal::shown` the only function that turns
  a buffer into something on a screen. Revealing anything — per-character masking
  is defensible — is a change to that one match. The typed count is already
  passed in and deliberately unused, so that change costs no call site.
  Word-boundary masking is never defensible: seven word lengths is most of a
  diceware phrase's search space.
- **The field is its own delimited area and shares its line with nothing.** A
  hint beside where a secret is typed is a hint that will one day be overlapped
  by what is typed into it — so instructions sit on their own lines above, with
  one blank line before the label-and-field block.
- **Every state of one window is the same rectangle in the same place.** A window
  reserves its lines with `Card::reserve` and each state fills them; short states
  are padded and a state that overruns is cut. What is above the field is the
  heading and does not move, what changes lives in a reserved column or on the
  line under the field, and the field itself never moves. A cut line is the
  signal that the layout or the wording wants shortening — never the box.
- **A refusal is a flash and a counter, not a paragraph.** The field border goes
  red for `FLASH` and comes back; nothing red stays on the screen, and the try
  count increments in the slot it already occupied. A mark that stays is a mark
  every later glance has to re-read, and a menu that must be answered turns a
  mistyped character into an interrogation.
- **A lapse is the ladder going back to the foot, and nothing else is one.**
  Practice and probes move no schedule, so a first-try failure there is a *miss*.
  `Sitting::landings` buckets each slug once and the buckets are disjoint, so the
  closing count adds up to what was drilled instead of setting a count of classes
  beside a count of one outcome. A lone slug is described rather than counted.
  **A lookup re-labels its whole slug**, whatever the cold try did: the row's own
  chip already says aided, and a closing line reporting the cold try beside it
  would tell two stories about one slug. Week one is lookups almost throughout,
  and reading that back as a run of lapses would punish the honest shape of it.
  The cold failure is not lost — it stays in the log, in the ladder, in `stats`
  and in the exit status, which is where a measurement belongs rather than in a
  summary line.
- **A glyph carries what a glyph can.** One column of standing per row, and words
  only for what the glyph cannot say. Nothing on a card spells out a key that a
  person already knows — enter submits, escape leaves — so the only key named is
  ctrl-l, and only where it is on offer. The full set is `rote help keys`, which
  is reference and belongs there rather than on the screen every day.
- **The caret is the terminal's own cursor**, moved into the field and shown
  there. It blinks the way every other password field on the machine blinks,
  follows the reader's own cursor settings, costs no animation loop, and cannot
  be overlapped by text the way a glyph on a shared line can. The card draws no
  caret; it reports where one belongs.
- **A command opens a dialog once.** `cmd::dialog` is the only caller of
  `Terminal::enter` outside the sitting. Raw mode and the alternate screen are
  process-wide, so a second `Terminal`, even one immediately discarded, restores
  the terminal under the live one and turns echo on beneath the next prompt.
  `Terminal::enter` refuses a second, loudly, because that failure mode is
  otherwise silent and leaks the secret to the screen.
- **Every card is `card::WIDTH` wide.** A box that resizes as content comes and
  goes reads as instability and makes the eye re-find the border. Height is the
  axis that varies, and `Screen::anchor` fixes the top edge so that variation
  grows downward. A line that outgrows `CONTENT` is cut at the single render
  choke point rather than breaking the box, and a test asserts real content never
  reaches that net.
- **Every line is placed with its own `MoveTo`.** Raw mode takes the line
  discipline away, so a newline written while it is held moves down without
  returning to column zero and everything after it starts under the end of the
  line before. Nothing writes a newline in raw mode; a test asserts no rendered
  line carries one.
- **Placement, resize and a terminal too small are the adapter's.** The drill
  never learns that terminal size exists. A resize repaints from the last card
  rather than waiting for a keystroke, and a terminal below the floor gets a
  plain refusal — the dialog has a fixed shape and nothing about it degrades
  usefully.
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
  rewrite an earlier record; a whole-file replacement re-serializes the past on
  every save, and a bug in that path rewrites history rather than failing.
- **Local civil dates**, where the lane elsewhere uses UTC instants and day
  differences. A daily ritual is a calendar concept and a UTC boundary falls in
  the small hours here. Each record freezes the day it was credited to, so a
  later change of timezone cannot re-date history.
