# In-house relics

Each subdirectory with a `relic.sh` is a Stage 2 relic — see
`~/.config/reliquary/GRADUATION.md` for the full system reference,
including anatomy, manifest schema, publish flow, and promotion to
external (Stage 3) status.

The `relic` CLI lives here at `relic/` — the first Stage-2 relic, and the
user-facing surface over the whole system. The deferred next step
(`relic graduate`) is in `~/.config/reliquary/design/`.

Private relics live under `~/.config/attic/` (encrypted; same anatomy).

## This lane is a cargo workspace

Relics are **Rust by default**; anything else records a `RUNTIME_EXEMPTION` in its
manifest, and `relic doctor` lists the ones that have not. So the lane root
carries the cargo furniture for all of them: `Cargo.toml` (one `members` list, one
set of `[workspace.dependencies]`, the only `[profile]` blocks that count),
`Cargo.lock`, `rustfmt.toml`, and one gitignored `target/`.

`members` is an explicit whitelist, so a non-Rust relic here is simply invisible
to cargo. A relic **does not** carry its own `[profile]` (cargo ignores it),
`rustfmt.toml`, or `Cargo.lock`.

`crates/` is the shared-library boundary: a member with **no `relic.sh`**, which
is what makes it inert to `relic list|status|doctor`, to the bootstrap snippet, and
to `up` — all four gate on a readable manifest. `crates/relic-core` is the first.
Code moves there when a **second** relic needs it, and not before.

Rust relics carry no `scripts/publish.sh` and no `scripts/test.sh` — the rust
branch of `~/.config/reliquary/lib/relic.sh` is the whole story, and it publishes
manifest-declared names out of the workspace `target/release/` rather than through
an `entrypoints/` symlink that would dangle on a fresh clone. Only a genuine
periodic job earns a `scripts/update.sh`.

**Never add an attic relic to `members`.** A member's name lands in the publicly
tracked `Cargo.lock`, and `.config/attic/**` is an encrypt pattern that would tar
its `target/` into the archive. GRADUATION.md records what to do instead.
