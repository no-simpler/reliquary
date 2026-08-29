#!/usr/bin/env bash
#
# Overrides `relic update` to append the one periodic job docket needs: packing
# the depot's history. The repository is configured with gc.auto=0, because an
# auto-gc firing under the SessionStart hook would print into a terminal that
# asked for nothing — so this is the only compaction it gets, and `up` is the
# only recurring, non-interactive, time-bounded slot on this machine.
#
# Packing never fails the update. A depot that cannot be read is a matter for
# `docket doctor`, not a reason to abort `up`.

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Through the binary, not a sourced library: `relic` owns publishing now, and a
# second implementation of it here is how one relic comes to be published
# differently from the rest. `relic publish` never consults this script, so
# there is no recursion.
relic publish "$(basename "$dir")"

depot="${DOCKET_ROOT:-$HOME/.claude/docket}"
if [[ -d "$depot/.git" ]]; then
    git -C "$depot" gc --quiet || true
fi
