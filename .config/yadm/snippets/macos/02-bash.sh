#!/bin/bash
#
# Bedrock: ensure modern bash (>=5) as early as feasible — right after Homebrew
# (01-homebrew.sh) and before the bulk Brewfile (02-brewfile.sh; this file sorts
# ahead of it: "02-bash" < "02-brewfile"). macOS ships only /bin/bash 3.2
# (frozen for licensing); countless scripts target `#!/usr/bin/env bash` and
# need 5.x. See ~/.config/reliquary/BEDROCK.md.
#
# NOTE: installing bash 5 does NOT upgrade the already-running bootstrap
# interpreter — this snippet (like all of them) stays strictly bash-3.2-safe.

BREW_PREFIX="$(brew --prefix 2>/dev/null)"
BASH_PATH="$BREW_PREFIX/bin/bash"

# Install modern bash up front (brew bundle later no-ops it if already present)
if [ ! -x "$BASH_PATH" ]; then
    print_bold -ad "Installing modern bash (bedrock)..."
    brew install bash
else
    print_info -ad "modern bash already installed"
fi

# Register it as a valid login shell (mirrors shared/12-fish.sh)
# Whole-line match: a substring test would accept the path inside a comment.
# The write is checked — an unchecked `sudo tee` here announced "Registering…"
# and then said nothing when the sudo failed, which is how this machine ran for
# two months with the registration silently absent.
if [ ! -x "$BASH_PATH" ]; then
    print_warning -ad "modern bash not found at $BASH_PATH; skipping /etc/shells registration"
elif grep -qxF "$BASH_PATH" /etc/shells 2>/dev/null; then
    print_info -ad "modern bash already registered in /etc/shells"
elif echo "$BASH_PATH" | sudo tee -a /etc/shells >/dev/null; then
    print_success -ad "Registered $BASH_PATH in /etc/shells"
else
    print_error -ad "Could not register $BASH_PATH in /etc/shells (needs admin rights); it cannot serve as a login shell until it is"
fi
