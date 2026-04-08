##
## Locale fix
##

set -gx LC_ALL en_US.UTF-8
set -gx LANG en_US.UTF-8

##
## GPG-Agent fix
##

if command -q gpg; or command -q gnupg
    set -gx GPG_TTY (tty)
end
