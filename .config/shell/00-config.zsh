# Zsh shell configuration

##
## General configuration
##

# Enable extended glob
setopt extended_glob

# Automatically change to a directory by typing its name without the 'cd' command
setopt auto_cd

# Use 'cd' as 'pushd', pushing the current directory onto the stack before changing
setopt auto_pushd

# Prevent duplicate directories from being added to the directory stack
setopt pushd_ignore_dups

# Modify 'pushd' to rotate the directory stack instead of swapping directories.
setopt pushdminus

# Directories to include under consideration when cd-ing
[ -d $HOME/Developer ] && cdpath=($HOME/Developer)

# Skip verification of insecure directories
ZSH_DISABLE_COMPFIX=true

# Prompt
set_prompt() {
    PROMPT="$([ $? -eq 0 ] && echo '%F{green}' || echo '%F{red}')➜%f%b %B%F{cyan}${PWD##*/}%f%b "
}
autoload -Uz add-zsh-hook
add-zsh-hook precmd set_prompt

##
## Plugins
##

# Install and initialize zinit, if not yet installed
ZINIT_HOME="${XDG_DATA_HOME:-${HOME}/.local/share}/zinit/zinit.git"
[ ! -d $ZINIT_HOME ] && mkdir -p "$(dirname $ZINIT_HOME)"
[ ! -d $ZINIT_HOME/.git ] && git clone https://github.com/zdharma-continuum/zinit.git "$ZINIT_HOME"
source "${ZINIT_HOME}/zinit.zsh"

if command -v zinit &>/dev/null; then
    # Add plugins
    zinit light zsh-users/zsh-completions
    zinit light zsh-users/zsh-syntax-highlighting
    zinit light zsh-users/zsh-autosuggestions

    # Load oh-my-zsh library scripts
    zinit snippet OMZL::clipboard.zsh
    zinit snippet OMZL::completion.zsh
    zinit snippet OMZL::directories.zsh
    zinit snippet OMZL::functions.zsh
    zinit snippet OMZL::history.zsh
    zinit snippet OMZL::key-bindings.zsh
    zinit snippet OMZL::misc.zsh
    zinit snippet OMZL::spectrum.zsh
    zinit snippet OMZL::theme-and-appearance.zsh
    zinit snippet OMZL::vcs_info.zsh
fi
