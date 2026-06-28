# Propagate — centralized template fan-out runbook

The procedure a **Reliquary session** follows to push a template update *out* to the projects it
applies to (use-case 4 in `./CLAUDE.md`). This is the hub → many-spokes direction: one central
session that discovers candidates, lets the user pick, and harmonizes each pick against the
current template. It is the counterpart to the per-project spoke → hub promotion, which happens
in that project's own session, not here.

## When to run

After an improvement has landed in a template (`DREAM.md` or `ROOT-CLAUDE.md`) and is worth
fanning out — i.e. it's a spine change or a newly-useful optional, not a project-local quirk.
Don't propagate churn; propagate substance.

## Procedure

### 1. Discover

Run the bundled discovery script:

```
~/.config/reliquary/templates/bin/discover-template-targets
```

It is read-only and side-effect-free. It enumerates the projects that look like ours and carry a
root `CLAUDE.md` and/or a `DREAM.md`, grouped per repo, flagging byte-identical `DREAM.md` clones
as one logical template (e.g. submodule-aspect fleets). `--help` documents flags; pass search
roots as arguments to narrow or widen the net.

### 2. Present and select

Show the enumeration to the user. For each candidate, the user decides:

- whether to include it at all, and
- which **facet** to harmonize: dreaming (`DREAM.md`), root directives (`CLAUDE.md`), or both.

Use a selection prompt (`AskUserQuestion` or an equivalent pick-list) so the user can take all or
some. Default to nothing selected — propagation is opt-in per target.

### 3. Harmonize each pick — merge, never overwrite

**The project's own context and domain take priority over any generic template suggestion.** The
template offers; the project decides. When a template directive and a deliberate project reality
disagree, the project wins unless the divergence is plainly stale. Harmonization raises the
project to the current spine; it never flattens the project into the template.

For each selected target+facet:

- **Read the target's actual file** and its always-load satellites first — the project's existing
  content is authoritative for everything domain-specific.
- **Diff intent against the current template**, not text: identify which `[CORE]` spine
  directives the target is missing or has an out-of-date version of, and which `[OPTIONAL]`
  modules have become newly relevant to it.
- **Fold the template's updates in while preserving the project's content.** Bring the spine into
  line; offer relevant optionals; keep every domain-specific section, path, and decision the
  project already holds. A harmonize is a merge — the project keeps its identity.
- **Respect the menu-with-a-spine rule in reverse:** don't force optionals the project clearly
  doesn't want, and don't strip a project-local divergence that is deliberate. When the template
  and the project disagree and it isn't obviously stale-vs-current, surface it.

### 4. Surface ambiguity

Harmonization decisions that aren't clear-cut — a spine change that conflicts with a project's
deliberate divergence, an optional whose applicability is borderline, a project-local section
that the template now covers differently — go to the user as a tight interview (narrow questions,
not a wall) before being written.

### 5. Close per each project's own rules

Each target governs its own result: apply edits in the target's worktree, follow **that project's**
commit conventions and VCS. Do not impose Reliquary's commit flow on a target.

**Cleanliness gate — refuse to touch a dirty target.** Only harmonize a target whose relevant
version-control state is clean *at the moment the procedure runs*. A dirty target is skipped and
reported, not stashed or worked around — its in-flight changes are the user's, and bundling them
into a propagation commit is never the house style. "Relevant state" depends on how the brain is
tracked:

- **Brain committed in the project's own repo** (in-tree, or yadm) → the gate is the working
  tree. Dirty tree → skip.
- **Brain tracked separately** (e.g. a `clc`-managed store, locally git-ignored) → the gate is the
  **brain's** sync state against its store, *not* the main code tree. A brain edit promoted via the
  separate VCS never touches main code, so an unrelated dirty main tree does not block it — but a
  brain that already carries un-promoted drift does (promoting would bundle the user's pending
  brain work). Check brain-vs-store sync first; drifted → skip.

**Un-promotable target → defer via an inverse-pull handoff, don't push.** A push only lands when
the target's brain can be *versioned* where it arrives. When it can't — the brain is tracked
nowhere (locally git-ignored *and* un-enrolled in any separate VCS), or the target is one member of
a clone group whose other members aren't individually versioned, so editing one would either strand
an un-versioned floating edit or break the group's byte-identicality — do **not** push the edit.
Instead leave a **handoff** inside the target (e.g. `.claude/handoffs/<topic>.md`) that records the
template payload and has a future session run the harmonization in the **pull** direction: version
the brain first (enroll it), then pull the current template in and merge per this runbook. The
handoff itself rides whatever VCS *is* available at the target — e.g. a `clc`-enrolled superproject
brain can carry and promote a handoff covering its un-enrolled submodules. This inverts the usual
hub→spoke push into a spoke-pulls-from-hub step, which is the only safe direction when the spoke
isn't yet a versioned destination.

**Stay on the checked-out branch.** Apply and commit on whatever branch the target currently has
checked out. Never switch, create, or rebase branches to land a propagation.

**Commit, or its degraded form.** Where the brain rides the project's own VCS, *commit* it there.
Where the brain rides a **separate** VCS, "commit" degrades to **promoting** the change through
that VCS's publish path (for a `clc`-managed brain: `clc save` / the promote flow) — same intent,
different mechanism.

**Verify regardless of tracking.** Run the project's own verification loop on the result however
the brain is tracked — the tracking mechanism changes *how you publish*, never *whether you
verify*. For doc-only DREAM edits that loop is light (cited paths/symbols still resolve; re-run the
project's deterministic pre-pass if one exists), but it still runs.

Close with a **per-target summary**: what was harmonized, what was skipped (and why — dirty gate,
deliberate divergence), what was deferred for the user.

## Evolving this runbook mid-fan-out

This procedure improves *while it runs*. A fan-out is the one time every spoke is read back to
back, so it is the richest source of runbook corrections — when execution surfaces a gap, an
ambiguity, or a sharper way to state a step, **apply the fix to this file (and any Reliquary meta
doc it implicates) in the same session**, then keep going. The meta-evolution is part of the
deliverable, not a distraction from it.

Keep the two kinds of mid-fan-out edit distinct:

- **Procedure/runbook edits** (this `PROPAGATE.md`, `CLAUDE.md`, `GRADUATION.md`, the discovery
  script) → fold in live and continue. They change *how* the fan-out runs, not *what* it
  distributes.
- **Template-content edits** (the `DREAM.md` / `ROOT-CLAUDE.md` payload being distributed) → these
  are a hub change, not a procedure tweak. If harmonizing reveals the template payload itself is
  wrong or incomplete (e.g. a spoke carries a clearly-generalizable improvement the template
  lacks — a promote-back), **stop the fan-out, fix the template at the hub, and restart** so every
  remaining spoke harmonizes against the corrected payload. Do not redesign the payload spoke by
  spoke.

## Scope discipline

- One target's harmonization never reads or writes another target's tree.
- The discovery script is the only thing that touches many repos, and it is read-only.
