#!/usr/bin/env bash
#
# relic.sh — the bootstrap seed.
#
# **The bootstrap paradox**: the thing that builds and publishes the first Rust
# binary cannot itself be a Rust binary. That is not a preference a faster
# toolchain could later overturn — it is a property of the path between a bare
# machine and its first executable, and it is why this file survives a
# programme that rewrote everything around it.
#
# So this is no longer a library. It is the shortest thing that produces **one**
# binary — `relic` — and then gets out of the way. Everything the retired
# 667-line version did, `relic` does, with a test suite and a type system behind
# it; the seed's only remaining job is to reach the point where that binary
# exists.
#
# Two properties are load-bearing and easy to lose:
#
#   * **No interpreter but this one.** The retired seed read manifests through a
#     `python3` reader that needs `tomllib`, so a machine whose `python3` was
#     3.9 failed every publish and reported it as a missing manifest field. The
#     seed reads nothing now.
#   * **bash-3.2-safe.** Sourced into the bootstrap's running interpreter, which
#     on a fresh macOS is the stock /bin/bash 3.2 — the modern bash installed
#     minutes earlier does not upgrade the shell already running.
#
# Usage:
#   source "$HOME/.config/reliquary/lib/relic.sh"
#   relic::seed

# Where the public lane is. Overridable so the seed can be exercised against a
# scratch tree rather than only in a scratch account.
RELIC_SEED_LANE="${RELIC_SEED_LANE:-$HOME/.config/relics}"

relic::_seed_say() {
    printf 'relic[seed]: %s\n' "$1"
}

relic::_seed_die() {
    printf 'relic[seed]: %s\n' "$1" >&2
    return 1
}

# relic::seed — build `relic` from source, publish it, and hand off.
#
# Returns non-zero when the first binary could not be produced. A machine that
# reaches the end of bootstrap with nothing published is a machine that will
# say so at `98-bedrock.sh`, which runs after this and treats an absent `assay`
# as the publish path being broken — a bedrock failure reporting itself by its
# own absence.
relic::seed() {
    local dir="$RELIC_SEED_LANE/relic"
    local target
    [ -d "$dir" ] || return 0

    command -v cargo >/dev/null 2>&1 ||
        relic::_seed_die "cargo not on PATH — nothing can be built or published" || return $?

    relic::_seed_say "building the seed"
    (cd "$dir" && cargo build --release --quiet) ||
        relic::_seed_die "cargo build failed for relic" || return $?

    # Asked rather than assumed: `cargo locate-project` is the only thing that
    # knows where the workspace target lands, and a hardcoded depth is what a
    # relocated lane would silently invalidate.
    target="$(cd "$dir" && cargo locate-project --workspace --message-format plain 2>/dev/null)"
    target="${target%/Cargo.toml}"
    [ -x "$target/target/release/relic" ] ||
        relic::_seed_die "built nothing at $target/target/release/relic" || return $?

    # The publish helper, sourced the way its two external callers source it.
    # shellcheck disable=SC1091
    META_NAME=relic . "$HOME/.config/reliquary/lib/install-on-path.sh" ||
        relic::_seed_die "cannot source install-on-path" || return $?
    install_on_path "$target/target/release/relic" relic ||
        relic::_seed_die "could not publish relic onto PATH" || return $?

    # Hand off. From here the binary owns discovery, the manifest schema, the
    # dependency checks and both lanes — none of which the seed knows about.
    relic::_seed_say "seeded; publishing the rest"
    "$HOME/.local/bin/relic" publish --all
}
