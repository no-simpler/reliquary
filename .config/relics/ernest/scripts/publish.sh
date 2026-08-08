#!/usr/bin/env bash
#
# Publish ernest onto PATH.
#
# Overrides relic::publish because a compiled relic has nothing to publish until
# it is built. The entrypoint points into target/release/, which is absent on a
# fresh machine, and relic::publish guards each entrypoint with [[ -e ]] — the
# dangling link would be skipped and bootstrap would report "no entrypoints
# published". So: build if needed, wire the entrypoint, then publish.
#
# The entrypoint is deliberately not version-controlled. It names a build
# artifact, so it belongs with target/ — and a symlink resolving to a compiled
# binary trips Reliquary's pre_commit binary guard.

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$dir/target/release/ernest"

if [[ ! -x "$binary" ]]; then
    printf 'relic[ernest]: building %s\n' "$binary"
    ( cd "$dir" && cargo build --release --quiet )
fi

mkdir -p "$dir/entrypoints"
ln -sfn ../target/release/ernest "$dir/entrypoints/ernest"

# shellcheck disable=SC1091
source "$HOME/.config/reliquary/lib/relic.sh"

# relic::publish re-runs this script when scripts/publish.sh is executable, so
# call the default body directly instead and avoid recursing.
relic::check_deps "$dir"

(
    export META_NAME="ernest"
    # shellcheck disable=SC1091
    source "$HOME/.config/reliquary/lib/install-on-path.sh"
    install_on_path "$dir/entrypoints/ernest" "ernest"
)
