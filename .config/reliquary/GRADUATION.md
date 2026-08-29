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
| 1     | `~/.config/bin/<name>`            | one-shot util, yadm-tracked     | `bbs`, `up`, `mm`     |
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

**The `relic` binary is the only reader**, through `relic::manifest`, and it is
the only thing that decides whether a directory is a relic at all. One reader
and one predicate, so no two consumers can disagree about what a relic is.

That used to be a shell library shelling out to a `python3` reader for
`tomllib`, which is how a machine whose `python3` was 3.9 failed every publish
and reported it as a missing manifest field. Nothing outside the binary parses
a manifest now — not the bootstrap seed, which reads none, and not `up`, which
asks the binary.

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

## The bootstrap seed: `~/.config/reliquary/lib/relic.sh`

**The bootstrap paradox**: the thing that builds and publishes the first Rust
binary cannot itself be a Rust binary. That is a property of the path between a
bare machine and its first executable, not a preference a faster toolchain could
overturn — which is why this file survived a programme that rewrote everything
around it, and why it is the only shell left in this system.

It is no longer a library. It is the shortest thing that produces **one** binary
and gets out of the way:

```bash
source ~/.config/reliquary/lib/relic.sh
relic::seed    # cargo build relic → install_on_path → `relic publish --all`
```

Two properties are load-bearing. It reads **no manifest and starts no
interpreter but the one running it**, so nothing on the path to the first binary
can be taken down by a stale `python3`. And it is **bash-3.2-safe**, because the
bootstrap sources it into the stock macOS shell, which the modern bash installed
minutes earlier does not upgrade.

Everything the retired 667-line library did — dependency checks, the publish
split, both gates, both ratchets, the cross-lane gate — is in the binary now,
with a test suite behind it. See `relic --help`.

## Files this system touches

- `~/.config/relics/`                                 — in-house relics (incl. `relic` CLI), and the cargo workspace
- `~/.config/relics/crates/`                           — shared crates (members, not relics)
- `~/.config/attic/`                                  — private relics (encrypted)
- `~/.config/reliquary/lib/relic.sh`                  — the bootstrap seed (the only shell left)
- `~/.config/reliquary/template/`                     — relic skeleton
- `~/.config/reliquary/lib/install-on-path.sh`        — stable PATH API + single registry
- `~/.local/bin/.reliquary-managed`                   — the shared PATH registry (not tracked)
- `~/.config/yadm/snippets/shared/12-publish-relics.sh` — bootstrap migrate + seed + hand-off
- `~/.config/bin/up`                                  — periodic update loop
- `~/.config/yadm/encrypt`                            — `.config/attic/**` pattern

`install-on-path.sh` lives in `~/.config/reliquary/lib/` alongside `relic.sh` —
one logical subsystem in one tree. External relics source it by that absolute
path; the dependency stays strictly unidirectional (relic → reliquary).
