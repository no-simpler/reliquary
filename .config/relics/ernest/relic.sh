# Manifest for the `ernest` CLI — the first Rust (Stage-2) relic.
# Sourced by ~/.config/reliquary/lib/relic.sh.
# See ~/.config/reliquary/GRADUATION.md for the full schema.

NAME="ernest"                        # required — published name + owner column
DESCRIPTION="Measure prose density: the share of a codebase's text that is prose, not code."
RUNTIME="rust"                       # required — python | bash | fish | rust | docker
MIN_RUNTIME_VERSION="1.89"           # floor, never a pin — the workspace's rust-version
BREW_DEPS=( )                        # rustup owns the toolchain; grammars build from source
EXTERNAL_DEPS=( )                    # optional — free-form notes (not enforced)
DOCKER=0                             # optional — 1 if entrypoints are docker-run shims
