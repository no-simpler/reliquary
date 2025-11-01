##
## Zsh configuration
##

# Key bindings
bindkey -e                      # use Emacs bindings
bindkey '^p' history-search-backward
bindkey '^n' history-search-forward

# Globbing
setopt extended_glob            # match advanced patterns such as `ls **/*.(jpg|png)`
setopt null_glob                # when nothing is match, return empty string, not pattern itself

# Directory navigation
setopt auto_pushd               # marry `cd` with `pushd`, to use `popd` and `dirs`
setopt pushd_ignore_dups        # don't push duplicate directories
setopt pushd_minus              # make `pushd -N` rotate stack to the left

# History file
HISTFILE=~/.zsh_history         # where to save history
HISTSIZE=50000                  # size of history in memory
SAVEHIST=50000                  # amount of history to save on closing
HISTDUP=erase                   # when saving new command, erase earlier duplicates

# History
setopt append_history           # append to HISTFILE instead of overwriting it
setopt share_history            # share history across zsh sessions
setopt extended_history         # record timestamp of command in HISTFILE
setopt hist_ignore_space        # ignore commands that start with space (for sensitive info)
setopt hist_ignore_all_dups     # ignore all duplicated commands
setopt hist_ignore_dups         # ignore consecutive duplicated commands
setopt hist_find_no_dups        # ignore duplicated commands when searching history
setopt hist_save_no_dups        # don't save duplicates to HISTFILE but allow them during session
setopt hist_expire_dups_first   # delete duplicates first when HISTFILE size exceeds HISTSIZE
setopt hist_verify              # show command with history expansion to user before running it

# Load completions
FPATH="$HOME/.config/zsh/completion:$FPATH"
autoload -Uz compinit && compinit

# Replace tab completion with fzf
zstyle ':completion:*' menu no
zstyle ':fzf-tab:complete:cd:*' fzf-preview 'ls --color $realpath'
zstyle ':fzf-tab:complete:__zoxide_z:*' fzf-preview 'ls --color $realpath'

# Skip verification of insecure directories
ZSH_DISABLE_COMPFIX=true
