## Interactive only
status is-interactive; or return

##
## Directories
##

set -l DEV_DIR "$HOME/Developer"
set -l SITES_DIR "$HOME/Sites"
test -d $DEV_DIR; and alias dvd "cd $DEV_DIR"
test -d $SITES_DIR; and alias sts "cd $SITES_DIR"

##
## Shorthand for 'ls' command (macOS)
##

alias ls 'ls -FG'
alias lsa 'ls -a'
alias ll 'ls -hl'
alias la 'll -a'

##
## Shorthand for basic filesystem manipulations
##

alias md 'mkdir -p'
alias rmr 'rm -rf'

##
## Directory navigation abbreviations
## Fish abbreviations expand inline — you see the full command before executing
##

abbr --add ... 'cd ../..'
abbr --add .... 'cd ../../..'
abbr --add ..... 'cd ../../../..'
abbr --add ...... 'cd ../../../../..'
abbr --add -- - 'cd -'

## Fish has built-in `cdh` for interactive directory history.
## These numbered aliases mirror zsh's `cd -N` with auto_pushd.
## Fish uses `dirh` / `cdh` / `prevd` / `nextd` natively.
alias 1 'prevd 1'
alias 2 'prevd 2'
alias 3 'prevd 3'
alias 4 'prevd 4'
alias 5 'prevd 5'
alias 6 'prevd 6'
alias 7 'prevd 7'
alias 8 'prevd 8'
alias 9 'prevd 9'

##
## Shell switching
##

alias bash '/usr/bin/env bash'
alias zsh '/usr/bin/env zsh'

##
## $PATH aliases
##

alias lpath 'string join \n $PATH'
alias lmanpath 'string join \n $MANPATH'

##
## Colorful tree
##

if command -q tree
    alias tree 'tree -CsuhN'
end

##
## More informative which
##

alias which 'type -a'

##
## Disk usage / free space
##

alias du 'du -kh -d 1'
alias df 'df -kh'

##
## tmux
##

alias tt tca

##
## Test Vim
##

alias tvim 'vim -u NONE -N'
