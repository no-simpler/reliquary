#!/bin/bash

# Initialize personal bin and XDG state directories
mkdir -p ~/.local/bin
mkdir -p ~/.config/bin
mkdir -p ~/.local/state/up
mkdir -p ~/.local/state/yadm

# Establish the postcondition, not just the directories: env.d owns these two
# lanes for login and interactive shells, but the bootstrap process is neither.
# `install_on_path` refuses to publish into a directory that is not on $PATH, so
# without this every relic publish fails on a fresh account — invisible here,
# where env.d has already ordered PATH. Order matches env.d/999-path.sh:
# ~/.config/bin ahead of Homebrew, so bare `yadm` is the wrapper.
bootstrap::path_prepend "$HOME/.local/bin" "$HOME/.config/bin"
