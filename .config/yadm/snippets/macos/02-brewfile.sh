#!/bin/bash

# Install packages from the Brewfile
BREWFILE_PATH="$HOME/.config/brew/Brewfile"
if [ -f "$BREWFILE_PATH" ]; then
    echo "Installing packages from Brewfile..."
    brew bundle --file="$BREWFILE_PATH"
else
    echo "Brewfile not found at $BREWFILE_PATH"
fi
