# Handoff: hoist `install-on-path.sh` into `reliquary/lib/`

**Status:** deferred. Registry consolidation and the `relic` CLI have landed;
the hoist was intentionally split out to avoid churning the external repos in
the same session (halo had no remote and a dirty tree at the time).

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
