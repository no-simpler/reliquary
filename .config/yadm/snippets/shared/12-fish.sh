#!/bin/bash

FISH_PATH="/opt/homebrew/bin/fish"

# Install fish if not present
if ! command -v fish &>/dev/null; then
    print_bold -ad "Installing fish shell..."
    brew install fish
else
    print_info -ad "fish shell already installed"
fi

# Register fish as a valid login shell
if ! grep -qF "$FISH_PATH" /etc/shells 2>/dev/null; then
    print_bold -ad "Registering fish in /etc/shells..."
    echo "$FISH_PATH" | sudo tee -a /etc/shells >/dev/null
else
    print_info -ad "fish already registered in /etc/shells"
fi

# Install Fisher plugin manager
if [ ! -f "$HOME/.config/fish/functions/fisher.fish" ]; then
    print_bold -ad "Installing Fisher plugin manager..."
    fish -c "curl -sL https://raw.githubusercontent.com/jorgebucaran/fisher/main/functions/fisher.fish | source && fisher install jorgebucaran/fisher"
else
    print_info -ad "Fisher already installed"
fi

# Install Fisher plugins from fish_plugins manifest
if [ -f "$HOME/.config/fish/fish_plugins" ]; then
    print_bold -ad "Installing Fisher plugins..."
    fish -c "fisher update"
else
    print_info -ad "No fish_plugins manifest found, skipping plugin install"
fi
