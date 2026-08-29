#!/usr/bin/env bash
#
# Overrides `relic update` to append the one periodic job mantra needs: sweeping
# state for sessions nobody will resume. `up` runs `relic update --all`, which
# makes it the only recurring, non-interactive, time-bounded slot on this
# machine — so the sweep happens there rather than on a schedule of its own.
#
# Sweeping never fails the update. A state directory that cannot be read is a
# matter for `mantra doctor`, not a reason to abort `up`.

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

# Through the binary, not a sourced library: `relic` owns publishing now, and a
# second implementation of it here is how one relic comes to be published
# differently from the rest. `relic publish` never consults this script, so
# there is no recursion.
relic publish "$(basename "$dir")"

# The binary that publish just installed — a copy of the build this run made, so
# the state this sweeps is the one that build understands. Its one line of
# output is redirected rather than suppressed by a flag: a quiet mode whose only
# caller is a redirect is surface for nothing.
"$HOME/.local/bin/mantra" gc >/dev/null || true
