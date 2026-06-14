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

## Deterministic pre-pass

Run the mechanical checks before the judgment sweep — cheap, no-judgment, detect-only:

```
~/.config/bin/check-shell-parity
```

Verifies the paired POSIX (`*.sh`) / fish (`*.fish`) configs define the same alias / abbr /
function *names*. A non-zero exit names the lagging file and the missing names. Adjudicate each:
add the missing definition to the lagging shell (translating syntax — `alias NAME=val` ↔
`alias NAME val` / `abbr --add NAME val`; `name() {…}` ↔ `function name … end`), or, if the
divergence is intentional and permanent, add the name to that pair's allowlist inside the script
with a one-line reason.

**Extension policy:** as more Reliquary checks become scriptable (e.g. the planned `yadm doctor`
self-check — startup smoke tests, `$PATH`-dup sanity, encrypted-file drift), fold them in here as
additional pre-pass commands rather than describing them as judgment prose. The pre-pass is the
reliable floor; the passes below are the judgment layer it cannot replace.

## The recurring passes

Each runs every session over `CLAUDE.md` (root + any nested), `.claude/TODO.md`, and the canonical
meta docs under `~/.config/reliquary/` (`GRADUATION.md`, `AUX.md`, `design/`). Source-of-truth
ordering when docs and reality disagree: (1) the actual configs/scripts on disk, (2) the existing
second-brain docs, (3) `~/.github/README.md` only when confirmed current. Surface candidate edits
as a list before writing anything large.

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

## Auto-resolve vs surface

Auto-resolve without asking: typos, obviously stale path/symbol references, convention cleanups,
no-churn prose stripping, and parity drift where one side is clearly the lagging copy.

Surface before editing: placement/scope calls, delete-vs-compact judgment, pruning a TODO item
whose completion isn't clearly verifiable, and anything touching encryption semantics or what an
opaque pattern reveals.

## Closing the pass

1. Doc-only edits don't need a verification loop; if a doc cites a symbol or path, confirm it still
   resolves. Re-run `check-shell-parity` if the pass touched a shell config.
2. Write a short summary: what was tightened, what was deferred, what would be worth its own session.
3. Commit per `CLAUDE.md` conventions once the tree is stable — informative-but-concise message,
   no co-authorship line, stage every `M`/`R`. `yadm commit` triggers a Touch ID prompt (commits are
   SSH-signed via 1Password); if the user is AFK it will time out — surface it and let them retry.
