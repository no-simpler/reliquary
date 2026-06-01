# Handoff: `relic` CLI — next subcommands

**Status:** `relic` v1 has landed at `~/.config/relics/relic/` with
`list / status / publish / test / update / registry / migrate`. Two
lifecycle-automation commands were deferred until a real relic or two teaches
the ergonomics (don't design these from zero data). Both live in the same
single self-contained `src/relic.sh` — keep that constraint (the published
entrypoint is one copied file; no runtime sibling sourcing without a bundling
`scripts/publish.sh`).

## `relic scaffold <name>` — Stage 1 → 2

Turn a one-shot `~/.config/bin/<name>` (or a fresh idea) into an in-house relic.

- `cp -r ~/.config/reliquary/template ~/.config/relics/<name>`.
- If promoting an existing Stage-1 script: move it into `src/`, leave the
  `entrypoints/<name>` symlink pointing at it.
- Fill the manifest (`NAME`, `RUNTIME`, deps). Prompt for `RUNTIME` if not
  inferable from the script's shebang.
- Publish and confirm (`relic publish <name>` then `relic status <name>`).
- Open question: should it `yadm rm` the old Stage-1 file and `yadm add` the new
  tree, or leave staging to the user? Lean: stage nothing, just print the
  `yadm add` line — matches the repo's manual-commit house style.

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

- `relic registry --prune`: drop registry entries whose target no longer exists
  in `~/.local/bin/` (the inverse of the live-vs-registry drift, e.g. the
  historical `transcribe-asr` gap — present on PATH, absent from the registry
  until republish).
- `relic doctor`: cross-check registry ↔ `~/.local/bin/` ↔ each relic's
  entrypoints, reporting orphans and unpublished entrypoints.
