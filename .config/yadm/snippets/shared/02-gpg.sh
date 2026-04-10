#!/bin/bash

# Configure GPG agent to allow loopback pinentry (needed for yadm's 1Password-based decryption)
mkdir -p ~/.gnupg
if ! grep -q 'allow-loopback-pinentry' ~/.gnupg/gpg-agent.conf 2>/dev/null; then
    echo 'allow-loopback-pinentry' >> ~/.gnupg/gpg-agent.conf
    print_info "Added allow-loopback-pinentry to gpg-agent.conf"
fi

# Point yadm at the 1Password-aware GPG wrapper (available after yadm decrypt)
if [[ -x "$HOME/.config/bin/gpg-yadm-op" ]]; then
    yadm config yadm.gpg-program "$HOME/.config/bin/gpg-yadm-op"
    print_info "Configured yadm to use 1Password for encryption passphrase"
fi
