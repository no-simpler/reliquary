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
