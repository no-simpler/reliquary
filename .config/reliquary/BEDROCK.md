# Bedrock

The **bedrock** is Reliquary's guaranteed substrate: the small set of system-wide dependencies
that are ensured **present, configured, and fully PATH-accessible (including their sub-APIs)** on
every machine the author uses. It is the floor every other repository is allowed to stand on
without re-checking.

## Why

On any machine a human actually uses, one thing is always there: a POSIX/bash-shaped shell. (Truly
barebones boxes have none, but they're out of scope — as is Windows, except via WSL. In scope:
macOS primarily, then WSL/Linux.) Bedrock **expands that always-present floor** from "a shell" to a
named, enforced contract. Everything the author owns — relics, meta-projects, sibling repos — may
assume bedrock exists and is wired correctly. When a tool needs *more* than bedrock, it does not
grow bedrock; it takes the **docker + self-isolation** route instead.

So bedrock stays deliberately minimal. The bar for membership is "so universally assumed that
re-installing or re-checking it in every project would be absurd," not "useful to have."

## Members (v1)

| Member  | Contract | Sub-APIs verified |
|---------|----------|-------------------|
| **bash**    | present; **modern (>=5) on PATH**, ensured early in bootstrap | — |
| **python3** | present; **latest, never minor-pinned**; one system-wide interpreter | runnable `python3 -c`; `python3 -m pip` |
| **uv**      | present; supplements python3 (isolation/tooling) — does **not** own the interpreter | `uvx` |
| **docker**  | CLI present, implementing the **full docker API** (any impl — OrbStack here, engine on Linux) | `docker compose`, `docker buildx` |
| **git**     | present (yadm *is* git; everything assumes it) | — |
| **curl**    | present (the bootstrap entrypoint) | — |
| **just**    | present; **latest, never pinned** — the host-side task entrypoint into every repo | — |
| **cargo**   | present, with a working toolchain; **latest, never pinned** — rustup owns it, the way brew owns python3 | `rustc`, `cargo fmt`, `cargo clippy` |

### bash
macOS freezes `/bin/bash` at 3.2 (GPLv3 licensing); countless scripts target `#!/usr/bin/env bash`
and need 5.x. Bedrock guarantees the **PATH-resolved** bash is >=5, not the stock one — `/bin/bash`
3.2 stays where it is (unfixable, and harmless once a modern bash leads on PATH). Modern bash is
installed *early* in bootstrap (`yadm/snippets/macos/02-bash.sh`, before the bulk Brewfile) so it's
a real guarantee, not an afterthought.

### python3 — the ownership model
- **Homebrew owns the system `python3`** (`brew "python"`). It is the always-present interpreter and
  **self-heals across minor versions**: `brew upgrade` (run by `up`) advances it to the latest 3.x
  transparently. Nothing pins a minor version system-wide; `#!/usr/bin/env python3` always resolves
  to whatever latest is on PATH.
- **uv is bedrock too, but supplementary.** It is the isolation/tooling layer — `uv run`,
  `uv tool`, `uvx` — for anything needing more than the bare interpreter. It does **not** own the
  base `python3`; there are no PATH shims redirecting `python3` at uv. (The "uv owns the interpreter"
  model was considered and deliberately rejected for v1: it needs shim wiring and self-healing
  plumbing for no real gain while brew already tracks latest.)
- **Per-app version floors, never pins.** An app that needs a minimum Python declares a *floor*
  where it already lives — relic manifests' `MIN_RUNTIME_VERSION`, enforced at publish time by
  `relic::check_deps` (`reliquary/lib/relic.sh`). A floor that breaks when system Python advances is
  a bug in the app, not a reason to pin the system.
- **Anything needing very specific Python wiring dockerizes** and self-isolates. That's the escape
  hatch, not a bedrock special case.

### docker
"Full docker API" means the CLI plus the `compose` and `buildx` plugins — not just a bare `docker`
binary. The implementation is unconstrained: OrbStack on this machine, the engine directly on Linux,
colima elsewhere — bedrock probes `docker` generically, never assumes the vendor. Daemon *liveness*
is runtime state (OrbStack auto-starts on demand), not part of the install contract, so the checker
does not probe it by default (doing so could auto-launch OrbStack or hang an unattended run).

### just
A `justfile` is the author's cross-repo convention for a project's task surface: the one place both
humans and agents look to learn what a repo can do (`just --list`) and to actually do it. That makes
`just` an **invocation-layer** dependency — without it on the host, a repo's entire task surface is
unreachable, and the miss surfaces as "command not found" in whatever tries to drive the repo rather
than as a legible setup error.

It is specifically the **host-side counterpart to docker**. The "when a tool needs more than bedrock,
it dockerizes" rule means the interesting work happens inside containers — but *something* on the
host has to drive them, and in practice every such repo's justfile is exactly that shim (recipes that
wrap `docker compose exec`). Bedrock guaranteeing docker but not `just` would guarantee the engine
and not the ignition.

Contract is presence only: no sub-API, and **no version floor** — brew tracks latest and self-heals
on `brew upgrade`, the same ownership model as python3. A repo needing newer recipe syntax than the
host has is a floor to declare in that repo, not a reason to pin the system.

### cargo
The relic lane is a cargo workspace and relics are Rust by default, so `relic publish` *builds from
source* — there are no prebuilt binaries. Without a toolchain, every relic on `$PATH` is
unbuildable, and on a fresh machine simply absent. That is the same argument that admitted `just`:
guaranteeing the engine and not the ignition is not a guarantee.

**rustup owns the toolchain**, exactly as Homebrew owns `python3`, and it self-heals the same way —
`up` runs `rustup update`, so nothing is ever pinned. This makes cargo the **first bedrock member
not installed from the Brewfile**: brew's `rustup` formula is keg-only, so routing through it would
*add* PATH plumbing rather than remove any. Bootstrap installs the official rustup instead
(`yadm/snippets/shared/11-rustup.sh`, `--no-modify-path` because `shell/env.d/040-env.*` owns
cargo's PATH). The install *mechanism* is therefore per-member; the *guarantee* is uniform.

The sub-APIs are not decoration. `rustc`, because cargo without a compiler builds nothing; and
`cargo fmt` / `cargo clippy`, which are rustup **components** rather than parts of cargo — `relic
test` runs both, so a toolchain missing either makes the entire verification gate unenforceable.
They are failures, not warnings, on the same reasoning as docker's `compose`/`buildx`. `rustup`'s
own absence is only a warning: another toolchain still builds, it just loses the self-healing.

## Where each concern lives

| Concern | Owner |
|---------|-------|
| **Install** (macOS) | base `brew/Brewfile` — members tagged with a trailing `# bedrock` marker. `git`, `bash`, `curl`, `python`, `uv`, `just` + the `orbstack` cask. Applied by `yadm/snippets/macos/02-brewfile.sh`; bash is front-run by `macos/02-bash.sh`. **`cargo` is the exception**: rustup owns it, installed by `yadm/snippets/shared/11-rustup.sh`, and carries no `# bedrock` tag. |
| **Install** (Linux/WSL) | **not yet implemented** — see the TODO queue. Verification already runs and fails loud there. |
| **Verify** | `assay bedrock` — cross-platform, side-effect-free, offline. Presence + version/sub-API probes + a shadow/duplicate scan. Exit `0` satisfied / `1` warnings / `2` incomplete. |
| **Enforce** | `yadm doctor` runs `assay` (so the dream pre-pass and `yadm update --quiet` both cover it); `yadm/snippets/shared/98-bedrock.sh` re-asserts it loudly at the end of bootstrap. |
| **Contract** (for other repos) | this doc + the "Bedrock" section in `~/.config/CLAUDE.md`. |

### Minimize shadows and duplicates
The goal is **one system-wide install per member** with clean PATH wiring. macOS ships copies that
cannot be expunged (`/bin/bash`, `/usr/bin/python3`, `/usr/bin/git`, `/usr/bin/curl`); bedrock
deliberately *shadows* them by putting Homebrew ahead on PATH (asserted last by
`shell/env.d/999-path.sh`). The station treats those known OS-baseline copies as expected, but
**warns** about any *other* extra copy on PATH, or when Homebrew provides a member that isn't the
one winning — surfacing real drift without pretending the unexpungeable copies are problems.

## Bootstrap is bash-3.2-safe

Bootstrap snippets are *sourced* into the running interpreter, which on a fresh macOS is `/bin/bash`
3.2. Installing modern bash mid-run does **not** upgrade that already-running shell. Therefore **every
bootstrap snippet must stay bash-3.2-safe** (POSIX; no associative arrays, no `${x,,}`, etc.). The
early-bash install benefits the post-bootstrap system, interactive shells, and the doctor check — not
the in-flight bootstrap run. (Re-exec'ing bootstrap under the fresh bash 5 was rejected as fragile;
the 3.2-safe authoring rule is simpler and robust.)

## Evolving bedrock

To add a member:
1. Add it to `MEMBERS` in `relics/assay/src/stations/bedrock.rs`, with a sub-API probe if it has one
   and an `expected_extra` entry if the OS ships a shadowable copy.
2. Give it an install path: the base `brew/Brewfile` with a `# bedrock` marker for anything brew
   ships, or a bootstrap snippet where an upstream installer is the better owner (as with
   rustup). Either way, note which lane in the member table. Linux install remains a TODO.
3. Document it in the member table above.

Keep the bar high: a new member must be *universally assumed*, not merely convenient. If it's only
needed by some projects, it belongs in those projects (or behind docker), not in bedrock. If the
bedrock surface ever grows enough to warrant it, promote it to a first-class `bedrock` CLI/relic
parallel to `relic` (see the TODO queue).
