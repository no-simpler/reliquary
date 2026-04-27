##
## yadm
##

# Wrap yadm executable to add custom commands to it
alias yadm=~/.config/bin/yadm-wrapper
alias ya='yadm add'
alias yf='yadm fetch --prune --prune-tags --tags --force'
alias yrs='yadm restore --staged'
alias ys='yadm status'
alias yc='yadm commit'
alias yp='yadm push'
alias ypt='yadm push --tags'
alias yff='yadm merge --ff-only @{u}'
alias ypff='yadm pull --ff-only'
alias ylf="yadm --no-pager log --format='%w(0,0,7)%C(auto)%h%Creset %<|(56,trunc)%s %C(yellow)(%as)%Creset %C(blue)%<(10,trunc)%al%C(auto)%+d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always | less -R"
alias ywlf="yadm --no-pager log --format='%w(0,0,7)%C(auto)%h%Creset %<|(116,trunc)%s %C(yellow)(%as)%Creset %C(blue)%<(10,trunc)%al%C(auto)%+d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always | less -R"
