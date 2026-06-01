# In-house relics

Each subdirectory is a Stage 2 relic — see
`~/.config/reliquary/GRADUATION.md` for the full system reference,
including anatomy, manifest schema, publish flow, and promotion to
external (Stage 3) status.

The `relic` CLI lives here at `relic/` — the first Stage-2 relic, and the
user-facing surface over the whole system. Deferred next steps (the
`install-on-path.sh` hoist, `relic scaffold`/`graduate`) are in
`~/.config/reliquary/design/`.

Private relics live under `~/.config/attic/` (encrypted; same anatomy).
