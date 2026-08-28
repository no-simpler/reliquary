# In-house relics

Each subdirectory with a manifest is a Stage 2 relic — see
`~/.config/reliquary/GRADUATION.md` for the full system reference,
including anatomy, manifest schema, publish flow, and promotion to
external (Stage 3) status.

The `relic` CLI lives here at `relic/` — the first Stage-2 relic, and the
user-facing surface over the whole system.

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

`crates/` is the shared-library boundary: a member with **no manifest**, which
is what makes it inert to `relic list|status|doctor`, to the bootstrap snippet, and
to `up` — all four gate on a readable manifest. `crates/relic-core` is the first.

**Platform code is reuse-first; domain code keeps the second-consumer gate.**
Colour, atomic writes, locking, subprocess capability, PATH resolution have their
second consumer by construction and move in on sight. An item ladder or a prose
metric waits for a second relic to actually need it. See the reuse ladder in
`~/.config/reliquary/HARDENING.md`.

## Lint policy and ratchets live at the lane root

`[workspace.lints]` in `Cargo.toml` is the **only** lint policy — `relic test`
passes no `-D warnings`, because a command-line group flag outranks every table
entry and collapses `warn` and `deny` into one level. The groups that flag used to
deny are named in the table at `deny`; the typed-domain lints sit at `warn` until
their code is rewritten. `clippy.toml` carries the test carve-outs, without which
the restriction lints fire on every test file and get suppressed module-wide.

`ratchets/` holds the committed baselines a subsystem carries with it if the lane
ever moves. `allows.toml` counts `#[allow]` per package and `relic test` enforces
it across the whole workspace — as an **equality**, so removing a suppression also
means lowering the number. `coverage.toml` joins it once the platform retro-fit has
stopped moving the numbers; until then `relic test --cover` reports and does not
gate. `deny.toml` is the supply-chain check (`cargo deny check`), and `cargo machete`
covers the unused-dependency case it does not. See "Track 3" in
`~/.config/reliquary/HARDENING.md`.

Rust relics carry no `scripts/publish.sh` and no `scripts/test.sh` — the rust
branch of `~/.config/reliquary/lib/relic.sh` is the whole story, and it publishes
manifest-declared names out of the workspace `target/release/` rather than through
an `entrypoints/` symlink that would dangle on a fresh clone. Only a genuine
periodic job earns a `scripts/update.sh`.

**Never add an attic relic to `members`.** A member's name lands in the publicly
tracked `Cargo.lock`, and `.config/attic/**` is an encrypt pattern that would tar
its `target/` into the archive. GRADUATION.md records what to do instead.
