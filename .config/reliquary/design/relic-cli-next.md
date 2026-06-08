# Handoff: `relic` CLI — next subcommands

**Status:** `relic` v1 has landed at `~/.config/relics/relic/` with
`list / status / publish / test / update / registry / migrate`, and `scaffold`
landed after it (see below). One lifecycle-automation command remains deferred —
`graduate` — until a real relic or two teaches the ergonomics (don't design it
from zero data). It lives in the same single self-contained `src/relic.sh` —
keep that constraint (the published entrypoint is one copied file; no runtime
sibling sourcing without a bundling `scripts/publish.sh`).

## ~~`relic scaffold <name>` — Stage 1 → 2~~ **DONE.**

Promotes a one-shot `~/.config/bin/<name>` (or a fresh idea) into an in-house
relic. Implemented in `src/relic.sh` (`cmd_scaffold` + the pure helpers
`valid_relic_name`, `valid_runtime`, `infer_runtime`, `manifest_set`,
`scaffold_tree`), covered by `tests/run.sh`.

- Copies `~/.config/reliquary/template`; fills `NAME`/`RUNTIME` via `manifest_set`
  and drops a relic-specific `CLAUDE.md` stub.
- Promotion: moves the Stage-1 script into `src/<name>`, wires the
  `entrypoints/<name> -> ../src/<name>` symlink, infers RUNTIME from the shebang
  (override with `-r/--runtime`; prompts on a TTY when neither is available).
- Publishes (`relic::publish`) and confirms (`relic status`).
- **Resolved open question** (per user directive — yadm is run agentically, never
  handed to the user): scaffold **stages its own result** — `yadm add` of the new
  tree plus the moved Stage-1 path's removal (staged only; the commit stays a
  deliberate step). The earlier "stage nothing, print the yadm line" lean was
  overridden.

## `relic graduate <name>` — Stage 2 → 3

The 7-step promotion in `GRADUATION.md` "Promotion to external relic". This one
is judgement-heavy and high-blast-radius — automate last, and make it **ask, not
assume**:

- `git init` + first commit; **ask** which remote (GitHub? none/Nexus like halo?).
- `yadm rm -r --cached .config/relics/<name>/`; `mv` to `~/Developer/<name>`.
- Generate `scripts/publish.sh` that sources `install-on-path.sh` directly (the
  external relic must not depend on `relic.sh` at runtime).
- **Ask** which Brewfile scope should absorb the manifest's `BREW_DEPS`.
- Append to the "Known external relics" list (best-effort channel).
- Each step should be confirmable / reversible; dry-run mode would be welcome.

## Smaller follow-ups

- ~~`relic registry --prune`~~ **DONE.** Drops registry entries whose target no
  longer exists in `~/.local/bin/`. Logic lives in `install-on-path.sh`
  (`install_on_path_prune_registry`, the canonical registry writer); the CLI
  delegates, mirroring `relic migrate`.
- ~~`relic doctor`~~ **DONE.** Read-only cross-check of registry ↔ `~/.local/bin/`
  ↔ each relic's entrypoints. Reports orphan registry entries, unpublished
  entrypoints (the `transcribe-asr`-shaped gap), and informational unmanaged lane
  files; exits non-zero on the first two. Pure helpers (`doctor_orphans`,
  `doctor_unpublished`, `doctor_unmanaged`) in `src/relic.sh`, covered by
  `tests/run.sh`.

Remaining deferred work in this file: `relic graduate`.
