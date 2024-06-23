#!/bin/bash

# Check if Mac App Store is available in the CLI
if command -v mas &>/dev/null; then
    # Install  'Keyboard Pilot' app from Mac App Store.
    if ! [[ -d "/Applications/Keyboard Pilot.app" ]]; then
        echo "Installing Keyboard Pilot app..."
        mas install 402670023
    else
        echo "Keyboard Pilot app is already installed."
    fi

    # Install  'Intermission' app from Mac App Store.
    if ! [[ -d "/Applications/Intermission.app" ]]; then
        echo "Installing Intermission app..."
        mas install 1439431081
    else
        echo "Intermission app is already installed."
    fi
else
    echo "Cannot install Mac App Store apps: mas command not found."
fi
