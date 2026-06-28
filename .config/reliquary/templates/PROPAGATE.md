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
commit conventions and VCS (some projects self-commit stable state; some hand off to the user;
some have a separate brain VCS). Do not impose Reliquary's commit flow on a target. Close with a
**per-target summary**: what was harmonized, what was skipped, what was deferred for the user.

## Scope discipline

- Propagation **distributes** an already-decided template change; it does not redesign the
  template mid-fan-out. If harmonizing reveals the template itself is wrong, stop the fan-out,
  fix the template (a hub edit), and restart.
- One target's harmonization never reads or writes another target's tree.
- The discovery script is the only thing that touches many repos, and it is read-only.
