#!/bin/bash

# Install Rust if it is not yet present
if ! command -v rustup &>/dev/null; then
    print_bold -ad "Installing rustup..."
    # -y: the installer's stdin is the pipe curl writes into, never a tty, so
    # its interactive confirmation cannot be answered — it aborts with "Unable
    # to run interactively" and cargo never arrives.
    # --no-modify-path: cargo's PATH is owned by shell/env.d/040-env.{sh,fish},
    # so rustup must not inject `. ~/.cargo/env` into ~/.profile or any rc file.
    curl --proto '=https' --tlsv1.2 https://sh.rustup.rs -sSf | sh -s -- -y --no-modify-path
else
    print_info -ad "rustup already installed"
fi

# rustup was installed with --no-modify-path, so cargo is on no PATH the bootstrap
# process can see: env.d only runs in interactive/login shells. Snippets 12 (publish
# relics), 13 (cargo binaries) and 99 (up) all need it. Sourcing the file rustup
# itself writes keeps this from drifting out of sync with env.d. POSIX-sh,
# bash-3.2-safe, idempotent, and process-local — so --no-modify-path still holds.
[ -f "$HOME/.cargo/env" ] && . "$HOME/.cargo/env"
