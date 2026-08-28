##
## Plugins
##

# zinit runs compinit itself, from several call sites, and appends this to every
# one of them. Without it each call stops an account that did not install
# Homebrew with the insecure-directories question — `compinit -i` in 030 governs
# only our own call. Must be set before zinit.zsh is sourced, since that is when
# zinit reads it.
typeset -gA ZINIT
ZINIT[COMPINIT_OPTS]=-i

# Install and initialize zinit, if not yet installed
ZINIT_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}/zinit/zinit.git"
[ ! -d $ZINIT_HOME ] && mkdir -p "$(dirname $ZINIT_HOME)"
[ ! -d $ZINIT_HOME/.git ] && git clone https://github.com/zdharma-continuum/zinit.git "$ZINIT_HOME"
source "${ZINIT_HOME}/zinit.zsh"

if command -v zinit &>/dev/null; then
    # Add plugins
    zinit light zsh-users/zsh-syntax-highlighting
    zinit light zsh-users/zsh-completions
    zinit light zsh-users/zsh-autosuggestions
    zinit light Aloxaf/fzf-tab

    # Load oh-my-zsh library scripts
    zinit snippet OMZL::clipboard.zsh
    zinit snippet OMZL::completion.zsh
    zinit snippet OMZL::directories.zsh
    zinit snippet OMZL::functions.zsh
    zinit snippet OMZL::key-bindings.zsh
    zinit snippet OMZL::misc.zsh
    zinit snippet OMZL::spectrum.zsh
    zinit snippet OMZL::theme-and-appearance.zsh
    zinit snippet OMZL::vcs_info.zsh
fi
