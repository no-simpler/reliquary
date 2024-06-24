#!/bin/bash

# Check if Mac App Store is available in the CLI
if command -v mas &>/dev/null; then
    print_bold -ad "Installing Mac App Store apps"

    # Install  'Keyboard Pilot' app from Mac App Store.
    if ! [[ -d "/Applications/Keyboard Pilot.app" ]]; then
        print_bold "Installing Keyboard Pilot app..."
        mas install 402670023
    else
        print_info "Keyboard Pilot app is already installed."
    fi

    # Install  'Intermission' app from Mac App Store.
    if ! [[ -d "/Applications/Intermission.app" ]]; then
        print_bold "Installing Intermission app..."
        mas install 1439431081
    else
        print_info "Intermission app is already installed."
    fi

    # Install  'Ulysses' app from Mac App Store.
    if ! [[ -d "/Applications/Ulysses.app" ]]; then
        print_bold "Installing Ulysses app..."
        mas install 1225570693
    else
        print_info "Ulysses app is already installed."
    fi
else
    print_error -ad "Cannot install Mac App Store apps: mas command not found."
fi
