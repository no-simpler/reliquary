#!/bin/bash

# Seed the benefactor `gh` config directory. The directory itself stays untracked,
# because `gh` writes an OAuth token into hosts.yml there; only the non-secret half is
# reproduced. Without this, ~/.config/gh/config.yml (git_protocol, aliases) silently
# stops applying inside the benefactor tree once the shim redirects GH_CONFIG_DIR.
#
# Copied, not symlinked: `gh config set` in a benefactor repository would write through
# a symlink into the tracked personal config. Guarded on absence, so a config that has
# since diverged is never clobbered.
mkdir -p "$HOME/.config/gh-benefactor"
if [ ! -e "$HOME/.config/gh-benefactor/config.yml" ] && [ -f "$HOME/.config/gh/config.yml" ]; then
    cp "$HOME/.config/gh/config.yml" "$HOME/.config/gh-benefactor/config.yml"
fi
