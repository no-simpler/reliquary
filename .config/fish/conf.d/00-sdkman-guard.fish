# Erase inherited SDKMAN_DIR (e.g. from parent zsh) when SDKMAN is not
# actually installed, to prevent sdkman-for-fish from warning on startup.
if set -q SDKMAN_DIR; and not test -d "$SDKMAN_DIR"
    set -e SDKMAN_DIR
end
