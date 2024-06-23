# Universal shell bug fixes. Must use compatible syntax.

##
## Locale fix
##

## Known problems cured by this fix include:
#.  * macOS: pipenv bug 'unknown locale'
#.    https://github.com/pypa/pipenv/issues/187
#

export LC_ALL=en_US.UTF-8
export LANG=en_US.UTF-8

##
## GPG-Agent fix
## https://www.gnupg.org/documentation/manuals/gnupg/Invoking-GPG_002dAGENT.html
##

if gpg --version &>/dev/null || gnupg --version &>/dev/null; then
    export GPG_TTY=$(tty)
fi

##
## zsh-completions bug that messes with 'mmd' and 'mcd' commands
## https://github.com/ohmyzsh/ohmyzsh/issues/1895#issuecomment-34887821
##

if typeset -f compdef &>/dev/null; then
    compdef -d mmd
    compdef -d mcd
fi
