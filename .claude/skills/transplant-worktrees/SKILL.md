---
name: transplant-worktrees
description: "Transplant committed changes from Claude Code worktrees onto the main worktree via rebase + fast-forward merge, then discard worktrees. Use when asked to pull/transplant/absorb worktree changes."
disable-model-invocation: true
allowed-tools: Bash(git:*), Bash(compose-gc:*)
argument-hint: "[worktree-name...]"
---

# Transplant worktrees

Incorporate committed worktree branch changes onto the current branch via rebase + ff-merge, then clean up.

## Current state

```!
git worktree list
```

## Preconditions

Before starting, verify:

1. The main worktree has a clean working tree (`git status` shows nothing).
2. Each worktree to transplant has a clean working tree.

If either is dirty, stop and ask the user what to do.

## Arguments

- If `$ARGUMENTS` names specific worktrees, only transplant those.
- If no arguments, transplant all worktrees except the main one.

With no worktrees present at all, skip straight to **Reconcile orphaned Docker stacks** — that pass is then the whole job.

## Procedure

For each worktree (sequentially — each rebase depends on the updated base branch):

1. **Detach the worktree HEAD** so the branch ref is free:
   `git -C <worktree-path> checkout --detach HEAD`

2. **Rebase the branch** onto the current branch:
   `git rebase <current-branch> <worktree-branch>`
   The rebase will leave HEAD on the rebased branch — switch back immediately:
   `git checkout <current-branch>`

3. **Fast-forward merge**:
   `git merge --ff-only <worktree-branch>`
   If ff-only fails, stop and ask the user — the branches are not cleanly stackable.

4. **Tear down the Docker Compose environment**:
   `compose-gc down <worktree-path>`
   Profile-complete and volume-removing. A service behind a `profiles:` key — such as a `system`-profile `chromium` — is invisible to a plain `down`: it survives the teardown, holds the project network open so that leaks too, and keeps burning CPU. Safe to run unconditionally: it no-ops when there is no compose file, and only project-scoped volumes are removed (not external ones like shared caches).
   It exits non-zero on `Resource is still in use` — that means something outlived the teardown and the reconcile pass still has work to do.

5. **Remove the worktree**:
   `git worktree remove <worktree-path>`

6. **Delete the branch**:
   `git branch -d <worktree-branch>`

After all worktrees are processed, run the reconcile pass below, then `git worktree prune`, then confirm clean state with `git status`.

## Reconcile orphaned Docker stacks

Step 4 only covers the worktrees this run removes. A worktree removed outside this skill — or one whose teardown was skipped or failed — leaves Docker state behind with the compose file gone, permanently out of `docker compose`'s reach: a surviving stack keeps a headless `chromium` (sometimes a whole db stack) alive, starving the machine until system-suite tests fail under the load, and even a fully-exited one pins its volumes for good.

`compose-gc` sweeps those by label instead, scoped to this repository and to both worktree layouts it has used. Its own `CLAUDE.md` carries the full reasoning, including the provenance test that holds unrelated tooling out of range.

The block below runs when the skill loads, so it reports the state you *inherited*. Run `compose-gc` again by hand once the last worktree is gone — that is the pass that catches anything this run leaked, and it is safe (and useful) on its own, purely to garbage-collect. `compose-gc -n` is the same sweep with nothing removed.

```!
compose-gc
```

## Conflict handling

When a rebase produces conflicts:

1. Read the conflicting files and understand both sides.
2. Resolve on a best-effort basis — most worktree changes are reasonably non-overlapping, so conflicts are typically mechanical (e.g. adjacent edits to a mod.rs, overlapping imports).
3. After resolving, `git add` the files and `git rebase --continue`.
4. If a conflict is genuinely ambiguous (semantic overlap, competing designs, unclear intent), abort the rebase (`git rebase --abort`), report what you found, and ask the user for guidance before retrying.
