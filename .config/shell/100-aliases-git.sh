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
alias gp='git push origin -u HEAD'
alias gpt='git push --tags'
alias gff='git merge --ff-only @{u}'
alias glg='git log --all --decorate=full --show-signature'

gbp() {
    local branch="${1:-main}"
    git branch --merged "$branch" | grep -v '^\*' | grep -v "^$branch$" | xargs -r git branch -d
}

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
