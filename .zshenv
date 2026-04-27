# Always-loaded zsh env (interactive AND non-interactive).
# Interactive .zshrc re-globs ~/.config/shell/interactive.d/ behind its gate;
# env.d files are idempotent so extra re-sourcing is a no-op.

export D__SHELL=zsh

for f in ~/.config/shell/env.d/*.sh(N); do source "$f"; done
unset f

# Propagate to bash subshells spawned from this zsh.
export BASH_ENV="$HOME/.bash_env"
