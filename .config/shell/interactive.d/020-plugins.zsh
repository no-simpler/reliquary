##
## Plugins
##

# OMZL::completion.zsh runs its own compinit, and oh-my-zsh's compfix reads this
# variable at that moment — so it has to be set before zinit sources anything,
# not beside our own compinit in 030. On an account that did not install
# Homebrew every such call stops the shell with a y/n question; ours takes -i,
# and this is the same answer for the ones we do not own.
ZSH_DISABLE_COMPFIX=true

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
