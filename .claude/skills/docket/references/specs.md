# Specs — design-and-implementation flow

A spec fuses system design and implementation plan for one substantial initiative.
Reach for a spec instead of a handoff when the work is an initiative that outlives any single pickup — many sessions, a plan worth iterating on before it is worked.

Its primary reader is the next session working that initiative — a transitive self-handoff.
Maintainer review is secondary but real, so a spec owes clear structure and correct spelling as a plain expectation.

An open spec means the initiative has work left; there is no parked "done" state, and graduation and expunge are the only exit.

## The spec directory

A spec is stored as a directory, `specs/<id>-<slug>/spec.md` under the project's docket, so anything the initiative accumulates — sketches, captured output, scratch data — sits beside the spec file and is retired with it.
Design-and-plan prose gravitates to `spec.md` itself.

Edit the spec in place at every step: there is no save step, and no history to preserve.
A spec never enters a diff or a commit, so it never gates one either.

## Frontmatter

The CLI owns the frontmatter block; you own the body.

- **`stage`** (required) — `design` or `implementation`.
  Advance it with `docket promote`.
- **`blocked`** (optional) — free-form agent-authored text, written and cleared with `docket set`.
  A boolean with a payload: non-empty means blocked and describes the block; absent or empty means not blocked.

The two are strictly orthogonal; a spec can be blocked in either stage or neither.

## Session entry — "let's keep working on `<id>`"

1. Read the spec, ingest `stage` and `blocked`.
2. **If `blocked` is present, address it before any stage work.**
   - External-circumstance block (upstream fix, better timing) — check whether the circumstance changed.
     If not, say so and stop.
   - Maintainer-involvement block (review, decision, approval) — the maintainer just opened the session.
     Presume them at the desk and pick it up.
   - Still decisively intractable → say so and stop.
   - Once cleared, drop the block and continue by stage.
3. **Dispatch by `stage`** to the matching playbook below.

Open the turn with a status banner: stage, one-line block summary, next action.
Finding a blocked spec still stuck is a valid outcome.

## What a spec carries

Concerns to cover, not a mandated heading layout.

- **System design** — locked decisions and their rationale.
- **Testing** — threaded throughout, never bolted on at the end.
- **Implementation plan** — the checkbox list and its waves, roughly one wave per sub-agent.
- **Graduation plan** — which slices of knowledge land in the project's committed docs, and where.

The checkbox plan is the progress marker: tick boxes as waves land and keep them honest, because their state is the first thing a cold session reads to learn where the work stands.

## Stage promotion

One promotion exists, `design` to `implementation`, performed with `docket promote`.
It is **maintainer-gated**.
Ask once the design context is ingested and confidence is genuine — opportunistically, not on a schedule.
Print the spec's state and why it looks ready, then pose an interview-style confirmation question.

- **Hands-off fallback.**
  Never promote unilaterally.
  Write the readiness summary into `blocked` as "awaiting promotion confirmation", keep `stage: design`, stop.
- The ladder runs forward only; there is no demotion.
  Evolving a spec in place under design-by-implementation is not demotion.
- There is **no** gated promotion at the end of implementation; graduation and expunge close it out.

## Stage `design` — brainstorming in place

The spec file is the canvas, not just the record.
Evolve it on two axes, editing straight into the file:

- **Extrapolate** where the design deserves wider coverage.
- **Dig in** where deferring to implementation-time is risky — a fork that, guessed wrong, forces a rewrite.

### Grill a reactive maintainer

Do not wait for direction on a fork you can frame.
Surface it grounded in how the codebase actually works, and lead with a recommendation.
Reserve the question for forks the maintainer alone owns.

### Watch the extrapolation in both directions

An answer that cuts a feature is as load-bearing as one that adds it.
A cut fork becomes a negative decision or a named extension point, never a silent deletion.
When an answer ripples past the question asked, chase the neighbors it unsettles in the same session.

### Resolve forks in place

A fork lives in the open-questions section until settled.
On resolution it is promoted into a locked decision, carrying its rationale and the alternative it beat.
Record the decision and its reason, not the back-and-forth.

## Stage `implementation` — driving the orchestration

A heavy spec is implemented by one long-running driver session that delegates coding to sub-agents.

### The orchestrator's stance

Stay high-level; investigate only enough to seed a sub-agent and verify its work.
Map the terrain with read-only explorers first, then drive implementers against that map.
Never read a sub-agent's raw transcript output file — it overflows the driver's context.
Rely on the completion notification plus a targeted review of the diff.

### Decision forks — when to pause

Keep the bar high.
Decide anything groundable in the code, the spec, or a sensible default, and note the call.
Pause only for business semantics, a near-equivalent source-of-truth choice, production behavior, or session scope.

### Waves

Break the work into waves, each a coherent slice one sub-agent can land.
Size a wave so it compiles and passes the project's whole gate on its own.
A partial slice cannot, under a strict gate.
Order by dependency and run sequentially in the main worktree.
Deliberately not parallel worktrees: every wave must pass the full gate, and the tree stays green.
Add a shared abstraction in the wave where its second consumer appears.

### Verify, then commit, between waves

Have each sub-agent iterate on targeted verification while it works.
**Then have it run the project's whole gate as its final check, never a subset it chose itself.**
That gate is exactly what the driver's commit runs, so a check the seed forgot to name is the one that fails the commit.
Naming checks per wave makes every seed a fresh chance to omit the one that matters, and the omission surfaces as the driver's failed commit rather than the sub-agent's.
The few minutes it costs are nothing against a wave.
A check that fails for an environmental reason is retried once the environment is right; it is never weakened to make it pass.

Keep commits small and per-wave, each a stable green state.

**A wave may land in more than one commit.**
The binding rule is that a *commit* passes the whole gate, never that a wave is one commit.
Where a wave's own prose worries about its size, take that at its word and cut it at a seam where each half stands alone.
Say so in the spec, so the extra commit reads as a decision rather than an accident.

### Seeding a sub-agent

Seed richly: exact confirmed signatures from prior waves, and a committed precedent to mirror.
State the load-bearing constraints, conventions, deliverables, test obligations, and verification commands.
Tell it not to touch git — the driver owns commits.
On an upstream-library blocker, have it record a brief and a local fallback rather than thrash.
Carry that into the spec, and set `blocked` if it gates the next session.

### The spec is the living handoff

Record each decision with its rationale and tick the checklist as waves complete.
Design-by-implementation is the expected flow; scope creep is accepted on a long spec.
Write the spec for meaning; do not spend driver context grinding mechanical prose polish.

## Stopping and blocking

A stop request means halt at the next committed green milestone, not an emergency abort.

Before halting:

- Land or cleanly abandon the in-flight wave, leaving no half-applied edits.
- Make the spec current: tick what landed, record the decisions taken.
- Set the frontmatter — correct `stage`, and `blocked` populated with whatever the next session must resolve.

Stopping with work unfinished and a `blocked` note is fully valid.

The driver may self-stop on context pressure, at a high bar and as a last resort.
Evolving a spec across many sub-agent sessions balloons the driver's context; a fresh driver then resumes from the spec.

## Graduation

Graduation and expunge are the mandatory close-out of every spec.

Implementation already produces the top tiers — self-documenting code, then doc comments.
Graduation handles only the residue those cannot carry.

- Distill that residue into the project's committed docs near the end of implementation, following the project's own documentation conventions.
- Create a new doc file only if genuine residual knowledge exists.
- Graduated prose is committed, so hold it to the project's standard for committed prose.
- Graduate only what a future reader needs; in-flight scaffolding is not it.

## Expunge

Once code, doc comments, and residual docs subsume the spec, retire it with `docket close`, which takes the spec directory and everything co-located in it.
Carry nothing forward.

## Command shapes

`docket help`, `docket help <topic>`, `docket <command> --help`.
