## Interactive only
status is-interactive; or return

##
## yadm
##

alias yadm ~/.config/bin/yadm-wrapper
abbr --add ya 'yadm add'
abbr --add yf 'yadm fetch --prune --prune-tags --tags --force'
abbr --add yrs 'yadm restore --staged'
abbr --add ys 'yadm status'
abbr --add yc 'yadm commit'
abbr --add yp 'yadm push'
abbr --add ypt 'yadm push --tags'
abbr --add yff 'yadm merge --ff-only @{u}'
abbr --add ypff 'yadm pull --ff-only'
alias ylf "yadm --no-pager log --format='%w(0,0,7)%C(auto)%h%Creset %<|(56,trunc)%s %C(yellow)(%as)%Creset %C(blue)%<(10,trunc)%al%C(auto)%+d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always | less -R"
alias ywlf "yadm --no-pager log --format='%w(0,0,7)%C(auto)%h%Creset %<|(116,trunc)%s %C(yellow)(%as)%Creset %C(blue)%<(10,trunc)%al%C(auto)%+d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always | less -R"
