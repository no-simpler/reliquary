# Design stub: the `relic` CLI

Deferred. Sketched now so paths and symbols already exist by the name they
will end up under, avoiding rename churn later.

## Goal

A single command that subsumes the per-relic publish/test/update flow and
the cross-stage scaffolding/promotion steps. Today there are two layers
under Reliquary's umbrella:

- `~/.config/reliquary/lib/relic.sh`        — shared shell library
- `~/.config/shell/lib/install-on-path.sh`  — stable PATH API (grandfathered location)

The CLI consolidates both into one user-facing surface.

## Commands (sketch)

```
relic list                       # all relics, with stage and status
relic status <name>              # is it published? are deps satisfied? is it dirty?
relic publish <name>             # equivalent to today's relic::publish
relic test    <name>             # equivalent to today's relic::test
relic update  <name>             # equivalent to today's relic::update
relic scaffold <util-name>       # one-shot util → in-house relic (Stage 1 → 2)
relic graduate <name>            # in-house relic → external relic (Stage 2 → 3)
```

Auto-detect target from cwd when `<name>` is omitted (matches `bb`'s
`resolve_targets` idiom — see `~/Developer/bb/cli/main.sh`).

## Self-hosting

The CLI lives as a Stage 2 relic at `~/.config/relics/relic/`. The
graduation lifecycle eats its own dog food: the tool that manages relics
is itself a relic.

## Hoist: install-on-path.sh → ~/.config/reliquary/lib/

`~/.config/shell/lib/install-on-path.sh` is fundamentally relic
infrastructure. The `~/.config/shell/lib/` location made sense when
install-on-path was the only shared file; now that it has siblings under
`~/.config/reliquary/lib/`, the split is awkward.

Plan:

1. Move `install-on-path.sh` to `~/.config/reliquary/lib/install-on-path.sh`.
2. Update every known caller:
   - `~/.config/reliquary/lib/relic.sh` — internal source line
   - `~/.config/yadm/snippets/shared/12-publish-relics.sh` — if it references the old path (it doesn't directly, but a transitive grep should confirm)
   - **All known external relics** (see GRADUATION.md "Known external relics" list at the time): update their publish scripts to source the new path. Currently `bb` and `halo`.
3. Leave a thin compatibility shim at the old path during the transition
   window that just sources the new location (`source ~/.config/reliquary/lib/install-on-path.sh`). Drop the shim after the next bootstrap cycle on all author machines.

The compatibility-shim window is the safety net for any external relic we
forgot to migrate. The "Known external relics" list is the working
checklist.

## Open questions

- **Private-lane visibility.** `relic list` should not reveal the
  existence of relics under `~/.config/attic/` in environments where the
  lane is encrypted (e.g., running the CLI from a `--no-decrypt` session
  or before someone has decrypted yet). Probably: silently skip if the
  lane is empty *or* if attempting to read fails; never enumerate.

- **Registry consolidation.** Today each relic has its own
  `~/.local/bin/.<name>-managed` registry. A single
  `~/.local/bin/.reliquary-managed` would be tidier but breaks
  symmetry with bb/halo's per-meta-repo model. Decide when the CLI
  lands; possibly support both during transition.

- **`relic graduate` automation.** End-to-end automation of the 7 steps
  in GRADUATION.md's "Promotion to external relic" — including the GitHub
  remote setup, the `yadm rm -r --cached`, the `mv` to `~/Developer/`,
  the publish.sh override generation, the Brewfile reflection, the
  GRADUATION.md awareness-list update. Some steps need human judgement
  (which remote? which Brewfile scope?); the CLI should ask, not assume.

- **Implementation language.** Bash matches house style (bb is bash;
  `up`, `pb`, `bbs` are bash). Python adds dep and runtime cost for
  little win. Default: bash.

## Out of scope for this stub

- Manifest format changes (e.g., switch from `relic.sh` to TOML) — only
  revisit if the bash manifest becomes painful.
- Auto-Brewfile generation from `BREW_DEPS` — explicitly rejected as
  complexity creep in GRADUATION.md.
- Docker-specific tooling — runtime concern, not CLI concern.

## Triggering this work

Design when there are 1-2 actually-graduated relics in `~/.config/relics/`
to learn from. Don't design speculatively from zero data.
