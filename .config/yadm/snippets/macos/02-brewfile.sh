#!/bin/bash

# Install packages from the Brewfile
BREWFILE_PATH="$HOME/.config/brew/Brewfile"
if [ -f "$BREWFILE_PATH" ]; then
    print_bold -ad "Installing packages from Brewfile..."
    brew bundle --file="$BREWFILE_PATH"
else
    print_error -ad "Brewfile not found at $BREWFILE_PATH"
fi
