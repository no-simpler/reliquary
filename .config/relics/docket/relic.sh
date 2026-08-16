# Manifest for the `docket` CLI — Stage-2 in-house relic.
# Sourced by ~/.config/reliquary/lib/relic.sh.
# See ~/.config/reliquary/GRADUATION.md for the full schema.

NAME="docket"                        # required — published name + owner column
DESCRIPTION="Outstanding agentic work, per project: handoffs, relays and specs bridging sessions."
RUNTIME="rust"                       # required — python | bash | fish | rust | docker
MIN_RUNTIME_VERSION="1.89"           # floor, never a pin — edition 2024 plus std file locking
BREW_DEPS=( )                        # rustup owns the toolchain
EXTERNAL_DEPS=( "git (project keys, and the depot's version control; absent git degrades to the working directory as the key, and no item can be closed)" )
DOCKER=0                             # optional — 1 if entrypoints are docker-run shims
