#!/usr/bin/env bash
#
# Overrides relic::update to append the one periodic job midden needs: retention.
# `up` runs relic::update across every relic, which makes it the only recurring,
# non-interactive, time-bounded slot on this machine — so the corpus is pruned
# there rather than by a schedule of its own.
#
# Pruning never fails the update. A corpus that cannot be read is a matter for
# `midden doctor`, not for a reason to abort `up`.

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

"$dir/scripts/publish.sh"

"$dir/target/release/midden" gc --quiet || true
