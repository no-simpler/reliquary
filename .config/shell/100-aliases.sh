##
## Directories
##

DEV_DIR="$HOME/Developer"
SITES_DIR="$HOME/Sites"
[ -d "$DEV_DIR" ] && alias dvd="cd $DEV_DIR"
[ -d "$SITES_DIR" ] && alias sts="cd $SITES_DIR"

##
## Shorthand for 'ls' command
##

## ls
case $(uname -s) in
Darwin)
    ## macOS ls (BSD):
    #.  -F  - Add symbolic indication of file types
    #.  -G  - Colorize output
    alias ls='ls -FG'
    ;;
Linux)
    ## GNU ls:
    #.  -F              - Add symbolic indication of file types
    #.  --color=always  - Colorize output
    alias ls='ls -F --color=always'
    ;;
*) ## Other OS:
    #.  -F  - Add symbolic indication of file types
    alias ls='ls -F' ;;
esac

## lsa:
#.  -a  - Include names starting with dots
alias lsa='ls -a'

## ll (long-ish format):
#.  -h  - Base-2 sizes with unit suffixes
#.  -l  - Long format, one line per file
alias ll='ls -hl'

## la (ll + lsa):
#.  -a  - Include names starting with dots
alias la='ll -a'

##
## Shorthand for basic filesystem manipulations
##

alias md='mkdir -p'
alias rmr='rm -rf'

##
## Directory navigation aliases
##

case $D__SHELL in
zsh)
    alias -g ...='../..'
    alias -g ....='../../..'
    alias -g .....='../../../..'
    alias -g ......='../../../../..'
    ;;
bash)
    alias ..='cd ..'
    alias ...='cd ../..'
    alias ....='cd ../../..'
    alias .....='cd ../../../..'
    ;;
esac

alias -- -='cd -'
alias 1='cd -1'
alias 2='cd -2'
alias 3='cd -3'
alias 4='cd -4'
alias 5='cd -5'
alias 6='cd -6'
alias 7='cd -7'
alias 8='cd -8'
alias 9='cd -9'

##
## Shell switching aliases
##

case $D__SHELL in
bash) alias zsh='/usr/bin/env zsh' ;;
zsh) alias bash='/usr/bin/env bash' ;;
esac

##
## $PATH aliases
##

alias lpath='printf "%b\n" "${PATH//:/\\n}\n"'
alias lmanpath='printf "%b\n" "${MANPATH//:/\\n}\n"'
alias libpath='printf "%b\n" "${LD_LIBRARY_PATH//:/\\n}\n"'

##
## Colorful tree:
#.  -C  - Colorize output
#.  -u  - (files) Include user's name/id
#.  -s  - (files) Include size
#.  -h  - (files) Human-readable sizes
#.  -N  - Print non-printables as is
##
if tree --version &>/dev/null; then
    alias tree='tree -CsuhN'
fi

##
## More informative which
##
alias which='\type -a'

##
## Disk usage for file:
#.  -k    - Block counts in Kbytes (1024 bytes)
#.  -h    - Human-readable sizes
#.  -d 1  - Go one level deep (list current dir)
##
alias du='du -kh -d 1'

##
## Free space:
#.  -k  - Block counts in Kbytes (1024 bytes)
#.  -h  - Human-readable sizes
##
alias df='df -kh'

##
## zsh-specific
##

if [[ $D__SHELL == zsh ]]; then
    alias rzc='rm -f ~/.zcompdump* && source ~/.zshrc'
fi

##
## tmux
##

alias tt='tca'

##
## Test Vim
##

alias tvim='vim -u NONE -N'
