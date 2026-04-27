# Startup file for Bash login shells

##
## Commands exclusive to login shells, if any
##

# ... login shell commands go here ...

##
## Source ~/.bash_env (always-on env; mirrors what $BASH_ENV gives non-interactive bash)
##

[ -r ~/.bash_env ] && source ~/.bash_env

##
## Source ~/.bashrc (Bash doesn't do this for login shells)
##

# Source .bashrc in home directory
[ -r ~/.bashrc -a -f ~/.bashrc ] && source ~/.bashrc
