# Relic graduation

The personal-CLI lifecycle inside Reliquary. A **relic** is a personal tool
the author keeps; it moves through three stages as it earns more structure.

This is the canonical reference. For benefactor-specific deltas, see
[[see AUX]] (encrypted; readable in decrypted environments).

## Stages

| Stage | Where it lives                    | Status                          | Examples              |
|-------|-----------------------------------|---------------------------------|-----------------------|
| 1     | `~/.config/bin/<name>`            | one-shot util, yadm-tracked     | `bbs`, `pb`, `up`     |
| 2     | `~/.config/relics/<name>/`        | in-house relic, yadm-tracked    | `relic`               |
| 3     | `~/Developer/<name>/`             | external relic, own git repo    | `bb`, `halo`          |

**Stage 1 → 2**: scaffold from `~/.config/reliquary/template/`, move the
script into `src/`, write a manifest, symlink the entrypoint, publish.

**Stage 2 → 3**: see "Promotion to external relic" below.

Stage 3 relics live outside Reliquary but still depend on its
`install-on-path.sh` API. **The dependency is strictly unidirectional**:
a relic reaches into Reliquary, never the reverse.

Reliquary keeps a convenience list of known external relics below — a
checklist for coordinating shared-wiring upgrades, not an authoritative
inventory. It can also *discover* registrants best-effort via the owner
column of the PATH registry (see "PATH wiring"), but does not chase this
exhaustively: a relic that never registered, or registered ownerless, is
simply invisible, and that's fine.

### Known external relics

- `bb`   — `~/Developer/bb/`   — github.com/decaland/bb-meta
- `halo` — `~/Developer/halo/` — local-only

Append to this list when you promote a relic to Stage 3 — it is a
convenience, kept current best-effort.

## In-house relic anatomy

```
~/.config/relics/<name>/
├── CLAUDE.md             # agent context for this relic
├── README.md             # optional human docs
├── relic.sh              # bash-sourced manifest
├── entrypoints/          # one file (or symlink) per published binary
│   └── <name>            # filename = published name on PATH
├── src/                  # source tree
├── tests/                # test suite (optional)
└── scripts/              # OPTIONAL — only when overriding defaults
    └── {publish,test,update}.sh
```

### Manifest (`relic.sh`)

```bash
NAME="blab"                          # required — published name + registry owner
DESCRIPTION="…"                      # optional — one-line summary
RUNTIME="python"                     # required — python|bash|fish|rust|docker
MIN_RUNTIME_VERSION="3.11"           # optional — enforced at publish time
BREW_DEPS=( "glab" )                 # optional — verified at publish time
EXTERNAL_DEPS=( )                    # optional — free-form notes
DOCKER=0                             # optional — 1 for docker-run shim entrypoints
```

### Entrypoints — convention over manifest

The published name is the filename in `entrypoints/`. The contents (usually
a symlink into `src/`) are what gets copied onto `$PATH`.

```bash
entrypoints/blab -> ../src/blab.py     # `which blab` → ~/.local/bin/blab
```

Multi-entrypoint relics just drop more files into `entrypoints/`.

## Shared library: `~/.config/reliquary/lib/relic.sh`

Thin defaults so most relics need zero `scripts/` overrides.

```bash
source ~/.config/reliquary/lib/relic.sh
relic::publish ~/.config/relics/<name>     # check_deps then install_on_path each entrypoint
relic::test    ~/.config/relics/<name>     # dispatches by RUNTIME
relic::update  ~/.config/relics/<name>     # dispatches by RUNTIME (rust: cargo build + republish)
```

Override the default by dropping an executable `scripts/<op>.sh` into the
relic dir — the lib will exec it instead.

External (Stage 3) relics do **not** depend on this lib. They source
`install-on-path.sh` directly.

## PATH wiring

Entrypoints land in `~/.local/bin/` via `install-on-path.sh`, which records
every managed binary in a single shared registry:

```
~/.local/bin/.reliquary-managed     # <name>[<TAB><owner>], one per line
```

The **owner** column is optional, per-entry provenance — the publishing
meta-repo's `META_NAME`. `META_NAME` is itself optional now; when set it
becomes the owner and is used to detect cross-relic collisions, when unset
the entry is ownerless. (Legacy per-meta `~/.local/bin/.<name>-managed`
files are folded into the single registry automatically — by bootstrap, by
`relic migrate`, and on first publish.)

**Unique names, fail fast.** PATH names must be globally unique. A publish
is refused if the name is already owned by a *different* relic, already
resolves elsewhere on `$PATH`, or a foreign file sits at the target — so a
relic learns at publish time that it needs a different name. Re-publishing a
name you already own is a normal overwrite. Promotion to Stage 3 preserves
the registry entry as-is.

## The `relic` CLI

`relic` is the user-facing surface over this whole system — and the first
Stage-2 relic, self-hosted at `~/.config/relics/relic/`:

```
relic list                       # all relics: stage, runtime, published-state
relic status [<name>]            # one relic's detail (deps, PATH wiring, git dirty)
relic publish [<name>]           # in-house relic → PATH (wraps relic::publish)
relic test    [<name>]           # wraps relic::test
relic update  [<name>]           # wraps relic::update
relic registry [--migrate|--prune]  # show / fold / prune the shared registry
relic migrate                    # fold legacy per-meta registries
relic doctor                     # cross-check registry ↔ ~/.local/bin ↔ entrypoints
```

`<name>` is optional for status/publish/test/update (cwd auto-detect).
In-house relics get the full set; external relics are read-only here
(`list`/`status` report them best-effort; manage them in their own repos).

`relic doctor` is a read-only health check: it reports orphan registry entries
(registered but no file on PATH), unpublished entrypoints (declared by a relic
but missing from the registry — the `transcribe-asr`-shaped drift), and
informational unmanaged lane files. `relic registry --prune` is its companion
fix: it drops orphan entries whose `~/.local/bin/<name>` target is gone.

Deferred subcommands (`scaffold`, `graduate`) and the `install-on-path.sh`
hoist are sketched in `design/` for a later session.

## Private lane: `~/.config/attic/`

Relics whose existence is sensitive live under `~/.config/attic/<name>/`
(same anatomy). The whole subtree is encrypted via the `attic/**` pattern
in `~/.config/yadm/encrypt`. The bootstrap snippet and `up` integration
iterate this lane as well — gracefully no-ops if the lane isn't decrypted.

## Bootstrap

`~/.config/yadm/snippets/shared/12-publish-relics.sh` iterates both lanes
and publishes every relic on every bootstrap. Idempotent; failures are
tolerated.

## `up` integration

`up` iterates relics and runs `relic::update` on each. Opt out with
`UP_SKIP_RELICS=1 up` or `up --no-relics`.

**Contract on `update.sh`**: must be non-interactive and time-bounded.
`up` is a batch tool; an interactive prompt or hanging process would
wedge the whole update run.

## Promotion to external relic (Stage 2 → 3)

1. `git init` inside the relic dir; push to GitHub (or wherever).
2. `yadm rm -r --cached .config/relics/<name>/` to untrack from Reliquary.
3. `mv ~/.config/relics/<name> ~/Developer/<name>` on the author's
   machine. On other machines, clone wherever convenient.
4. **Add an explicit `scripts/publish.sh`** that sources
   `install-on-path.sh` directly. The external relic must not depend on
   `relic.sh` at runtime — only on `install-on-path.sh` (the stable
   cross-stage API). `relic.sh` and `entrypoints/` may be kept or shed.
5. **Verify or add `BREW_DEPS` entries** to the appropriate Brewfile —
   the manifest stays the source of truth, but external relics live
   outside Reliquary's bootstrap loop, so their deps must be declared
   somewhere Reliquary's machine setup will honor.
6. Commit Reliquary's untracking.
7. **Add the relic to the "Known external relics" list above.** This is a
   convenience checklist for coordinating shared-wiring upgrades — kept
   current best-effort, not load-bearing. (The relic also self-identifies
   via the registry's owner column once it publishes, so discovery degrades
   gracefully if the list drifts.)

## Files this system touches

- `~/.config/relics/`                                 — in-house relics (incl. `relic` CLI)
- `~/.config/attic/`                                  — private relics (encrypted)
- `~/.config/reliquary/lib/relic.sh`                  — shared library
- `~/.config/reliquary/template/`                     — relic skeleton
- `~/.config/reliquary/design/`                       — deferred work (hoist, scaffold, graduate)
- `~/.config/shell/lib/install-on-path.sh`            — stable PATH API + single registry
- `~/.local/bin/.reliquary-managed`                   — the shared PATH registry (not tracked)
- `~/.config/yadm/snippets/shared/12-publish-relics.sh` — bootstrap migrate + re-publish
- `~/.config/bin/up`                                  — periodic update loop
- `~/.config/yadm/encrypt`                            — `.config/attic/**` pattern

The `~/.config/shell/lib/` home for `install-on-path.sh` is grandfathered. A
deferred change will hoist it to `~/.config/reliquary/lib/`, coordinated with
external relics' source paths — see `design/`.
