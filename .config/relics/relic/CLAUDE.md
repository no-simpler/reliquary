# `relic` — the relic-management CLI

The first in-house (Stage-2) relic. It manages the relic lifecycle and
**dogfoods the very pipeline it manages**: it is published onto PATH by the
same `relic::publish` / `install-on-path.sh` rails it exposes.

For the lifecycle, stages, and registry model, see
`~/.config/reliquary/GRADUATION.md`. For deferred work (the `install-on-path.sh`
hoist, `relic graduate`), see `~/.config/reliquary/design/`.

## Anatomy

- `relic.sh` — manifest (`NAME=relic`, `RUNTIME=bash`).
- `src/relic.sh` — the whole CLI, **one self-contained file**. This is load-bearing:
  the published entrypoint is a single `cp`-copied file in `~/.local/bin/`, so the
  CLI cannot rely on sibling files at runtime. It sources its libraries by absolute
  path (`reliquary/lib/relic.sh`, `shell/lib/install-on-path.sh`). Do **not** split
  `src/` into multiple sourced files without also adding a bundling `scripts/publish.sh`.
- `entrypoints/relic` → `../src/relic.sh` — the published name.
- `tests/run.sh` — sources `src/relic.sh` (which self-guards via the
  `BASH_SOURCE == $0` check) and tests the pure helpers against fixtures.

## Commands

`list`, `status`, `publish`, `test`, `update`, `scaffold`,
`registry [--migrate|--prune]`, `migrate`, `doctor`. `<name>` is optional for
status/publish/test/update (cwd auto-detect). Unambiguous command prefixes are
accepted (`relic st`, `relic pub`; note `s` alone is now ambiguous —
status/scaffold — so `relic sc` for scaffold).

- `scaffold <name> [-r <rt>]` — Stage 1 → 2 promotion. If `~/.config/bin/<name>`
  exists it is moved into `src/<name>`, the `entrypoints/<name>` symlink is wired,
  RUNTIME is inferred from its shebang (override with `-r/--runtime`), then the
  relic is published and the result staged in yadm (`yadm add` of the tree + the
  moved Stage-1 path's removal — staged only, the commit stays deliberate). With
  no Stage-1 source it lays down a bare skeleton and prints next steps. Helpers
  (`valid_relic_name`, `valid_runtime`, `infer_runtime`, `manifest_set`,
  `scaffold_tree`) are pure/file-level and unit-tested in `tests/run.sh`.
- `doctor` — read-only cross-check of registry ↔ `~/.local/bin/` ↔ each relic's
  entrypoints. Reports orphan registry entries (no backing file), unpublished
  entrypoints (declared but unregistered), and unmanaged lane files (informational).
  Exits non-zero on the first two; unmanaged-file notes alone don't fail it.
- `registry --prune` — drop registry entries whose `~/.local/bin/<name>` target is
  gone (the fix for `doctor`'s orphans); delegates to `install-on-path.sh`.

- **In-house relics** (Stage 2, `relics/` + `attic/`) support the full set.
- **External relics** (Stage 3, from GRADUATION.md's known list) are read-only here:
  `list`/`status` report them best-effort; publish/test/update defer to their own repos.

## Conventions

- **Attic-safe.** Private relics under `~/.config/attic/` are only surfaced when their
  manifest is *readable*. An encrypted/undecrypted lane reveals nothing — never add code
  that enumerates the lane or prints counts that leak its existence.
- **Bash 3.2 compatible.** No associative arrays / bashisms past 3.2; the macOS floor.
- **Reads, not writes, the registry.** All PATH/registry mutation goes through
  `install-on-path.sh` (single `.reliquary-managed`, fail-fast on name collisions).
