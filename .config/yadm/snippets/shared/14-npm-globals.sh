#!/bin/bash

# Restore the committed set of global npm packages (see ~/.config/npm/globals.txt).
# Requires node/npm to already be present (installed via the base Brewfile during
# 02-brewfile.sh). Idempotent — packages already present are skipped. `up` keeps
# them current afterwards (npm update -g).

GLOBALS_FILE="$HOME/.config/npm/globals.txt"

if ! command -v npm &>/dev/null; then
    print_warning -ad "npm not found; skipping global npm packages"
elif [ ! -f "$GLOBALS_FILE" ]; then
    print_info -ad "No npm globals manifest at $GLOBALS_FILE, skipping"
else
    while IFS= read -r line; do
        pkg="${line%%#*}"
        pkg="$(echo "$pkg" | xargs)"
        [ -z "$pkg" ] && continue
        if npm ls -g "$pkg" &>/dev/null; then
            print_info -ad "$pkg already installed"
        else
            print_bold -ad "Installing $pkg..."
            npm install -g "$pkg"
        fi
    done < "$GLOBALS_FILE"
fi
