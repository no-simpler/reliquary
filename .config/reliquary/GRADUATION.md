# Relic graduation

The personal-CLI lifecycle inside Reliquary. A **relic** is a personal tool
the author keeps; it moves through three stages as it earns more structure.

This is the canonical reference for the lifecycle. `HARDENING.md` owns the
verification standard a relic is held to — the typed-domain rules, the reuse
ladder, the ratchets, and the method for replacing a script with one. For
benefactor-specific deltas, see [[see AUX]] (encrypted; readable in decrypted
environments).

## Stages

| Stage | Where it lives                    | Status                          | Examples              |
|-------|-----------------------------------|---------------------------------|-----------------------|
| 1     | `~/.config/bin/<name>`            | one-shot util, yadm-tracked     | `bbs`, `pb`, `up`     |
| 2     | `~/.config/relics/<name>/`        | in-house relic, yadm-tracked    | `relic`, `ernest`, `docket` |
| 3     | `~/Developer/<name>/`             | external relic, own git repo    | `bb`, `halo`          |

**Stage 1 → 2**: `relic scaffold <name>`, the sanctioned way to start one. What
it lays down splits by runtime — see "The `relic` CLI" — so hand-laying one means
reading "Entrypoints" and "The Rust lane" first.

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

## Runtime stance: Rust by default

A relic is written — or rewritten — in **Rust** unless there is a reason not to,
and the reason is recorded in the manifest:

```bash
RUNTIME="bash"
RUNTIME_EXEMPTION="…why this one is not Rust…"
```

`relic doctor` reports every relic that is not Rust and does not say why. It is
**informational and never a failure**: a relic awaiting its rewrite has to keep
publishing, and the report is the worklist for working through them — it empties
as each is either rewritten into the workspace or given its reason.

`relic scaffold` defaults to `rust`, so the stance costs nothing to follow. An
exemption is asked for explicitly (`-r bash --exempt "<why>"`).

The stance reaches past relics to the whole executable surface, and what stays
shell stays shell on purpose — `HARDENING.md`'s triage rule decides, and its
irreducible-residue classes enumerate what it decides against.

## In-house relic anatomy

```
~/.config/relics/<name>/
├── CLAUDE.md             # agent context for this relic
├── README.md             # optional human docs
├── relic.toml            # manifest
├── Cargo.toml            # rust: a workspace member; see "The Rust lane"
├── entrypoints/          # interpreted only — one file (or symlink) per binary
│   └── <name>            # filename = published name on PATH
├── src/                  # source tree
├── tests/                # test suite (optional)
└── scripts/              # OPTIONAL — only when overriding defaults
    └── {publish,test,update}.sh
```

### Manifest (`relic.toml`)

```toml
[relic]
name = "blab"                # required — published name + registry owner
description = "…"            # optional — one-line summary
runtime = "python"           # required — rust by default, see the stance
runtime-exemption = "…"      # required when runtime is not rust
min-runtime-version = "3.11" # optional — enforced at publish time
entrypoints = []             # optional — compiled relics only; defaults to [ name ]
brew-deps = ["glab"]         # optional — verified at publish time
external-deps = []           # optional — free-form notes
docker = false               # optional — true for docker-run shim entrypoints
```

Keys sit under `[relic]` rather than at the top level: TOML binds everything
after a table header to that table, so a flat manifest would capture the first
future `[section]`'s keys into itself. Unknown keys inside `[relic]` are refused;
unknown *tables* are left alone, which is what the namespace buys.

`~/.config/reliquary/lib/relic.sh` is the only reader — `relic::load_manifest`
for one relic, `relic::_manifest_read` for many at once, both parsing through
`lib/manifest.py`. Every consumer that has to decide whether a directory is a
relic at all asks `relic::has_manifest`: the `relic` CLI, the bootstrap publish
snippet, `up`. One predicate, so no two of them can disagree.

### Entrypoints

Publishing splits on whether the artifact exists before a build.

**Interpreted** (`python`, `bash`, `fish`, `docker`) — convention over manifest.
The published name is the filename in `entrypoints/`; the contents (usually a
symlink into `src/`) are what gets copied onto `$PATH`. Multi-entrypoint relics
just drop more files in.

```bash
entrypoints/blab -> ../src/blab.py     # `which blab` → ~/.local/bin/blab
```

**Compiled** (`rust`) — manifest over convention, and no `entrypoints/` at all.
There is nothing on disk to publish until cargo has run, and the artifact lands
in the workspace `target/` rather than beside the source. So the names come from
`ENTRYPOINTS` (defaulting to `NAME`) and resolve against
`<workspace>/target/release/`. A symlink into an unbuilt `target/` dangles on a
fresh clone, which is what every Rust relic used to override `publish` to work
around.

### Doctrine belongs to the binary

A relic that **agents operate** owns its own doctrine — not just its usage.
Reference (flags, contracts, schemas) goes in `--help`; doctrine (what the thing
is for, when to reach for which shape) goes in a `guide` namespace in the same
binary. Nothing that teaches the tool lives in a file the binary does not ship.

The machine-wide skill at `~/.claude/skills/<name>/` is then a **trigger stub**:
its `description` is the only channel that reaches an agent's context without a
tool call, and its body is two sentences pointing at `<name> guide`. That split
is what `AGENTIC-TOOLING.md` already demands of third-party tools ("one line
stating the tool exists"), and it holds for in-house relics for two further
reasons — a skill is a Claude Code artifact while the binary is harness-agnostic,
and Stage-3 graduation moves the binary to its own repository and would otherwise
strand the prose behind in Reliquary.

Keep both namespaces at the floor of what an agent needs to act correctly; prose
that will not fit belongs to a different owner, not to a longer page. `docket` is
the reference implementation: `docket guide` and `docket help` are disjoint, and
`src/guide.rs` sits beside `src/help.rs` so the two never blur.

## The Rust lane

`~/.config/relics/` is a **cargo workspace**. Every Rust relic is a member; the
lane's other relics are simply absent from `members`, which is a whitelist —
the same discipline yadm applies to `$HOME`.

```
~/.config/relics/
├── Cargo.toml         # [workspace] resolver="3", members, workspace.dependencies, profiles
├── Cargo.lock         # one lock for the lane
├── rustfmt.toml       # one config
├── .gitignore         # /target
├── target/            # one build cache, gitignored
├── crates/            # shared libraries — members, not relics (no manifest)
│   └── relic-core/
├── docket/  ernest/  midden/     # Rust relics: members
└── relic/                        # bash: invisible to cargo
```

A member declares nothing a workspace can hold for it: `edition`, `rust-version`,
`license` and `publish` come from `[workspace.package]`, shared dependencies from
`[workspace.dependencies]` (`anyhow.workspace = true`), and **profiles are
workspace-only** — cargo ignores a member's own `[profile]` block.

### Why a workspace and not a path dependency

Path dependencies need no registry and no publishing, so sharing was never the
problem. Coverage was: a path dependency *outside* a workspace is compiled as a
dependency, so `cargo fmt --all`, `cargo clippy --workspace` and
`cargo test --workspace` never touch it. A member is covered by all three, and
`relic test <relic>` passes `-p` for the relic and for every `crates/*` member so
shared code is gated from each of its dependents rather than by nothing.

The lane also stops triplicating its build: one `Cargo.lock` means the relics
cannot drift onto different versions of a shared crate, and one `target/` means a
dependency they share is compiled once.

### `crates/` — the shared-library boundary

A crate under `crates/` is a workspace member with **no manifest**, which is
exactly what makes it inert: `relic list|status|doctor`, the bootstrap snippet and
`up` all gate on a readable manifest, so none of them sees it. It publishes
nothing and owns no PATH name.

`relic-core` is the first: git as a capability (the constructor that strips the
ambient `GIT_*` environment, so a relic run from a git hook does not answer for
the hook's repository) and one meaning for a path (`project_key`, so two relics
asked about one directory agree).

`fs::write_atomic` is the second admission, on the same evidence: `docket` and
`midden` had the identical tmp-then-rename written out by hand in both stores.
Centralising a duplicate is also the moment to make it right — the shared version
fixed the collision both copies had (`with_extension("tmp")` *replaces* the
extension, so `a.md` and `a.json` shared one temporary), synced the parent
directory after the rename, and stopped leaking a temporary on the error path.

**Admission splits by kind.** *Platform* code — colour resolution, atomic writes,
locking, subprocess capability, PATH resolution — is **reuse-first**: its second
consumer exists by construction, because every relic needs it, and one
implementation is what makes relics behave identically. *Domain* code keeps the
**second-consumer gate**: an item ladder or a prose metric moves in when a second
relic actually needs it and not before, because the wrong abstraction costs more
than the duplication. A crate that collects what might one day be shared is a god
crate, and every relic pays for it. See the reuse ladder in `HARDENING.md`.

The bar is per-language, and it does **not** reach across into the bash relics.
`relic` and `nexus` each carry their own copy of four output helpers and eight
lines of colour setup, and that stays duplicated on purpose: the population is two
files and shrinks to zero as each is rewritten, so a `lib/output.sh` would be
built for a lane that is being emptied. It would also cost `nexus` the property
its header claims — one self-contained copied file in `~/.local/bin` — by handing
it a runtime file dependency for four `printf`s. Revisit only if a bash relic is
ever written that is *not* on the rewrite list.

### Build cache

`target/` is regenerable and grows without bound: cargo never reclaims artifacts
for dependency versions it no longer resolves, or for toolchains that are gone.
Two things hold it down. `[profile.dev] debug = "line-tables-only"` at the
workspace root — full DWARF was the overwhelming majority of the lane's disk, and
line tables are all a CLI relic's backtraces need. And `up`'s **Cargo build
cache** step, which runs `cargo sweep --installed` then `cargo sweep --time 30`
once the tree passes a ceiling (`RELIC_TARGET_CEILING_KB`, 2 GiB by default), and
skips silently below it. `yadm doctor` reports the size so a machine that has not
run `up`, or that lacks `cargo-sweep`, does not balloon unnoticed.

### Attic relics must not join this workspace

A member's name and version land in `Cargo.lock`, which is tracked **publicly** —
adding a private relic would leak its name. And `~/.config/yadm/encrypt` matches
`.config/attic/**`, so a `target/` beneath an attic relic would be tarred into
the encrypted archive and committed.

So when the first attic relic becomes Rust: give `~/.config/attic/` its own
workspace, depend on `relic-core` by path across the lanes, and **add a `target/`
exclusion to `~/.config/yadm/encrypt` before that first build exists**, not after.

## Shared library: `~/.config/reliquary/lib/relic.sh`

Thin defaults so most relics need zero `scripts/` overrides.

```bash
source ~/.config/reliquary/lib/relic.sh
relic::publish ~/.config/relics/<name>     # check_deps, then publish by RUNTIME
relic::test    ~/.config/relics/<name>     # dispatches by RUNTIME
relic::update  ~/.config/relics/<name>     # dispatches by RUNTIME
relic::cover   ~/.config/relics/<name>     # rust only — coverage + the ratchet
relic::mutants ~/.config/relics/<name>     # rust only — mutation testing
```

**Fast loop, slow gate.** `test` is the loop and must stay fast, because agents route
around slow commands: format, then lint, then the suite. `cover` and `mutants` are
separate, deliberate invocations run at wave boundaries. The bash branch of `test`
runs `check-shell-lint` first for the same reason the rust branch runs `fmt` first —
cheapest station reports first, and bash has nothing else that can be verified
statically.

For `rust` the defaults are the whole story, so no Rust relic carries a
`scripts/publish.sh` or `scripts/test.sh`: publish builds then installs from the
workspace `target/`, test runs format → clippy at `-D warnings` → the suite
(fail-fast in ascending cost), and update is publish. Only a relic with a genuine
periodic job of its own keeps a `scripts/update.sh` — `docket` packs its depot,
`midden` prunes its corpus — and it calls `relic::publish` rather than
reimplementing it.

Override any default by dropping an executable `scripts/<op>.sh` into the
relic dir — the lib will exec it instead.

**Tests run against the live machine unless a relic stops them.** A relic that
owns state redirects both its own state root *and* `HOME` at a scratch tree in
every test — the first keeps the suite out of the real corpus, the second keeps
git from reading the machine's global config and signing setup. `docket` is the
worked example, with `DOCKET_ROOT`. A test that forgets is not a failing test; it
is a passing test that mutated your data.

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
relic test    [<name>] [--cover] # wraps relic::test; --cover adds the coverage gate
relic mutants [<name>]           # mutation testing — the assertion-quality gate
relic update  [<name>]           # wraps relic::update
relic scaffold <name> [-r <rt>]  # Stage 1 → 2: promote a bin/ util or fresh idea
relic registry [--migrate|--prune]  # show / fold / prune the shared registry
relic migrate                    # fold legacy per-meta registries
relic doctor                     # cross-check registry ↔ ~/.local/bin ↔ entrypoints
```

`<name>` is optional for status/publish/test/update (cwd auto-detect).
In-house relics get the full set; external relics are read-only here
(`list`/`status` report them best-effort; manage them in their own repos).

`relic doctor` is a read-only health check: it reports orphan registry entries
(registered but no file on PATH), unpublished entrypoints (declared by a relic
but missing from the registry — the `transcribe-asr`-shaped drift), and — both
informational — relics that are not Rust and do not say why, and unmanaged lane
files. `relic registry --prune` is its companion
fix: it drops orphan entries whose `~/.local/bin/<name>` target is gone.

`relic scaffold <name>` automates **Stage 1 → 2**: it moves a `~/.config/bin`
one-shot util into `src/`, wires the entrypoint, fills the manifest (RUNTIME
inferred from the script's shebang, or `-r/--runtime`, defaulting to `rust` when
neither says otherwise), publishes, and stages the result in yadm. A Rust relic is
scaffolded as a workspace member — `Cargo.toml`, `src/main.rs`, and an appended
`members` entry — and a non-Rust `-r` requires `--exempt "<why>"`. With no Stage-1
source it scaffolds a bare skeleton and prints next steps. The `graduate` subcommand stays
deferred.

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
   cross-stage API). `relic.toml` and `entrypoints/` may be kept or shed.
   For a Rust relic this is also its exit from the lane's workspace: drop its
   `members` entry, and give the repo back what the workspace was holding — its
   own `[profile]`, `rustfmt.toml`, `Cargo.lock`, and a decision about every
   `crates/*` crate it depended on (vendor it, or depend on Reliquary's copy by
   absolute path and accept that the tie is no longer unidirectional).
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

- `~/.config/relics/`                                 — in-house relics (incl. `relic` CLI), and the cargo workspace
- `~/.config/relics/crates/`                           — shared crates (members, not relics)
- `~/.config/attic/`                                  — private relics (encrypted)
- `~/.config/reliquary/lib/relic.sh`                  — shared library
- `~/.config/reliquary/template/`                     — relic skeleton
- `~/.config/reliquary/lib/install-on-path.sh`        — stable PATH API + single registry
- `~/.local/bin/.reliquary-managed`                   — the shared PATH registry (not tracked)
- `~/.config/yadm/snippets/shared/12-publish-relics.sh` — bootstrap migrate + re-publish
- `~/.config/bin/up`                                  — periodic update loop
- `~/.config/yadm/encrypt`                            — `.config/attic/**` pattern

`install-on-path.sh` lives in `~/.config/reliquary/lib/` alongside `relic.sh` —
one logical subsystem in one tree. External relics source it by that absolute
path; the dependency stays strictly unidirectional (relic → reliquary).
