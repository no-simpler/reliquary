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

## Where each concern lives

| Concern | Owner |
|---------|-------|
| **Install** (macOS) | base `brew/Brewfile` — members tagged with a trailing `# bedrock` marker. `git`, `bash`, `curl`, `python`, `uv` + the `orbstack` cask. Applied by `yadm/snippets/macos/02-brewfile.sh`; bash is front-run by `macos/02-bash.sh`. |
| **Install** (Linux/WSL) | **not yet implemented** — see the TODO queue. Verification already runs and fails loud there. |
| **Verify** | `bin/check-bedrock` — cross-platform, side-effect-free, offline. Presence + version/sub-API probes + a shadow/duplicate scan. Exit `0` satisfied / `1` warnings / `2` incomplete. |
| **Enforce** | `yadm doctor` runs `check-bedrock` (so the dream pre-pass and `yadm update --quiet` both cover it); `yadm/snippets/shared/98-bedrock.sh` re-asserts it loudly at the end of bootstrap. |
| **Contract** (for other repos) | this doc + the "Bedrock" section in `~/.config/CLAUDE.md`. |

### Minimize shadows and duplicates
The goal is **one system-wide install per member** with clean PATH wiring. macOS ships copies that
cannot be expunged (`/bin/bash`, `/usr/bin/python3`, `/usr/bin/git`, `/usr/bin/curl`); bedrock
deliberately *shadows* them by putting Homebrew ahead on PATH (asserted last by
`shell/env.d/999-path.sh`). `check-bedrock` treats those known OS-baseline copies as expected, but
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
1. Add it to `MEMBERS` in `bin/check-bedrock`, with a `check_<name>` sub-API probe if it has one and
   an `expected_extra` entry if the OS ships a shadowable copy.
2. Add it to the base `brew/Brewfile` with a `# bedrock` marker (and, eventually, the Linux install
   path).
3. Document it in the member table above.

Keep the bar high: a new member must be *universally assumed*, not merely convenient. If it's only
needed by some projects, it belongs in those projects (or behind docker), not in bedrock. If the
bedrock surface ever grows enough to warrant it, promote it to a first-class `bedrock` CLI/relic
parallel to `relic` (see the TODO queue).
