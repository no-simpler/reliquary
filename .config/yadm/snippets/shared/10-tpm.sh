#!/bin/bash

# Externalize the path to a variable
TPM_PATH="$HOME/.tmux/plugins/tpm"

# Clone TPM repository into the target directory if it doesn't exist
if [ ! -d "$TPM_PATH" ]; then
    print_bold -ad "Installing tmux plugin manager (TPM)..."
    git clone https://github.com/tmux-plugins/tpm "$TPM_PATH"
else
    print_info -ad "TPM already installed in $TPM_PATH"
fi
