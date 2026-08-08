#!/usr/bin/env bash
#
# Rebuild and republish. Called by `up` for every relic, so it must stay
# non-interactive and time-bounded: cargo never prompts, and a no-op rebuild
# returns immediately. Only the very first build is slow — that is the
# tree-sitter C grammars compiling once.

set -euo pipefail

dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

cd "$dir"
cargo build --release --quiet

exec "$dir/scripts/publish.sh"
