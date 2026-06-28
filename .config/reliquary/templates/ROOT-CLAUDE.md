# Root CLAUDE.md — template

> Template, not a live brain. The filename is `ROOT-CLAUDE.md` (not `CLAUDE.md`) so it is never
> auto-loaded as a real root brain — the same indirection trick depot/meta repos use. Instantiate
> it as a project's root `CLAUDE.md`, then **subtract and fill in**: keep every `[CORE]` block,
> keep the `[OPTIONAL — …]` blocks whose "when" matches, delete the rest, and replace every
> `<placeholder>` with the project's real content. The governing rules for using and updating
> this template live in `./CLAUDE.md`.
>
> A root brain has two natures. The cross-cutting **directive blocks** below are pre-written and
> reusable almost verbatim. The **skeleton sections** (Map, Architecture, …) are project-specific
> scaffolding — keep the headers, write the bodies yourself.

---

# <Project name>

<One or two lines: what this repo is, who/what it serves, and its prevailing working style.>

## Map   [CORE — fill in]

> Each `CLAUDE.md` documents only its own directory level; nested dirs get their own file. The
> root carries a map pointing at each notable file/dir and each nested `CLAUDE.md` with a
> one-line purpose. Add a nested `CLAUDE.md` only when conventions diverge from the parent or it
> carries value beyond the file listing — never preemptively.

- `<dir/>` — <purpose>
- `<dir/CLAUDE.md>` — <what that subtree's brain owns>

## Architecture   [fill in]

<The project-specific shape: components, layers, data flow, deploy kinds, external contracts.
Stay at altitude — name the owning file/symbol and stop; don't enumerate what the reader can
look up.>

## Conventions

### Second brain   [CORE]

The agent-facing knowledge layer is the `CLAUDE.md` network (root + any nesting) plus the entire
`.claude/` directory. It is <gitignored / personally version-controlled — state which>, so its
vocabulary is unconstrained. It documents structure, conventions, and approach — not specifics a
reader can get by reading the code or running the tool.

### Documentation hygiene   [CORE]

- **No-churn.** Docs describe the present; version history owns the past. Never annotate a
  rewrite with "previously / no longer / used to / revised on <date>". A standalone "we
  explicitly choose not to do X because Y; revisit if Z" rationale is allowed — it is
  forward-looking, not history.
- **Cross-references stop at the file.** Cite "see `path/FILE.md`" plus at most a one-line
  topical hook; never paraphrase or enumerate another file's contents — that copy silently rots.
  Citing a *section* is mirroring too; cite the file as a whole.
- **Leave look-up-able specifics to the reader.** Counts, catalogs, signatures, line numbers,
  current assignments rot — name the owning file/symbol and stop.
- **No version pins in prose.** Name the tool; the manifest/lockfile owns its version.

> No-churn exceptions (the canonical set — keep those that apply): negative-decision records
> ("chose not to X because Y"); history-as-subject pages where chronology *is* the content
> (timelines, changelogs, audit ledgers); in-flight checklists meant to evolve.

### Co-locality   [CORE]

Put prose at the most granular level that still encompasses all affected code: a subtree-specific
directive in that subtree's `CLAUDE.md`; a glob-spanning one in a `paths:`-scoped rule; a
repo-wide one in root `CLAUDE.md`. This doubles as access-frequency tiering (every session →
root; occasional → nested; blue-moon → no home).

### Markdown style   [CORE]

Reference-guide prose: terse, structured, easy to look up; no preamble or filler. One sentence
per source line; blank line between paragraphs; no hard width-wrapping. Bullet lists for sibling
enumerations. **Never use Markdown tables.** DRY — one fact, one place.

### Name the pattern   [OPTIONAL — domains with recurring concepts worth a shared vocabulary]

Recurring concepts get short, consistent, load-bearing names used uniformly across docs, code,
and conversation. Define a pattern once and mark its instances (a greppable comment) rather than
re-listing them. Prefer established vocabulary; coin a tight term only when none fits.

## Committed-file hygiene   [OPTIONAL — projects that hide agentic usage on their public edge]

Any file that may be read by others — every version-tracked file, commit messages, PR/MR
descriptions, tags — must not reveal assistant usage. Forbid assistant-adjacent vocabulary
(`agent`, `prompt`, `LLM`, `AI`, vendor/tool names, `co-authored-by`, …); substitute neutral,
human-facing phrasing. The second brain is explicitly *outside* this public edge and
unconstrained. **Tone-neutralization at the crossing:** when prose moves from a gitignored doc
into committed code/docs, neutralize it at the crossing point. Pointers run one way — brain files
may cite committed docs; committed files never cite the brain.

## Commits   [CORE]

- Short imperative subject; body explains **why** when non-obvious. The commit history is the
  audit trail — no parallel narrative/session log.
- One commit per logical change; commit clean, stable checkpoints proactively (uncommitted work
  is unbacked and invisible to the next session).
- **No co-authorship / attribution trailers** on any git object or public content.
- Push deliberately — <state the rule: push only when asked / no remote / on merge>.
- **Commit authority** (declare one): *proactive* — commit a sensible stable change set without
  waiting for confirmation, letting the pre-commit gate verify it; or *hand-off-only* — never
  commit directly, always hand the change to the user.

> If commits are signed via an interactive prompt (Touch ID / GPG / 1Password), a prompt timeout
> means the user is AFK — surface it plainly and let them retry; don't loop or work around signing.

## Verification   [OPTIONAL — projects with a test/lint/format gate]

One canonical verify command that humans, agents, and CI all run — an agentic pass must mean a
human pass. Run it before claiming a task complete.

```
<your verify command, e.g. just verify  |  composer qa>
```

- Running one station fast for the part you touched is a **pre-check**, not a substitute for the
  full gate. Never reach for flags that force-green by dropping scope.
- No silent gaps: a change to the loop isn't done until local, agentic, and CI all agree — mirror
  any station/wiring change across them in the same change set.
- Don't fight text/style gates — reword to fit rather than carving per-file exceptions; uniform
  mechanical hygiene is the gate's value.
- **Coverage as habit:** touch code, leave matching test coverage. <State gate strength: hard
  percentage gate / habit-not-a-gate.>

---

## Optional satellite modules

These describe always-load satellites a root brain commonly wires in. Keep a module only if the
project adopts that system; each typically graduates into its own `.claude/rules/*.md` or
referenced doc rather than living inline.

### Specs system   [OPTIONAL — multi-session design+implementation initiatives]

Substantial multi-session work gets a spec: a single Markdown file under `.claude/specs/` fusing
design and implementation plan, written primarily for the next session (a transitive
self-handoff). One spec = one file. Lifecycle: inception → design → implementation plan →
checkbox detalization → per-session batching → finalization → completion → graduation (distill
residual knowledge into committed docs) → expunge. Edit in place at every stage; version control
owns the history. Specs are durable-while-alive — distinct from disposable session scratch.

### Handoffs + SessionStart hooks   [OPTIONAL — multi-project / cross-session coordination]

Temporary, non-regenerable data that doesn't fit a durable doc: a sender writes
`<source>--<slug>.md` into a handoff inbox; the recipient reads it, incorporates it into its own
VCS, then deletes it (recipient owns cleanup). Recipient may equal sender — a delayed prompt to
your own future session. A `SessionStart` hook surfaces pending handoffs at boot and is silent
when none exist. **Principle:** `CLAUDE.md` is for invariants; hooks are for conditional context
— anything relevant only in some sessions is better emitted by a script the hook calls than
written as static prose.

### Worktree anchoring   [OPTIONAL — projects that work in linked git worktrees]

In a linked worktree, anchor every file operation to the worktree path. Tools and subagents may
report the bare repo root; writing there lands edits on the wrong branch. A `SessionStart` hook
that detects a non-main worktree and prints "work here, not in the main checkout" pairs well.

### Meta / aspect / depot topology   [OPTIONAL — submodule monorepos]

An orchestrator "meta" repo tracks sibling projects as submodule "aspects", each worked on from
its own root. **Stop-guard:** if an agent arrived at the meta by traversing up from an aspect,
it is in the wrong context and must stop — work happens at the aspect root. Cross-aspect data
flows only through a shared depot (a read-only, regenerable artifact exchange under
`$XDG_DATA_HOME`), never direct sibling reads; aspect `.claude/settings.json` denies reads of
sibling trees to enforce it.

### LLM-wiki shape   [OPTIONAL — agent-maintained knowledge-base projects]

A persistent, interlinked markdown graph the agent maintains while the human curates sources and
asks questions. Three layers: transient raw sources in a gitignored `inputs/` (distilled then
deleted) → the durable interlinked wiki (the only layer that survives, the source of truth) →
the schema (`CLAUDE.md`) defining structure. An `index.md` catalogs every page by category with
a one-line summary, updated on every add/remove. Flat categories with globally-unique kebab-case
slugs; Obsidian-style `[[slug]]` / `[[slug|label]]` links (no path, no `.md`), never wrapped in
backticks. Session ritual: read `CLAUDE.md` → `index.md` → relevant pages before acting.

### `.claude/rules/*.md` mechanics   [OPTIONAL — projects using the rules feature]

Rules **with** `paths:` frontmatter auto-load when a matching file enters context; rules
**without** `paths:` auto-load at session start alongside `CLAUDE.md`. No hook/skill/settings
wiring needed. Keep globs tight, name the file by topic, write the body as present-tense directives.

### Platform baseline & PATH publishing   [OPTIONAL — tools that assume a provisioned machine]

Assume a minimal, named, provisioned dependency substrate present and wired on every machine, so
tools need not re-check it; dockerize rather than grow the baseline. Publish
executables onto `$PATH` through one canonical owned helper with an ownership registry that
refuses to overwrite unmanaged files — never touch the PATH dir directly. Cold-start
self-sufficient: first run self-heals from a lockfile, then is offline-capable.
