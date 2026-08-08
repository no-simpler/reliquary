#!/usr/bin/env bash
#
# Run the suite. nextest when it is present (it is in cargo/crates.txt), plain
# cargo test otherwise.

set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if command -v cargo-nextest >/dev/null 2>&1; then
    exec cargo nextest run
fi

exec cargo test
