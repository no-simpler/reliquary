#!/usr/bin/env bash
#
# Publish docket onto PATH.
#
# Overrides relic::publish because a compiled relic has nothing to publish until
# it is built. The entrypoint points into target/release/, which is absent on a
# fresh machine, and relic::publish guards each entrypoint with [[ -e ]] — the
# dangling link would be skipped and bootstrap would report "no entrypoints
# published". So: build, wire the entrypoint, then publish.
#
# The entrypoint is deliberately not version-controlled. It names a build
# artifact, so it belongs with target/.

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
binary="$dir/target/release/docket"

# Unconditionally, not only when the binary is missing: guarding on absence
# would publish whatever was built last, shipping a source change as a stale
# binary. cargo is incremental, so an up-to-date tree makes this a no-op.
printf 'relic[docket]: building %s\n' "$binary"
( cd "$dir" && cargo build --release --quiet )

mkdir -p "$dir/entrypoints"
ln -sfn ../target/release/docket "$dir/entrypoints/docket"

# shellcheck disable=SC1091
source "$HOME/.config/reliquary/lib/relic.sh"

# relic::publish re-runs this script when scripts/publish.sh is executable, so
# call the default body directly instead and avoid recursing.
relic::check_deps "$dir"

(
    export META_NAME="docket"
    # shellcheck disable=SC1091
    source "$HOME/.config/reliquary/lib/install-on-path.sh"
    install_on_path "$dir/entrypoints/docket" "docket"
)
