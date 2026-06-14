# Always-loaded zsh env (interactive AND non-interactive).
# Interactive .zshrc re-globs ~/.config/shell/interactive.d/ behind its gate;
# env.d files are idempotent so extra re-sourcing is a no-op.

export D__SHELL=zsh

# Relocate the remaining zsh startup files (.zprofile, .zshrc, .zlogin) into
# ~/.config/zsh/. ~/.zshenv must stay at $HOME root — zsh reads it before
# ZDOTDIR exists — but everything after it is sourced from ZDOTDIR.
export ZDOTDIR="$HOME/.config/zsh"

for f in ~/.config/shell/env.d/*.sh(N); do source "$f"; done
unset f

# Propagate to bash subshells spawned from this zsh.
export BASH_ENV="$HOME/.bash_env"
