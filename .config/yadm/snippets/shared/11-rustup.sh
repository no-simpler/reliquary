#!/bin/bash

# Install Rust if it is not yet present
if ! command -v rustup &>/dev/null; then
    print_bold -ad "Installing rustup..."
    # --no-modify-path: cargo's PATH is owned by shell/env.d/040-env.{sh,fish},
    # so rustup must not inject `. ~/.cargo/env` into ~/.profile or any rc file.
    curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh -s -- --no-modify-path
else
    print_info -ad "rustup already installed"
fi
