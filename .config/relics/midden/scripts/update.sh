#!/usr/bin/env bash
#
# Overrides `relic update` to append the one periodic job midden needs: retention.
# `up` runs `relic update --all`, which makes it the only recurring,
# non-interactive, time-bounded slot on this machine — so the corpus is pruned
# there rather than by a schedule of its own.
#
# Pruning never fails the update. A corpus that cannot be read is a matter for
# `midden doctor`, not a reason to abort `up`.

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Through the binary, not a sourced library: `relic` owns publishing now, and a
# second implementation of it here is how one relic comes to be published
# differently from the rest. `relic publish` never consults this script, so
# there is no recursion.
relic publish "$(basename "$dir")"

# The binary that publish just installed — a copy of the build this run made, so
# the corpus this prunes is the one that build understands.
"$HOME/.local/bin/midden" gc --quiet || true
