# Manifest for an in-house relic. Sourced by ~/.config/reliquary/lib/relic.sh.
# See ~/.config/reliquary/GRADUATION.md for the full schema.

NAME=""                              # required — published name + META_NAME
DESCRIPTION=""                       # optional — one-line summary
RUNTIME=""                           # required — python | bash | fish | rust | docker
MIN_RUNTIME_VERSION=""               # optional — e.g. "3.11"; enforced at publish time
BREW_DEPS=( )                        # optional — brew package names; verified at publish
EXTERNAL_DEPS=( )                    # optional — free-form notes (not enforced)
DOCKER=0                             # optional — 1 if entrypoints are docker-run shims
