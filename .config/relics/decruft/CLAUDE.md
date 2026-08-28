# `decruft` — remove what a tool left behind

Deletes inert OS metadata and interpreter caches. Published onto PATH; `up` runs
it as a step.

## Two lanes, because "may this be deleted?" has two best answers

- **Inside a git repository**, the repository answers. Only *ignored, untracked*
  paths are candidates, so a per-repository unignore is respected without this
  program knowing the rule, and a tracked file is never a candidate at all.
- **Outside one** — the XDG data dir — there is nobody to ask, so the answer is
  by name, and the set of names is deliberately small.

`src/ignored.rs` asks the repository, `src/walk.rs` finds them, `src/cruft.rs`
holds the names, `src/plan.rs` decides everything before anything is removed —
so `--dry-run` and a real run are one computation rather than two that agree.

## What is deliberately kept

Editor swap, backup and lock files are gitignored, which keeps them out of
commits, but a live one is crash-recovery state and this may run while an editor
is open. Ignoring is not deleting. Dependency and build trees are inert too, but
expensive to rebuild — cost keeps them, not safety. Vendored interpreter trees
(uv's managed pythons) are skipped whole: their caches belong to the tool that
installed them.

A directory left empty by a removal is reported, never deleted: git does not
track an empty directory, so removing one could silently take a placeholder.

## Deviations from `bin/decruft`

The method is in `~/.config/reliquary/HARDENING.md`; this is the list.

| deviation | why | pinned by |
| --- | --- | --- |
| Oracle is `git ls-files --others --ignored --exclude-standard --directory -z`, not `git clean -Xdn` | The old one parsed the English literal `"Would remove "`. Under a translated git it matched nothing, removed nothing, and reported success | `ignored::tests::a_trailing_separator_is_not_part_of_the_name` |
| Cruft **inside a wholly-ignored directory** is found | `--directory` collapses such a directory and stops, and the collapsed name is not cruft. Collapsed means everything under it is ignored, which is exactly the condition that makes the by-name rule safe there. Two real caches on this machine were invisible to the old tool | `ignored_cruft_inside_a_repository_goes` |
| The result is **collapsed**: nothing sits beneath anything else | git's two forms disagree about how far to descend. Collapsing makes the count independent of that rather than dependent on a version of it | `plan::tests::a_directory_swallows_what_is_under_it` |
| Interpreter caches are no longer pruned | They were in both the prune list and the cruft list, and pruning wins — so the XDG lane's clause for them was unreachable. A pruned directory is never yielded, so it can never be removed | `cruft::tests::nothing_is_both_pruned_and_cruft` |
| A repository that cannot be read is **reported** | The old script sent git's stderr to `/dev/null`, so a dead worktree looked like a clean sweep. One is reported on this machine right now | `Plan::unanswered` |
| Symlinks are unlinked, never followed | A link named like cruft is one file; what it points at is not this program's to judge | `a_symlink_is_unlinked_not_followed` |
| `--root` relocates the data lane too | Pointing the sweep at one tree and the data lane at another would sweep something the caller did not name — and made the lane untestable | `the_data_directory_is_swept_by_name` |
