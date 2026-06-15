# Reliquary TODO

Single live queue for Reliquary work.

> **House rule.** Completed items are **deleted** from this file, not archived —
> `yadm log` is the record of what was done. No DONE section. Add new work as a
> new section; keep each item independently sized so it can be picked up alone.

---

## Bedrock — Linux/WSL install path

Bedrock verification (`bin/check-bedrock`) is cross-platform and already runs (and fails loud) on
Linux, but **installation** is macOS/Brewfile-only. Add a Linux install path: distro packages for
bash/python3/git/curl, the curl installer (or distro pkg) for uv, docker engine for docker. Wire it
into the `linux/` bootstrap snippets. Until then, a fresh Linux box reports bedrock incomplete by
design. See `reliquary/BEDROCK.md`.

## Bedrock — Brewfile ↔ check-bedrock parity guard (optional)

Consider a small guard (parallel to `check-shell-parity`) asserting that every `# bedrock`-tagged
line in `brew/Brewfile` has a matching member in `check-bedrock`'s `MEMBERS`, and vice-versa, so the
install list and the contract can't silently diverge.

## Bedrock — revisit "uv owns python3" (only if needed)

v1 has Homebrew own the system `python3` with uv supplementary (no shims). The rejected alternative —
uv owns the interpreter via `uv python install` + a `python3` shim — is only worth revisiting if the
brew-owned model causes real friction. Don't do it speculatively.

## Bedrock — promote to a `bedrock` CLI/relic (only if surface grows)

If bedrock's surface outgrows a single checker, promote it to a first-class `bedrock` CLI/relic
(`bedrock check|doctor|list`) parallel to `relic`. Not warranted at v1.

---
_Reference (not a todo): confirmed-correct config exclusions are documented in
`~/.config/CLAUDE.md` → "Deliberately not tracked (audited)"._
