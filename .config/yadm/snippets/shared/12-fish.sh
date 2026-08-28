#!/bin/bash

FISH_PATH="/opt/homebrew/bin/fish"

# Install fish if not present
if ! command -v fish &>/dev/null; then
    print_bold -ad "Installing fish shell..."
    brew install fish
else
    print_info -ad "fish shell already installed"
fi

# Register fish as a valid login shell. Whole-line match and a checked write,
# for the reasons macos/02-bash.sh records beside the same pair of lines.
if grep -qxF "$FISH_PATH" /etc/shells 2>/dev/null; then
    print_info -ad "fish already registered in /etc/shells"
elif echo "$FISH_PATH" | sudo tee -a /etc/shells >/dev/null; then
    print_success -ad "Registered $FISH_PATH in /etc/shells"
else
    print_error -ad "Could not register $FISH_PATH in /etc/shells (needs admin rights); it cannot serve as a login shell until it is"
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
