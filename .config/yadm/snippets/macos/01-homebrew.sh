#!/bin/bash

# Setting up Homebrew
if ! command -v brew &>/dev/null; then
    print_bold -ad "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)" && eval "$(/opt/homebrew/bin/brew shellenv)"
else
    print_info -ad "Homebrew is already installed"
fi
