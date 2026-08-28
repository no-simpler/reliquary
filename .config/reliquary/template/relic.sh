# Manifest for an in-house relic. Sourced by ~/.config/reliquary/lib/relic.sh.
# See ~/.config/reliquary/GRADUATION.md for the full schema.

# shellcheck disable=SC2034  # a manifest is data: every field is read by the sourcer

NAME=""                # required — published name + META_NAME
DESCRIPTION=""         # optional — one-line summary
RUNTIME="rust"         # required — rust by default; see the stance
RUNTIME_EXEMPTION=""   # required when RUNTIME is not rust — why not
MIN_RUNTIME_VERSION="" # optional — e.g. "3.11"; enforced at publish time
ENTRYPOINTS=()         # optional — compiled only; defaults to ( "$NAME" )
BREW_DEPS=()           # optional — brew package names; verified at publish
EXTERNAL_DEPS=()       # optional — free-form notes (not enforced)
DOCKER=0               # optional — 1 if entrypoints are docker-run shims
