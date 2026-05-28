# Re-publish in-house relics onto PATH after bootstrap.
# Idempotent; install-on-path enforces registry-based ownership.
#
# Iterates both lanes:
#   ~/.config/relics/ — public, yadm-tracked
#   ~/.config/attic/  — private; only iterates if decrypted

# shellcheck disable=SC1091
source "$HOME/.config/reliquary/lib/relic.sh" 2>/dev/null || return 0

_publish_lane() {
    local lane="$1"
    [ -d "$lane" ] || return 0
    local r
    for r in "$lane"/*/; do
        [ -f "${r}relic.sh" ] || continue
        relic::publish "$r" || echo "  publish failed: $r"
    done
}

_publish_lane "$HOME/.config/relics"
_publish_lane "$HOME/.config/attic"

unset -f _publish_lane
