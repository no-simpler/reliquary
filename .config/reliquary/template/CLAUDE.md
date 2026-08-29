# Relic template

This directory is the skeleton a new in-house relic is scaffolded from. Prefer
`relic scaffold <name>` over copying it by hand — it fills the manifest, lays down
the Rust half, and appends the workspace member.

For the full graduation reference, see `~/.config/reliquary/GRADUATION.md`.

## Rust unless exempted

`relic.toml` ships `runtime = "rust"`, because that is the default and not a
guess. Changing it means filling in `runtime-exemption` with why this one is not
Rust — `relic doctor` lists every relic that changed one without the other.

## What to fill in

1. **`relic.toml`** — set `name`, `description`, and `min-runtime-version`.
2. **`src/`** — put your source here.
3. **`tests/`** — optional; add tests in your runtime's idiom.
4. **`CLAUDE.md`** — replace this file with project-specific agent context.

### Rust (the default)

Add a `Cargo.toml` inheriting from `[workspace.package]` and add the relic to
`members` in `~/.config/relics/Cargo.toml`. Declare no `[profile]` (cargo ignores a
member's) and no `rustfmt.toml` or `Cargo.lock` (the workspace holds both). There
is **no `entrypoints/`** — published names come from `entrypoints`, defaulting
to `name`, and resolve against the workspace `target/release/`. No `scripts/` either:
the lib's rust branch builds, tests and publishes.

### Interpreted (needs an exemption)

Add **`entrypoints/<name>`** — a symlink to the executable in `src/` you want
published. The filename is the published name. Add `scripts/{publish,test,update}.sh`
only to override what `relic publish|test|update` does by default.

## Publish

```bash
relic publish <name>
```
