#!/bin/bash

# Restore the committed set of cargo-installed binaries (see ~/.config/cargo/crates.txt).
# Requires rustup/cargo to already be present (see 11-rustup.sh). Uses cargo-binstall
# for fast prebuilt installs, falling back to `cargo install`. Idempotent — crates
# already installed are skipped. `up` keeps versions current afterwards.

CRATES_FILE="$HOME/.config/cargo/crates.txt"

if ! command -v cargo &>/dev/null; then
    print_warning -ad "cargo not found; skipping cargo binaries"
elif [ ! -f "$CRATES_FILE" ]; then
    print_info -ad "No cargo crates manifest at $CRATES_FILE, skipping"
else
    # Bootstrap cargo-binstall (fast prebuilt installs) if absent
    if ! command -v cargo-binstall &>/dev/null; then
        print_bold -ad "Installing cargo-binstall..."
        curl -L --proto '=https' --tlsv1.2 -sSf \
            https://raw.githubusercontent.com/cargo-bins/cargo-binstall/main/install-from-binstall-release.sh | bash
        [ -d "$HOME/.cargo/bin" ] && export PATH="$HOME/.cargo/bin:$PATH"
    fi

    installed="$(cargo install --list)"
    while IFS= read -r line; do
        crate="${line%%#*}"
        crate="$(echo "$crate" | xargs)"
        [ -z "$crate" ] && continue
        if printf '%s\n' "$installed" | grep -qE "^${crate} v"; then
            print_info -ad "$crate already installed"
        else
            print_bold -ad "Installing $crate..."
            cargo binstall --no-confirm "$crate" 2>/dev/null || cargo install "$crate"
        fi
    done < "$CRATES_FILE"
fi
