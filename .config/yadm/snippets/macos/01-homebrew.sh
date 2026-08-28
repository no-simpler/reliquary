#!/bin/bash

# Setting up Homebrew
if ! command -v brew &>/dev/null; then
    print_bold -ad "Installing Homebrew..."
    /bin/bash -c "$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)"
else
    print_info -ad "Homebrew is already installed"
fi

# Put Homebrew on PATH in *every* branch, not only the one that installed it.
# Everything downstream — the Brewfile, python3 for the manifest reader, node,
# git — resolves through this. See lib/00-path.sh for what skipping it cost.
if bootstrap::brew_shellenv; then
    print_info -ad "Homebrew on PATH at $(brew --prefix)"
else
    print_error -ad "Homebrew not found after setup; later snippets will use system tools"
fi
