#!/usr/bin/env bash
#
# The gate: format, lint, suite. Fail-fast in ascending cost, so the cheapest
# station reports first. `relic test docket` runs this same file. Nothing here
# takes a flag to drop a station — a partial run is a pre-check, not a pass.

set -euo pipefail

cd "$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

if ! cargo fmt --all --check; then
    printf '\nformatting: run `cargo fmt --all`\n' >&2
    exit 1
fi

cargo clippy --all-targets --all-features -- -D warnings

if command -v cargo-nextest >/dev/null 2>&1; then
    exec cargo nextest run
fi

exec cargo test
