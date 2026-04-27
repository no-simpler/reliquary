## Interactive only
status is-interactive; or return

##
## Git aliases
##

abbr --add gf 'git fetch --prune --prune-tags --tags --force'
abbr --add gaa 'git add --all'
abbr --add gaas 'git add --all && git status'
abbr --add grs 'git restore --staged'
abbr --add grc 'git rm --cached -rf .'
abbr --add gs 'git status'
abbr --add gu 'git rm --cached -rf . 2>/dev/null; git add --all; git status'
abbr --add gc 'git commit'
abbr --add gp 'git push origin -u HEAD'
abbr --add gpt 'git push --tags'
abbr --add gff 'git merge --ff-only @{u}'
abbr --add glg 'git log --all --decorate=full --show-signature'

function gbp
    set -l branch (test -n "$argv[1]"; and echo $argv[1]; or echo main)
    git branch --merged $branch | string match -v '\*' | string match -v "  $branch" | xargs -r git branch -d
end

## git-log (80-character-wide semi-oneliners)
alias glfr "git --no-pager log --format='%w(0,0,7)%C(auto)%h%Creset %<|(56,trunc)%s %C(yellow)(%as)%Creset %C(blue)%<(10,trunc)%al%C(auto)%+d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always"
alias glr 'glfr -10'
alias gllr 'glfr -20'
alias glllr 'glfr -40'
alias gllllr 'glfr -80'

## git-log (80-char) piped to less
alias glf 'glfr | less -R'
alias gl 'glfr -10 | less -R'
alias gll 'glfr -20 | less -R'
alias glll 'glfr -40 | less -R'
alias gllll 'glfr -80 | less -R'

## git-log (140-character-wide semi-oneliners)
alias gwlfr "git --no-pager log --format='%w(0,0,7)%C(auto)%h%Creset %<|(116,trunc)%s %C(yellow)(%as)%Creset %C(blue)%<(10,trunc)%al%C(auto)%+d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always"
alias gwlr 'gwlfr -10'
alias gwllr 'gwlfr -20'
alias gwlllr 'gwlfr -40'
alias gwllllr 'gwlfr -80'

## git-log (140-char) piped to less
alias gwlf 'gwlfr | less -R'
alias gwl 'gwlfr -10 | less -R'
alias gwll 'gwlfr -20 | less -R'
alias gwlll 'gwlfr -40 | less -R'
alias gwllll 'gwlfr -80 | less -R'

## git-log (unlimited true oneliners)
alias golfr "git --no-pager log --format='%C(auto)%h%Creset %s %C(yellow)(%as)%Creset %C(blue)%al%C(auto)%d%Creset' --all --graph --abbrev=7 --decorate-refs-exclude='refs/tags/publish-*' --color=always"
alias golr 'golfr -10'
alias gollr 'golfr -20'
alias golllr 'golfr -40'
alias gollllr 'golfr -80'

## git-log (unlimited oneliners) piped to less
alias golf 'golfr | less -R'
alias gol 'golfr -10 | less -R'
alias goll 'golfr -20 | less -R'
alias golll 'golfr -40 | less -R'
alias gollll 'golfr -80 | less -R'
