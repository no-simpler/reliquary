# Handoff: hoist `install-on-path.sh` into `reliquary/lib/`

**Status:** deferred — but **no longer blocked, and ready to execute.** Registry
consolidation and the `relic` CLI have landed; the hoist was originally split out
to avoid churning the external repos in the same session (halo had a dirty tree
at the time). That precondition is now satisfied: as of the session that added
`relic doctor` / `registry --prune`, both `~/Developer/bb` and `~/Developer/halo`
are clean. The next session can do the move as one coordinated changeset without
waiting on anything — see the steps and reference checklist below.

Note one new caller to update that didn't exist when this handoff was written:
`install-on-path.sh` now also defines `install_on_path_prune_registry` (a sibling
of `install_on_path_migrate_registries`); it moves with the file, no path edits
needed, but the `relic registry --prune` / `migrate` delegation in `relic.sh`
sources the lib via the same `INSTALL_ON_PATH_LIB` constant covered in step 3.

## Why

`~/.config/shell/lib/install-on-path.sh` is fundamentally relic infrastructure.
Its `shell/lib/` home is grandfathered from when it was the only shared file;
it now has siblings under `~/.config/reliquary/lib/` (`relic.sh`). The split is
awkward — one logical subsystem living in two trees.

## Target

Move it to `~/.config/reliquary/lib/install-on-path.sh`. Do it as one
coordinated changeset, ideally when both external repos are clean.

## Steps

1. `git mv` (in yadm terms: move the file) to `reliquary/lib/install-on-path.sh`.
2. Leave a **compat shim** at the old path so nothing breaks mid-migration:
   ```bash
   # ~/.config/shell/lib/install-on-path.sh — shim; real impl hoisted.
   source "$HOME/.config/reliquary/lib/install-on-path.sh"
   ```
   The shim must stay sourced-transparent and must NOT re-add a `META_NAME`
   guard (`META_NAME` is optional — callers set it in the env before sourcing,
   so it flows straight through to the real file).
3. Update the **functional** source lines (3):
   - `reliquary/lib/relic.sh` — the `source "$HOME/.config/shell/lib/install-on-path.sh"` inside `relic::publish`.
   - `~/Developer/bb/biogen/scripts/meta/publish.sh`.
   - `~/Developer/halo/alfred/scripts/meta/publish.sh`.
4. Update **doc/settings** refs (~14): bb `CLAUDE.md` ×3 + 3 `.claude/settings.json`
   Read-permission entries; halo `CLAUDE.md` / `DEPOT_CLAUDE.md` / `scripts/meta/CLAUDE.md`
   / `alfred/CLAUDE.md` (×5) + 3 `.claude/settings.json`. Config-side: this repo's
   `CLAUDE.md`, `GRADUATION.md` "Files this system touches", the helper's own header.
5. Commit each external repo (halo: commit only, no remote).
6. Drop the shim after one bootstrap cycle confirms every caller resolves the
   new path on all author machines.

## Coordination

The "Known external relics" list in `GRADUATION.md` is the migration checklist —
walk it when updating callers. It's best-effort, so also grep both `~/Developer`
trees for the old path before declaring done.
