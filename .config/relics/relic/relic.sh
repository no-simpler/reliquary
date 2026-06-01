# Manifest for the `relic` CLI — the first in-house (Stage-2) relic.
# Sourced by ~/.config/reliquary/lib/relic.sh.
# See ~/.config/reliquary/GRADUATION.md for the full schema.

NAME="relic"                         # required — published name + owner column
DESCRIPTION="Manage Reliquary relics: list, status, publish, test, update, registry."
RUNTIME="bash"                       # required — python | bash | fish | rust | docker
MIN_RUNTIME_VERSION="3.2"            # macOS floor; the CLI is bash-3.2 compatible
BREW_DEPS=( )                        # none — awk/git are part of the base system/Brewfile
EXTERNAL_DEPS=( )                    # optional — free-form notes (not enforced)
DOCKER=0                             # optional — 1 if entrypoints are docker-run shims
