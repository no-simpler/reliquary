#!/bin/bash

# Install Rust if it is not yet present
if ! command -v rustup &>/dev/null; then
    print_bold -ad "Installing rustup..."
    curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh
else
    print_info -ad "rustup already installed"
fi
