# Manifest for the `midden` CLI — Stage-2 in-house relic.
# Sourced by ~/.config/reliquary/lib/relic.sh.
# See ~/.config/reliquary/GRADUATION.md for the full schema.

# shellcheck disable=SC2034  # a manifest is data: every field is read by the sourcer

NAME="midden" # required — published name + owner column
DESCRIPTION="Machine-wide friction corpus: what the harness cost an agent, filed as it happened."
RUNTIME="rust"             # required — python | bash | fish | rust | docker
MIN_RUNTIME_VERSION="1.89" # floor, never a pin — edition 2024 plus std file locking
BREW_DEPS=()               # rustup owns the toolchain
EXTERNAL_DEPS=("git (project key resolution; absent git degrades to the working directory)")
DOCKER=0 # optional — 1 if entrypoints are docker-run shims
