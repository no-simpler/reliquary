#!/bin/bash

# Externalize the path to a variable
# Beside the plugins TPM installs, which it locates from tmux.conf's own
# directory — not the pre-XDG ~/.tmux/plugins/.
TPM_PATH="$HOME/.config/tmux/plugins/tpm"

# Clone TPM repository into the target directory if it doesn't exist
if [ ! -d "$TPM_PATH" ]; then
    print_bold -ad "Installing tmux plugin manager (TPM)..."
    git clone https://github.com/tmux-plugins/tpm "$TPM_PATH"
else
    print_info -ad "TPM already installed in $TPM_PATH"
fi
