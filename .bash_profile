# Startup file for Bash login shells

##
## Commands exclusive to login shells, if any
##

# ... login shell commands go here ...

##
## Source ~/.bashrc (Bash doesn't do this for login shells)
##

# Source .bashrc in home directory
[ -r ~/.bashrc -a -f ~/.bashrc ] && source ~/.bashrc
