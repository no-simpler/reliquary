##
## Bash configuration
##

# Disable mail checks:
shopt -u mailwarn
unset MAILCHECK

# Completion
if [ -r "$(brew --prefix)/etc/profile.d/bash_completion.sh" ] ; then
    source "$(brew --prefix)/etc/profile.d/bash_completion.sh"
fi
