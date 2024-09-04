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
## Git aliases
##

alias gf='git fetch --prune --prune-tags --tags --force'
alias gaa='git add --all'
alias gaas='git add --all && git status'
alias grs='git restore --staged'
alias grc='git rm --cached -rf .'
alias gs='git status'
alias gu='git rm --cached -rf . &>/dev/null; git add --all; git status'
alias gc='git commit'
alias gp='git push'
alias gpt='git push --tags'
alias gff='git merge --ff-only @{u}'
alias glg='git log --all --decorate=full --show-signature'

# git-log (80-character-wide semi-oneliners)
alias glfr="git --no-pager log --format='%w(0,0,7)%C(auto)%h%Creset %<|(56,trunc)%s %C(yellow)(%as)%Creset %C(blue)%<(10,trunc)%al%C(auto)%+d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always"
alias glr="glfr -10"
alias gllr="glfr -20"
alias glllr="glfr -40"
alias gllllr="glfr -80"

# git-log (80-character-wide semi-oneliners) with output re-directed to less
alias glf="glfr | less -R"
alias gl="glfr -10 | less -R"
alias gll="glfr -20 | less -R"
alias glll="glfr -40 | less -R"
alias gllll="glfr -80 | less -R"

# git-log (140-character-wide semi-oneliners)
alias gwlfr="git --no-pager log --format='%w(0,0,7)%C(auto)%h%Creset %<|(116,trunc)%s %C(yellow)(%as)%Creset %C(blue)%<(10,trunc)%al%C(auto)%+d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always"
alias gwlr="gwlfr -10"
alias gwllr="gwlfr -20"
alias gwlllr="gwlfr -40"
alias gwllllr="gwlfr -80"

# git-log (140-character-wide semi-oneliners) with output re-directed to less
alias gwlf="gwlfr | less -R"
alias gwl="gwlfr -10 | less -R"
alias gwll="gwlfr -20 | less -R"
alias gwlll="gwlfr -40 | less -R"
alias gwllll="gwlfr -80 | less -R"

# git-log (unlimited true oneliners)
alias golfr="git --no-pager log --format='%C(auto)%h%Creset %s %C(yellow)(%as)%Creset %C(blue)%al%C(auto)%d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always"
alias golr="golfr -10"
alias gollr="golfr -20"
alias golllr="golfr -40"
alias gollllr="golfr -80"

# git-log (unlimited true oneliners) with output re-directed to less
alias golf="golfr | less -R"
alias gol="golfr -10 | less -R"
alias goll="golfr -20 | less -R"
alias golll="golfr -40 | less -R"
alias gollll="golfr -80 | less -R"

##
## $PATH aliases
##

alias path='printf "%b\n" "${PATH//:/\\n}\n"'
alias manpath='printf "%b\n" "${MANPATH//:/\\n}\n"'
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
## Docker
##

alias dcu='docker compose up -d'
alias dcb='docker compose up -d --build --no-deps'
alias dce='docker compose exec'
alias dcd='docker compose down'
alias dcdd='docker compose down --volumes --rmi all --remove-orphans'
alias dcl='docker compose logs -f'

##
## yadm
##

# Wrap yadm executable to add custom commands to it
alias yadm=~/.pbin/yadm-wrapper
alias ya='yadm add'
alias yf='yadm fetch --prune --prune-tags --tags --force'
alias yrs='yadm restore --staged'
alias ys='yadm status'
alias yc='yadm commit'
alias yp='yadm push'
alias ypt='yadm push --tags'
alias yff='yadm merge --ff-only @{u}'
alias ypff='yadm pull --ff-only'
alias ywlf="yadm --no-pager log --format='%w(0,0,7)%C(auto)%h%Creset %<|(116,trunc)%s %C(yellow)(%as)%Creset %C(blue)%<(10,trunc)%al%C(auto)%+d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always | less -R"

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
