# Dream session — template

> Template, not a live procedure. Instantiate it into a project as `DREAM.md` (at whatever
> relative path the project keeps its procedures), then **subtract**: keep every `[CORE]`
> block, keep the `[OPTIONAL — …]` blocks whose "when" matches the project, delete the rest.
> Replace bracketed `<placeholders>` with the project's real paths/commands. The governing
> rules for using and updating this template live in `./CLAUDE.md`.

Manually invoked. A periodic self-healing pass over a project's *second brain* — the
`CLAUDE.md` network (root plus any nested) and everything else under `.claude/` — plus any
mechanical health checks that have been made scriptable. Keeps the docs compact, well-located,
and consistent with the code and configs they describe.

This file holds only the *repeatable procedure*. Standing directives live in `CLAUDE.md`; the
work backlog lives in `<.claude/TODO.md>`. One-off directives raised mid-session land in those
homes, not here.

## When to run   [CORE]

- On explicit request ("run a dream pass").
- After a stretch of edits to the docs or the code they describe — drift accumulates fastest
  right after a burst of change.
- As an occasional heartbeat during quiet stretches.

A dream pass **tightens what is already there**. It does not invent features, findings, or
backlog work. If a check surfaces something material — a real bug, a config that contradicts
its docs, a genuine open question — stop, surface it, and treat it as its own session.

## Source-of-truth ordering   [CORE]

When the docs and reality disagree, trust in this order: (1) the actual code / configs / scripts
on disk, (2) the existing second-brain docs, (3) committed prose (`README`, `docs/`) only when
confirmed current for the area in question — otherwise treat it as stale.

## Deterministic pre-pass   [OPTIONAL — projects with any scriptable health check]

Run the mechanical checks before the judgment sweep — cheap, no-judgment, detect-only:

```
<your linter / doctor command, e.g. just dream-lint  |  bin/doctor>
```

It mechanizes the checks that need no judgment (link/path resolution, format/lint, parity,
drift, frontmatter completeness). It **detects only; it never edits**. Adjudicate each finding
through *Auto-resolve vs surface* below.

**Extension policy.** The pre-pass is the reliable floor; the judgment passes below are the
layer it cannot replace. When a mechanical check keeps recurring across passes, fold it into the
linter rather than into this prose.

## The read IS the pass   [CORE]

A dream pass is a **cover-to-cover read**, not a query. The pre-pass and the directives below
say *what* to notice; they never substitute for *reading*. An agent can grep a dozen
defect-signatures, match none, and pronounce the brain clean — and that verdict is worthless,
because the defects a dream pass exists to catch have no grep signature: a cooled section gone
verbose, a fact stated two subtly-different ways, a claim that quietly contradicts another doc
far away.

Read the network the way a stranger reads a set of documents found for the first time. Start at
the entry point (root `CLAUDE.md`, then any index it points to), then read everything it
catalogs — in full, in a sensible order — holding the accumulating picture in mind. As you
read, drop **feelers**: lightweight flags, not edits, on anything that reads as inconsistent,
duplicated, or verbose.

Two phases, in order:

1. **Read** the whole, accumulating feelers. Do not fix while reading — a feeler dropped early
   is often resolved or confirmed later, and fixing mid-read acts on half the picture.
2. **Fix** the feelers via the passes below, auto-resolving the clear ones and surfacing the
   rest.

An "all clear" is a fine and common outcome — but it is legitimate **only** as the product of a
complete read, never as an inference from a clean linter run. Record what was read in the
closing summary so the verdict is auditable.

**Reading at scale.**   [OPTIONAL — when the network outgrows one comfortable read]
Fan the read across parallel `Explore` subagents (aim for ≤3) — each reads a slice and returns
its feelers; the main loop holds the cross-slice picture and adjudicates. Git and prior-pass
coverage focus *depth* (deep-read what changed, re-skim what did not), never *whether* to read:
do not skip a doc on the assumption it is still fine.

## The recurring passes   [CORE]

Each runs every session. Surface candidate edits as a list before writing anything large.

### 1. Compaction
Aim for terseness.
- Remove documentation of past churn — keep what *is*, drop what *was*. Red flags: "previously",
  "no longer", "used to", "was retired", inline before/after parentheticals. **Version history
  owns the past.** A standalone "we explicitly choose not to do X because Y" rationale is fine;
  history attached to a rewrite is not.
- Cut specifics that rot and are cheaply re-derived: line numbers, exhaustive enumerations,
  counts, catalogs, version strings.
- Prefer one sharp sentence to a multi-paragraph treatment when both carry the same rule.

### 2. Defragmentation
When related facts are scattered without system but are feasibly unifiable, bring them under one
roof. Scattering across **disjoint** file-tree locations is the signal for a path-scoped rule;
scattering within one subtree wants consolidation into that subtree's own `CLAUDE.md`.
**Lean-page bias:** when a section has outgrown its host, create a new compact doc rather than
bloating the host or compressing the section past its signal threshold.

### 3. Reasonable deduplication
State things once, but tolerate cheap denormalization that aids discovery. A duplicate that has
drifted into **contradiction** is a bug: pick the authoritative location, fix it there, replace
the other with a cross-link or a one-line restatement.

### 4. Path / symbol sanity
The docs cite concrete paths and script/command names. Spot-check that cited paths still resolve
and named symbols still exist; prune or correct references to things that moved or were removed.

## Co-locality principle   [CORE]

Put agentic prose at the most granular level that still encompasses all affected code. This
ladder doubles as an access-frequency tiering:

- **Every session** → root `CLAUDE.md` (or a `paths:`-less rule) — loads at session start.
- **Occasional, when touching relevant files** → a nested `CLAUDE.md` (or a `paths:`-scoped rule).
- **Blue moon / cheaply rediscoverable** → no home; let it be rediscovered.

## Prevention reflex   [CORE]

The dream session is the safety net, not the only defense. When a pass fixes a pattern that
earlier sessions produced, also sharpen a standing directive in that pattern's natural home
(root `CLAUDE.md`, a subtree `CLAUDE.md`, or a rules file) so future sessions are oriented away
from re-accumulating it. The aim is orientation, not expecting perfection.

## Auto-resolve vs surface   [CORE]

Auto-resolve without asking: typos, obviously stale path/symbol references, convention cleanups,
no-churn prose stripping, and clearly-lagging duplicates.

Surface before editing (a tight interview — narrow questions, not a wall): placement/scope
calls, delete-vs-compact judgment, structural moves that change doc boundaries, pruning a
backlog item whose completion isn't clearly verifiable, and anything touching security/secrets
or what an obfuscated pattern reveals. In a hands-off session, make the call in the spirit of
the passes and report it in the summary.

## Closing the pass   [CORE]

1. State coverage first: what was read this pass, so an "all clear" is auditable rather than
   asserted. A pass that changed nothing still reports the read.
2. Write a short summary: what was tightened, what was deferred, what would be worth its own
   session.
3. Verification: doc-only edits don't need a full verification loop; if a doc cites a symbol or
   path, confirm it still resolves. Re-run the pre-pass if the pass touched anything it checks.
4. Commit verified-stable state per `CLAUDE.md` ## Commits — informative-but-concise message,
   **no co-authorship line**, no parallel narrative log (the commit history is the audit trail).

---

## Optional modules

Keep a module only if its "when" matches the project; otherwise delete it.

### Auto-memory drain   [OPTIONAL — projects with a harness-written memory store]

The harness memory store is a transient feeder, not a half of the network. Each dream session
empties it: every memory is promoted to its co-locality home or deleted. Apply this decision
tree to each memory, in order:

- Its substance already lives in committed docs, a spec, or version history → delete.
- It duplicates an always-loaded second-brain file (root `CLAUDE.md`, a `paths:`-less rule) → delete.
- It is cheaply rediscoverable, or records an already-fixed state → delete; rediscover if it recurs.
- It is a durable directive or fact with a natural home → promote it there (per Co-locality), then delete.
- Only if it adds a layer no other doc holds *and* is not cheap to rediscover does it stay a memory.

Keep the memory index to one terse line per surviving memory. After the drain the inbox is
usually empty; that steady state is the goal. The Prevention reflex applies: when the drain keeps
promoting the same kind of memory, sharpen the directive that should have stopped it.

### Migrate prose to code   [OPTIONAL — codebases with a natural doc-comment vantage]

Some agentic prose is narrowly scoped to a single class/function and is not assistant-flavored.
For that prose, weigh relocating it to a doc-comment on the target so it lives where it's read.
Cross-file or cross-directory directives almost always stay agentic — code offers no natural
vantage for them. When prose crosses from a gitignored doc into committed code, **neutralize tone
at the crossing** per `CLAUDE.md` ## Committed-file hygiene; anything addressed exclusively to the
agent stays in the second brain.

### Cross-page contradiction sampling   [OPTIONAL — knowledge bases with load-bearing facts]

Pick a small handful of load-bearing facts that circulate through multiple pages and grep each
across the network. Every occurrence should match the canonical source. Mismatch → identify the
canonical source and rewrite the divergent copies in place. Do **not** exhaustively check every
fact — the cost is unbounded; sample the high-traffic ones and rely on canonical-source
discipline for the rest.

### Stale forward-looking claims   [OPTIONAL — docs that carry plans / decisions / predictions]

Walk recently-touched pages. For each forward-looking assertion — a plan, a decision rationale,
a "we will do X if Y" — ask whether it is still current truth. Still true → leave it. No longer
true → rewrite in place (no-churn; do **not** annotate the change). "Depends on a fact I haven't
re-verified" → record the verification as an open item; leave the claim untouched.

### LLM-wiki integrity   [OPTIONAL — interlinked markdown knowledge-base projects]

- **Link targets resolve.** For every `[[slug]]` / `[[slug|label]]`, confirm the target page
  exists. Broken link → create the stub (if the concept deserves a page) or rewrite to drop it.
  Skip wikilinks inside code blocks and literal-syntax illustrations.
- **Orphans.** A page with zero inbound links (other than the index) is suspect: link it from
  the prose that should mention it, or demote it into a related page and delete the file. The
  index/overview entry-point pages are allowed to have no inbound links.
- **Index registration.** Walk every page and confirm it is listed under the right category in
  the index with a one-line summary; drop listings whose files are gone; move mis-categorized ones.
- **Stub section.** Recompute the index's "stubs" list from current wikilink targets vs. files
  on disk. An empty list is fine.

### Cold-storage lifecycle   [OPTIONAL — log-heavy / append-heavy knowledge bases]

Pages that grow thick, time-ordered interiors stay maximally detailed **while live**, then turn
reference-only once resolved. Compaction tiers them: hot detail stays where it is read; cold
detail collapses to conclusion-plus-evidence, or moves to a cold-storage sibling page.
- **Compact what has cooled, never what is live.** Open work is out of scope. This is the
  load-bearing guardrail; when in doubt whether something is still live, leave it.
- **Evidence survives compaction at full fidelity.** Dated entries, verbatim quotes, and id
  anchors are the load-bearing record; what compacts is the *analytical interior*.
- Cold-storing is a compaction-plus-defrag move: re-point inbound anchor links *before* moving,
  and leave a one-line index stub at the vacated location.

### `.claude/rules/*.md` mechanics   [OPTIONAL — projects using the rules feature]

- Rules **with** `paths:` frontmatter (a YAML list of repo-relative globs) auto-load when a
  matching file enters context.
- Rules **without** `paths:` auto-load at session start, alongside `CLAUDE.md`.
- No hook/skill/settings wiring is required for either.
- Hygiene: keep globs tight (don't shadow rules over files they weren't written for); name the
  file by topic, not by directory; write the body as present-tense directives.

### Opening prompt   [OPTIONAL — when you want a paste-as-is invocation]

> Run a dream pass. Read this `DREAM.md` for the operating directives, then **read the second
> brain cover to cover** — every doc the entry point catalogs, in full, as a first-time reader —
> dropping feelers on anything inconsistent, duplicated, or verbose. Read the whole before
> fixing anything. Then make the second-phase fixes against the passes: lean-page bias,
> auto-resolve the obvious, surface ambiguous cases as a tight interview before touching them.
> The linter is the mechanical floor, not a substitute for the read. An "all clear" is
> legitimate only as the product of the complete read. Close with a one-paragraph summary of
> what changed and what you read.
