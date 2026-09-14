# `rote` — in-house (Stage-2) relic

The binary documents itself, across two namespaces that never overlap.
**Reference** — flags, the record schema, the files, the machines, exit codes — is
`rote help`. **Doctrine** — the ladder, irregularity, custody — is `rote guide`.
**Do not restate any of that here, in a README, or in a code comment.** This relic
evolves; a second copy would be wrong within a week.

## What the tool is for

**`rote` keeps a faithful record and proposes a schedule. It does not judge, and it
does not gate.** The human applies an actual schedule — more often than proposed,
matching it, or less — and the tool reconciles the difference and records what
happened. Every threshold produces a number in `stats` and never a verdict.

**The good faith principle.** It guards against accident and confusion, never
against malice. The record could be made to say anything; the author is trusted not
to, and at worst to do it by mistake. Every guard here is an accident guard: the
flagship marker, the dossier named before an attachment, the refusal to enrol over
a live lineage. None of them is a lock, and none should become one.

**The storage rule that follows.** A value may be stored when it is underivable or
expensive to derive. A judgement may not be stored at all, because it should not
exist. A derived **reading** has exactly one home — `stats` when it carries a
threshold, the dossier when it does not — and is not also reported by `doctor`.

A **defect** is not a reading, and the rule inverts for one: a state with a
remedy has exactly one home and it is always `doctor`, because that is the only
surface `assay` collects into `yadm doctor`. `aided_mismatch` is the worked
example. `doctor` states it and names the fix; the sitting announces the
**edge**, in both directions, at the moment it moves — an event names a moment,
so it cannot go stale the way a second copy of a standing claim does. `stats`
carries the historical count and attaches no consequence to it, which is how the
two came to contradict each other: a refusal followed by a pass clears the
dossier and leaves a ninety-day count still asserting the disagreement.

## Prose rules for everything the binary prints

- **No backticks.** Help, guide, notes and errors are read in a terminal.
- **Canonical vocabulary**, one word per concept across the flag, the record field
  and the prose: *corpus*, *chain*, *lineage*, *slug*, *engram*, *dossier*,
  *sitting*, *turn*, *drill*, *attachment*, *capture*, *rung*, *occasion*, *aided*,
  *streak*, *machine*, *hostname*, *attached*, *dormant*, *review*, *practice*,
  *lapse*, *cold*.
- **`aided` is an adjective on a capture, never a third occasion.** The occasion
  says whether the schedule asked; aided says whether the memory was measured.
  Collapsing them is what made an aided review lose its due-ness.
- Full stops end sentences and fragments; bare listings take none.
- Refer to command help as `rote help <command>`, never `rote <command> --help`.
- Columns run in order of increasing variability, so the widest cell is last and is
  never padded.

## Layout

```
lib.rs            the crate, so the suite builds records with the real types
cli.rs            clap derive; doc comments are the help text
cmd/mod.rs        the context, dispatch, and what every command shares
cmd/sitting.rs    the daily call, the practice offer, and practice
cmd/enroll.rs     enroll, attach, rotate, retire
cmd/dialog.rs     the terminal side those four share
cmd/reading.rs    status, stats, log, machines
cmd/health.rs     doctor and banner
config.rs         ~/.config/rote/config.toml, or its defaults
corpus/mod.rs     the projection: lineages, engrams, dossiers
corpus/record.rs  the record types, the identities, the hash chain, total parsing
corpus/chain.rs   one file per machine, the filename grammar, the merge
corpus/drill.rs   the derived grouping over captures, and where one landed
doctor.rs         findings in relic_core's vocabulary, and their human shape
exit.rs           the four exit codes
intake.rs         the double-entry policy — pure, and shared with the sitting
ladder.rs         the schedule, the occasion, the standing — pure
machine.rs        the identity, the hostname, the flagship marker
render/           one row model, three renderers
secret.rs         the typed value, its editing by caret, and what holds one
sitting/mod.rs    the roster, and the loop that works through it
sitting/screen.rs the sitting's own layout, as a pure function from frame to card
slug.rs           a validated lineage name
stats.rs          retention, latency, punctuality, the streak — pure
store.rs          the three homes, the clock, locking, appending
tui/mod.rs        the traits, the entry loop, the key intents, render_field
tui/card.rs       the box and the one entry line; colour is relic_core::style
tui/term.rs       crossterm: raw mode, the alternate screen, placement, restoration
verifier/mod.rs   argon2id in PHC string format
verifier/file.rs  the machine-local file, keyed by engram
guide.rs          doctrine
help.rs           reference
```

## The three homes, and why they are three

`~/Trove/ark/rote/chains/` holds one append-only chain per machine: outcomes,
timings and intervals. Not regenerable, no secret material, restic's to keep.

`~/.local/state/rote/verifiers.toml` holds the PHC strings and never leaves the
machine. An argon2id verifier is a confirmation oracle — unlimited offline guessing
against an unambiguous success signal — an accepted shape for a ninety-bit phrase
and a poor one for a login password. Putting one in `ark` would replicate it to two
providers, into every snapshot and into a month of prior versions, where no
rotation can reach it. A verifier carries no history and is regenerable by typing
the secret again, so it belongs where losing the local copy loses nothing, which is
the placement rule `POSTURE`'s `design/layout.md` states.

`~/.local/state/reliquary/flagship` is the marker, which `rote` **reads and never
writes**. It says what this machine is, so it has to be machine-local by
construction: a marker that travelled with the dotfiles would declare every machine
the flagship.

An engram with no verifier here is therefore a **first-class state**, not a
corruption: it is what a restore onto a new machine looks like, and `attach` is how
it comes back. Never add a fallback that invents one — the sitting does not invent
a verifier, it asks for one.

**Attachment is machine-local, and the two halves may disagree.** The corpus
records attach *events*, permanently and as a visible seam; the machine's state dir
holds the attachment *fact*. Attaching on one machine and then wiping its state
leaves the event standing and the fact gone, which is correct. Neither is
authoritative over the other.

## Constraints

Things a future edit must not undo.

- **The typed value lives only in `secret`.** `Secret`'s buffer never reallocates,
  it zeroizes its whole capacity, and it has no `Display` and no `Serialize`. Every
  path to the bytes goes through `Secret::expose`, so auditing the crate is one
  grep. Deliberately **not** `secrecy`: `SecretString` is built through
  `String::into_boxed_str`, which shrinks to fit and so copies exactly the buffer
  this type exists to avoid copying. `Pasted` sits beside it on the same terms —
  one `expose`, a redacted `Debug`, wiped on drop, and neither `Copy` nor `Clone`
  — so what a paste carried is wiped whether the prompt takes it or refuses it,
  and `Key` gives up `Copy` to carry it rather than duplicating a secret in
  passing. Two residuals are accepted rather than closed: crossterm's own read buffer holds typed bytes transiently, and the signal
  path exits without running destructors — the kernel zeroes pages on reuse, swap
  is encrypted and core dumps are off, and the alternative is `mlock` through
  unsafe code. **A reveal adds a third, larger one**, accepted with its eyes
  open: `Field::drawn`, both of `Piece::painted`'s copies, `join`'s pair,
  `edge`'s `format!`, the `Vec<String>` from `render`, and stdout's `LineWriter`,
  which keeps the bytes after a flush with only its length reset. `Terminal::paint`
  wipes the outgoing frame unconditionally and a `Card` built over a shown field
  wipes its body on drop — which is also why `Card` is deliberately not `Clone` —
  but the transients above are not reachable. Under masking the caret also goes
  onto the wire as an absolute `MoveTo` on every paint, which is a per-keystroke
  length oracle in any recording of the stream.
- **A full buffer refuses the character and says so.** The dialog and the pipe hold
  one rule: a secret longer than the field is refused, never truncated into a
  verifier for something nobody typed.
- **`secret` never grows and never derives a word boundary.** Editing is by
  character index and nothing calls `Vec::insert`, `Vec::remove` or `Vec::drain`,
  each of which checks `len == capacity` and calls `reserve`. A removal shifts the
  tail down and `truncate` does not reach what it left above the new length, so
  `scrub` wipes that range first — its own function precisely because it is the
  only part of a removal a test in safe Rust can see. Word boundaries are free
  functions in the entry loop: a word length is the one thing about a passphrase
  this tool may never disclose, and the module that holds the value is the wrong
  place to teach how to find one.
- **Every capture is written the moment it is taken.** The sitting loop hands each
  one to a record callback before the next prompt is drawn, so a closed window or a
  signal keeps every reading it had already produced. The closing card is drawn
  from the corpus after the last write, not before it.
- **The verifier is written before the record of it.** A failure between the two
  then leaves a fact with no record — the legitimate disagreement — rather than a
  record with no fact, which is the direction that lies.
- **Every refusal a writer can meet lands in `Store::open`.** The flagship gate and
  the future-schema refusal both live there, because every caller opens the store
  in order to write and the refusal must arrive before any command has touched the
  verifier file.
- **A cold prompt gives back nothing about what was typed.** The cold drill try
  is the one prompt that measures a memory, and four behaviours are consequences
  of that one property rather than four settings: it refuses a paste, it stays
  blind, it offers no reveal and it moves no caret. A character count is partial
  recognition feedback delivered mid-retrieval and a reveal is the whole of one,
  so neither may reach the prompt that is measuring. `read_secret` takes `cold`
  and derives the rest; nothing else may grow a second switch.
- **No secret ever reaches argv, the environment, the clipboard, a formatter, a
  log line or an error.** The screen is the one exception and it is deliberate:
  every non-cold prompt masks per character and `ctrl-r` shows the characters
  until it is pressed again, which is what NIST SP 800-63B-4 asks a verifier to
  offer and what stops an enrollment minting a verifier for a typo nobody can
  detect. A reveal is never sticky — `revealed` is a local in `read_secret`, so
  it cannot outlive the prompt — and it concedes to a lost focus and to
  `CONCEAL` seconds of silence.
- **The disclosure-closure rule.** Under masking an affordance is offered only
  if its effect is fully determined by what is already disclosed: the total
  length and the caret. Word-wise motion fails that test — a jump the width of a
  word discloses the width of a word — so it waits for the characters to be
  showing. That is the same disclosure that rules out word-boundary masking,
  arriving through the side door, and every new key is checked against the rule
  rather than against the last key that was added.
- **The key map is readline's**, because readline is the muscle memory a terminal
  already has. Where readline names one intent twice both spellings arrive, and
  where a terminal encodes one intent twice both arrive too; `Key` is a set of
  intents and the loop never learns which spelling came. Two of readline's are
  refused rather than missing: `ctrl-y` and undo both need a store of prior
  buffer states, which is a plaintext copy with a lifetime of its own.
- **`--stdin` refuses when stdin is a terminal.** The precise threat is an echoing
  read: a secret typed into one lands on the screen and in scrollback. No stream
  check can see the command line, so the pipe's discipline — a vault at the other
  end, never an echo — is stated in `rote help stdin` and nowhere enforced. The
  sitting accepts no stdin at all.
- **`rotate` proves the current secret; `attach` proves nothing and says so at the
  prompt.** Without the proof a verifier could be replaced by one somebody else
  knows and the reading would still say memorised. `attach` is the honest opposite:
  it holds nothing to check against, so it records the claim and verifies neither —
  and it names the dossier it is about to continue first, which is an accident
  guard rather than a check.
- **A rotation removes the outgoing verifier.** Keyed by engram, setting the new one
  no longer overwrites the old, and a verifier for a secret that has been rotated
  away is a live oracle for it. `doctor` grades one Broken.
- **Bracketed paste is enabled so a cold try can refuse one.** Every other prompt
  takes a paste, because measurement is the whole of the line and nothing else
  here measures. The refusal is a commitment device and not a control: DECSET
  2004 is advisory, a terminal that ignores it delivers a paste as ordinary
  typing, and a vault typing into the window is invisible either way. So
  `paste_accepted` and `paste_refused` are counts of pastes and never counts of
  lookups, and nothing may describe them as the second. A paste too long for the
  field enters none of itself and counts as refused, so the two partition every
  paste a prompt saw.
- **The lookup is only ever offered after a capture is recorded, and never at an
  attachment prompt.** There is no standalone verb for an aided entry and there
  should not be: one would let the vault be consulted before anything is written,
  and the cold attempt — the whole reading — would go unrecorded. `--aided` is the
  declaration for a sitting that already went that way, not a shortcut past it. At
  an attachment prompt offering it would imply that what is typed is being checked
  against something.
- **Four commands are dialogs and the rest are not.** `enroll`, `attach`, `rotate`
  and the sitting have to be typed at; everything else answers `--format json` and
  is a script's to call. A dialog opens the alternate screen, holds its outcome
  until a key is pressed, and leaves nothing in scrollback. `--stdin` is not a
  dialog and never opens one. The paths that say one thing and ask nothing stay
  inline: a screen that opens to say *nothing* and then demands a keypress is
  hostile on the most frequent path there is.
- **Bare `rote` asks for what is due and nothing else, and offers practice on one
  keystroke when nothing is.** Deciding what to add beyond what is due is the
  schedule deciding again; asking costs one key and no return.
- **One entry loop, one field, one place a secret becomes glyphs.**
  `tui::read_secret` is the only loop that accepts a typed secret, `Card::entry`
  the only thing that draws one, and `tui::render_field` the only function that
  turns a buffer into something drawable. It masks, sanitises and windows, and
  hands the card a finished string — so `card.rs` never sees a buffer and
  `Secret::expose` gains no caller on the render path. **Sanitising is not
  optional**: every non-cold prompt takes a paste, so the buffer may hold a
  newline, a tab, an escape, a combining mark or a glyph two columns wide. A
  newline written in raw mode puts the rest of the card wherever the cursor
  happened to be, and a wide glyph overruns a box that counts characters while
  `Piece::width` stays silent. A character is drawn only if it is non-control and
  one column wide; anything else is one placeholder column, which keeps the count
  equal to the width.
- **The field is one row, and a partial view always says so.** The window follows
  the caret and the offset is chosen so the caret lands in the text area rather
  than on the border. Where the view is cut off the border glyph becomes a
  chevron, so an unmarked field is a complete view and nothing else can be
  mistaken for one. The offset is state and lives beside `revealed` in the loop,
  because a window recomputed from the caret alone jumps whenever the caret
  crosses a boundary. A one-row field is also why `ASK_SLOTS` and `screen::SLOTS`
  are both still eight.
- **The field is its own delimited area and shares its line with nothing.** A hint
  beside where a secret is typed is a hint that will one day be overlapped by what
  is typed into it — so instructions sit on their own lines above, with one blank
  line before the label-and-field block.
- **Every state of one window is the same rectangle in the same place.** A window
  reserves its lines with `Card::reserve` and each state fills them; short states
  are padded and a state that overruns is cut. What is above the field is the
  heading and does not move, what changes lives in a reserved column or on the line
  under the field, and the field itself never moves. A cut line is the signal that
  the layout or the wording wants shortening — never the box.
- **A refusal is a pulse and a counter, not a paragraph — and the two have
  different lifetimes.** The field border goes red for at most `FLASH` and comes
  back, because nothing red may sit on the screen making every later glance
  re-read it. What the refusal *said* stays under the field until the next
  keystroke, with no ceiling: starting to type is the reader saying they have
  read it or that it no longer matters, and no timer can know that. A standing
  status — the try count — is not a refusal and shows through again once the
  refusal is taken down; it increments in the slot it already occupied. Once the cold tries are spent the
  field stays, saying *out of tries*, with the lookup still on offer: a typed
  answer is flashed and refused rather than judged, escape leaves. The one moment a
  person most needs the vault is after the third miss, and it is also the entry
  that checks the vault against the verifier.
- **A double entry that differs asks again, for as many rounds as the drill allows
  tries**, and a proof that fails does the same. The policy lives in `intake`, pure,
  because the sitting needs it too and draws a different card. A blind field gives
  no other way to find the slip, and a command that exits on it makes the person
  retype everything from the start. At an attachment prompt an empty entry is
  **refused rather than conceded**: there is nothing there to concede to.
- **A bounded retry says which try this is, every refusal spends one, and the
  card that gives up says what it cost.** Three claims, and the loop is only
  bounded in practice when all three hold. A bound nobody can see is
  indistinguishable from no bound, so a person retypes until they give up on the
  command rather than on the pair; one refusal that costs nothing — an empty
  entry was the one — makes the loop unbounded however much the others cost; and
  a last card that repeats the reason is a last card that reads as one more
  round, because the reason is the half already on the screen. `intake::tried`
  is the **one composer** for the count, shared by the drill try, the proof and
  both double entries, so a fourth retry cannot grow its own wording. Where the
  closing line cannot hold reason and cost together it keeps the cost:
  `tui::OUTCOME_ROOM` is the budget and the caller does the dropping, the same
  rule the hint under a field follows.
- **A lapse is the ladder going back to the foot, and nothing else is one.**
  Practice moves no schedule, so a first-sample failure there is a *miss*, and
  `stats` counts it as one. The card, `stats` and `rote guide ladder` say the same
  thing or one of them is wrong. `Outturn::landings` buckets each turn once and the
  buckets are disjoint, so the closing count adds up to what was in front of a
  person. A lone turn is described rather than counted. **A lookup re-labels its
  whole drill**, so the chip says aided — but the cold miss it opened with is still
  what was measured, and the closing line says so. Hiding that would be the tool
  deciding what a week looks like.
- **A glyph carries what a glyph can.** One column of standing per row, and words
  only for what the glyph cannot say. **Green is only ever a clean pass —
  unaided, first try. Yellow is everything that got there another way. Red is a
  failure. Dim is nothing measured.** An attachment lands on `+` in yellow and
  not a green tick: something was added and nothing was judged, and a drill
  recovered with the vault or on the third go is the same claim. `screen::icon`
  and `dialog::Checked` are the only two places that decide it, so no screen
  decides it for itself. The closing tally names its own subject, because it
  buckets each turn by its *first* capture and would otherwise read as a
  contradiction of the row above it. Nothing on a card
  spells out a key that a person already knows — enter submits, escape leaves — so
  the only key named is ctrl-l, and only where it is on offer.
- **`Card::waiting` takes the caret down with the box.** Under masking the caret
  column *is* the length, so a caret left behind would park the terminal's cursor
  on a column that discloses what was just submitted, on the one card that sits
  alone for half a second.
- **The caret is the terminal's own cursor**, moved into the field and shown there.
  It blinks the way every other password field on the machine blinks, follows the
  reader's own cursor settings, costs no animation loop, and cannot be overlapped
  by text the way a glyph on a shared line can. The card draws no caret; it reports
  where one belongs.
- **A command opens a dialog once.** `cmd::dialog::open` is the only caller of
  `Terminal::enter` outside the sitting. Raw mode and the alternate screen are
  process-wide, so a second `Terminal`, even one immediately discarded, restores
  the terminal under the live one and turns echo on beneath the next prompt.
  `Terminal::enter` refuses a second, loudly, because that failure mode is
  otherwise silent and leaks the secret to the screen.
- **Every card is `card::WIDTH` wide.** A box that resizes as content comes and goes
  reads as instability and makes the eye re-find the border. Height is the axis
  that varies, and `Screen::anchor` fixes the top edge so that variation grows
  downward.

  **Prose wraps; composed lines assert.** A sentence goes through `Card::prose`,
  which breaks it at word boundaries under a hanging indent — cutting takes the
  tail, and the tail is where the point of a sentence is. A line laid out in
  columns — a row, a tally, a context line — goes through `card::fit`, which
  drops whole separated facts rather than half of one, and the chip is the
  column that gives because it is the only one made of droppable facts. The clip
  in `Card::edge` is a `debug_assert!` before it is a net, so *every* test that
  renders *any* card catches an over-long line with no screen-by-screen upkeep.
- **Every line is placed with its own `MoveTo`.** Raw mode takes the line discipline
  away, so a newline written while it is held moves down without returning to
  column zero. Nothing writes a newline in raw mode; a test asserts no rendered
  line carries one.
- **Placement, resize and a terminal too small are the adapter's.** The sitting
  never learns that terminal size exists.
- **The terminal is put back three ways**, because each covers a way of leaving the
  others do not: an RAII guard, a panic hook, and a `signal-hook` thread. A
  default-disposition `SIGTERM` runs no destructor, and a terminal left in raw mode
  with echo off shows nothing of what is typed into it next.
- **Replay reads `rung_after`; `stats` computes everything else.** The rung a record
  carries is what the tool decided at the time, which makes history immune to a
  later change in the ladder — and the ladder is configurable now, so that matters
  more. Everything with a threshold in it is computed from the merged corpus
  instead, because a counter maintained across a merge counts one real interval
  twice. `rung_before`, and the two measured intervals, are **witnesses**: replay
  never reads them, and `doctor` compares them against merged order to find a
  divergence.
- **A malformed line is kept, never dropped**, and a record from a newer schema is a
  loud refusal to write rather than a partial read. That doctrine is about lines,
  not files: a file in the chains directory that is not a chain is **excluded**,
  because reading a conflict copy would double every event in the corpus.
- **Never merge a chain; sequence chains.** One machine writes one file. The corpus
  is the merge of all of them ordered by instant, then machine, then position.
- **Every date in a dossier is monotone non-decreasing under replay.** The merge
  orders by instant while the schedule reads civil days, so a machine in another
  zone can hand replay a record whose day precedes its predecessor's.
- **`doctor` never writes, and reports defects only.** `assay`'s registry station
  asserts the first of every binary it collects, on a two-second budget, so nothing
  in that path hashes. A fact that `status` or `stats` already states is not also a
  finding.
- **`banner` never resolves the machine identity**, which costs a subprocess. It
  runs before every shell prompt through `coop`, so the steady state is one
  `stat` and one small read. It rebuilds the cache when the cache has stopped
  answering for what is on disk, by the same read-only path `status` and
  `doctor` take — no flagship gate, no hashing — which is once per change rather
  than once per prompt. **A reminder must not depend on state destroyed by the
  event it exists to announce**: the cache lives in the machine-local tree a
  restore does not bring back, and the state a restore leaves — every lineage
  dormant — is the one the attachment verb exists for. `Cache::witness` is what
  makes absence answerable rather than silent.
- **Every test sets `ROTE_ROOT` and `ROTE_STATE`.** They are seams, along with
  `ROTE_CONFIG`, `ROTE_UI`, `ROTE_HOST`, `ROTE_FLAGSHIP`, `ROTE_MACHINE` and
  `ROTE_NOW`. `ROTE_FLAGSHIP` and `ROTE_MACHINE` are seams rather than files
  precisely so the identity and the marker keep their property of not being
  copyable. `ROTE_NOW` is the clock at the process boundary: the harness and the
  binary each asking the wall clock what the drill day is are two answers for
  the minutes a run straddles the rollover hour. There is deliberately **no seam that
  lowers the KDF cost**: the suite seeds a corpus instead, and mints one verifier at
  the shipped parameters for the whole run.
- Publishing and testing carry no per-relic scripts. Do not reintroduce
  `scripts/publish.sh` or `scripts/test.sh`.

## Testing

**Snapshots for wording, assertions for invariants.** What a card is made of — one
rectangle, a field that never moves, nothing typed ever reaching a rendered line —
is asserted in the crate, because a snapshot cannot express it and `insta accept`
would bless a regression in it. What a card and a topic *say* is in
`tests/wording.rs`, so a rewrite reviews as a diff.

**Properties for the merge.** Per-machine chains make replay a merge of several
ordered streams, which is the shape property testing earns its keep on: the
interesting failures are interleavings nobody would think to write down.
`tests/invariants.rs` holds them, and the one that matters most is that attach
events move no rung and no anchor — the fact-and-event seam, asserted.

**The crate is a library so the suite can use its own types.** A JSON literal in a
test drifts from the wire form the moment either changes, and the constants it
would have to duplicate — the schema, the argon2 cost — are exactly the ones a test
must not restate.

## Verifying the dialog

The card, the placement, the cursor and the refusal of a paste only exist at a
terminal, so `assert_cmd` cannot reach them: without a tty `Terminal::enter` bails
before any of it runs. Drive the published binary under `pty.fork` with `TIOCSWINSZ`
for the size, replay the output stream, and assert on what a person would actually
have seen. Two defects were found that way and by nothing else: a discarded
`Terminal` restoring the screen under the live one, which echoed the new secret
during a rotation, and a resize leaving a broken box until the next keystroke.

**The check that matters most is a grep of the raw stream for the typed secret**,
including its prefixes — that is what caught the echo. It is now two checks
rather than one, because a reveal is a legitimate frame: the token must be
**absent across a whole run in which `ctrl-r` is never pressed**, and **present
exactly once after one `ctrl-r`**. A run that never presses it is the regression
the echo taught; a run that does is the feature. Use tokens that cannot collide
with the card's own words; `new` and `one` both appear in it. The
**attachment card is the second thing worth driving that way**, because it is the
second place a typed secret becomes a verifier.

## Measured, so it is not re-derived

Argon2id on the flagship, 2026-09-10, release build, `m=262144 p=1`:

| `t` | one verification |
|---|---|
| 1 | 112 ms |
| 2 | 230 ms |
| 3 | 350 ms |
| **4** | **468 ms** |
| 5 | 590 ms |

`t = 4` is shipped, against `age -p`'s measured 450 ms at the same 262 MiB. `p` buys
nothing: lanes divide the same memory rather than adding passes over it.

An unoptimised build costs 6.2 s a call, which is why the workspace carries
`[profile.dev.package.argon2] opt-level = 3`. Without it the suite is unusable
rather than merely slow.

The crate's `zeroize` feature wipes the initial hash and the finalisation block. It
does **not** wipe the 256 MiB working array, so `verifier::digest_bytes` supplies
that buffer itself and zeroizes it. Inverting argon2's internal state is as hard as
inverting BLAKE2b, so this is belt rather than braces — the belt costs one `Vec`,
and the two cross-checks against the reference implementation are what keep the
hand-assembled PHC string honest.

Resolving the machine identity costs **17 ms**, one `ioreg` call. It is therefore
resolved lazily: a reading never pays it, and `banner` — which runs before every
shell prompt — never reaches it at all.

## Format choices that are not the lane's habit

- **JSON Lines**, where the lane elsewhere uses frontmatter documents and whole-file
  JSON. An append is one `O_APPEND` line write, so no later write can rewrite an
  earlier record; a whole-file replacement re-serializes the past on every save,
  and a bug in that path rewrites history rather than failing.
- **One file per machine**, where the lane elsewhere has one file. An append-only
  hash chain is not a mergeable structure. The old shape said so as a complaint —
  it warned when a second hostname appeared — and this is that complaint made into
  a design.
- **Local civil dates**, where the lane elsewhere uses UTC instants and day
  differences. A daily ritual is a calendar concept and a UTC boundary falls in the
  small hours here. Each record freezes the day it was credited to, so a later
  change of timezone cannot re-date history.
