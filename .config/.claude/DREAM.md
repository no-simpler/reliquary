# Dream session

Manually invoked. A periodic self-healing pass over Reliquary's *second brain* — the
`CLAUDE.md` network (root plus any nested) and anything else under `.claude/` — plus the
mechanical health checks that have been made scriptable. Keeps the docs compact, well-located,
and consistent with the configs they describe.

This file is the home for **repeatable housekeeping that an agent can do unattended**. It holds
only the *repeatable procedure*. Standing directives live in `CLAUDE.md`; the work backlog lives
in `.claude/TODO.md`. One-off directives raised during a session land in those homes, not here.

## When to run

- On explicit request ("run a dream pass").
- After a stretch of edits to the shell configs or to `CLAUDE.md` — drift accumulates fastest
  right after a burst of change.
- As an occasional heartbeat during quiet stretches.

A dream pass *tightens what is already there*. It does not invent new features or new backlog
work — if a check surfaces something material (a real bug, a config that contradicts its docs),
stop, surface it, and treat it as its own session.

## Source-of-truth ordering

When docs and reality disagree, trust in this order: (1) the actual configs / scripts on disk,
(2) the existing second-brain docs, (3) committed prose (`~/.github/README.md`) only when confirmed
current for the area — otherwise treat it as stale.

## Deterministic pre-pass

Run the mechanical checks before the judgment sweep — cheap, no-judgment, detect-only:

```
yadm doctor
```

Bundles the scriptable health checks: `yadm` resolves to the wrapper in every shell, interactive
startup smoke tests, `$PATH`-dup sanity, alias/abbr/function parity across the paired POSIX
(`*.sh`) / fish (`*.fish`) configs, and encrypted-archive SHA drift. Touch-ID-free; non-zero exit
on any *failure* (warnings, e.g. PATH dups, don't fail). `yadm doctor --full` adds the Touch-ID
archive-vs-disk `verify` — run it deliberately, not in the unattended pre-pass.

When doctor reports parity drift it names the lagging file and the missing names. Adjudicate each:
add the missing definition to the lagging shell (translating syntax — `alias NAME=val` ↔
`alias NAME val` / `abbr --add NAME val`; `name() {…}` ↔ `function name … end`), or, if the
divergence is intentional and permanent, add the name to that pair's allowlist inside
`check-shell-parity` with a one-line reason.

**Extension policy:** as more Reliquary checks become scriptable, fold them into `yadm doctor`
(a new section in the wrapper's `doctor()`), not into prose here. The pre-pass is the reliable
floor; the passes below are the judgment layer it cannot replace.

## The read IS the pass

A dream pass is a **cover-to-cover read**, not a grep. The pre-pass and the passes below say *what*
to notice; they never substitute for *reading*. The defects a dream pass exists to catch — a cooled
section gone verbose, a fact stated two subtly-different ways, a claim that quietly contradicts
another doc far away — have no grep signature.

Read the network the way a stranger reads it for the first time: start at root `CLAUDE.md`, then read
everything it points to, in full, holding the accumulating picture in mind. Two phases, in order:
**read** the whole while dropping feelers (lightweight flags, not edits — a feeler dropped early is
often resolved or confirmed later); then **fix** the feelers via the passes, auto-resolving the clear
ones and surfacing the rest. An "all clear" is legitimate only as the product of a complete read,
never as an inference from a clean `yadm doctor` run.

## The recurring passes

Each runs every session over `CLAUDE.md` (root + any nested), `.claude/TODO.md`, and the canonical
meta docs under `~/.config/reliquary/` (`GRADUATION.md`, `AUX.md`, `design/`). Surface candidate
edits as a list before writing anything large.

### 1. Compaction
Aim for terseness. Keep what *is*, drop what *was* — no change-narration prose ("used to be X").
Cut specifics that rot and are cheaply re-derived (line numbers, exhaustive enumerations, version
strings). Prefer one sharp sentence to a paragraph when both carry the same rule.

### 2. Defragmentation
When related facts are scattered without system, bring them under one roof — usually the relevant
section of `CLAUDE.md`, or the meta doc that owns the topic (relic lifecycle → `GRADUATION.md`).

### 3. Reasonable deduplication
State things once, but tolerate cheap denormalization that aids discovery. A duplicate that has
drifted into *contradiction* is a bug: pick the authoritative location, fix it there, replace the
other with a cross-link or a one-line restatement.

### 4. Path / symbol sanity
`CLAUDE.md` cites many concrete paths and script names. Spot-check that cited paths still resolve
and named scripts/subcommands still exist; prune or correct references to things that moved or were
removed.

## Co-locality principle

Put agentic prose at the most granular level that still encompasses all affected config. The ladder
doubles as access-frequency tiering: every-session facts → root `CLAUDE.md`; occasional,
touch-relevant facts → a nested `CLAUDE.md` or the meta doc that owns the topic; blue-moon /
cheaply-rediscoverable facts → no home, let them be rediscovered.

## Prevention reflex

The dream session is the safety net, not the only defense. When a pass fixes a pattern earlier
sessions produced, also sharpen a standing directive in that pattern's natural home (root or a
subtree `CLAUDE.md`, or the meta doc that owns the topic) so future sessions are oriented away from
re-accumulating it.

## Reliquary-specific guardrails

- **Public repo + encryption obfuscation.** This repo (and its `CLAUDE.md`) is public. Never add
  text that describes the *contents* of encrypted files or de-obfuscates the patterns in
  `~/.config/yadm/encrypt` — the patterns are intentionally opaque. Compaction must not leak what
  they protect. (See `CLAUDE.md` ## Encryption.)
- **TODO house rule.** Completed items in `.claude/TODO.md` are *deleted*, not archived — `yadm log`
  is the record. A pass may prune items the repo has already satisfied (verify against reality
  first); it never adds a DONE section.
- **yadm whitelist.** Any new tracked file a pass produces is invisible to git until explicitly
  `yadm add`-ed. A clean `yadm status` means "nothing *tracked* changed", not "nothing worth
  saving". Name new paths explicitly — `yadm add -A`/`.` will not pick them up.

## Auto-memory drain

Reliquary has a harness-written memory store (under `~/.claude/projects/…/memory/`, indexed by
`MEMORY.md`) — a transient feeder, not a half of the network. Each dream session empties it. Per
memory, in order: substance already in committed docs or `yadm log` → delete; duplicates an
always-loaded file (root `CLAUDE.md`) → delete; cheaply rediscoverable or records an already-fixed
state → delete; a durable directive/fact with a natural home → promote it there (per Co-locality),
then delete; only a layer no other doc holds *and* not cheap to rediscover stays a memory. Keep
`MEMORY.md` to one terse line per survivor. The Prevention reflex applies: when the drain keeps
promoting the same kind of memory, sharpen the directive that should have stopped it.

## Auto-resolve vs surface

Auto-resolve without asking: typos, obviously stale path/symbol references, convention cleanups,
no-churn prose stripping, and parity drift where one side is clearly the lagging copy.

Surface before editing: placement/scope calls, delete-vs-compact judgment, pruning a TODO item
whose completion isn't clearly verifiable, and anything touching encryption semantics or what an
opaque pattern reveals.

## Closing the pass

1. State coverage first: what was read this pass, so an "all clear" is auditable rather than
   asserted. A pass that changed nothing still reports the read.
2. Doc-only edits don't need a verification loop; if a doc cites a symbol or path, confirm it still
   resolves. Re-run `yadm doctor` if the pass touched a shell config.
3. Write a short summary: what was tightened, what was deferred, what would be worth its own session.
4. Commit per `CLAUDE.md` conventions once the tree is stable — informative-but-concise message,
   no co-authorship line, stage every `M`/`R`. `yadm commit` triggers a Touch ID prompt (commits are
   SSH-signed via 1Password); if the user is AFK it will time out — surface it and let them retry.
