# Publish in-house relics onto PATH after bootstrap.
#
# Three lines and a hand-off. The seed builds `relic` from source and publishes
# it; `relic publish --all` does both lanes, skipping an attic that has not been
# decrypted because a relic with no readable manifest is a directory it knows
# nothing about.
#
# Idempotent; install-on-path enforces a single shared registry and unique PATH
# names.

# Fold any legacy per-meta registries into the single .reliquary-managed file
# before publishing, so the first publish writes into the consolidated registry.
# Idempotent; tolerates absence.
# shellcheck disable=SC1091
source "$HOME/.config/reliquary/lib/install-on-path.sh" 2>/dev/null &&
    install_on_path_migrate_registries

# shellcheck disable=SC1091
source "$HOME/.config/reliquary/lib/relic.sh" 2>/dev/null || return 0

relic::seed || echo "  relic seed failed — nothing is published"
