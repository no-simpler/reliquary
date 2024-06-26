# Startup file for Bash interactive shells

##
## Fail-safe against non-interactive shells
##

# Rely on $PS1 to be empty in a non-interactive shell
[ -n "$PS1" ] || return

##
## Disable keyboard input while runcoms are loaded
##

stty -icanon -echo

##
## Set and export the name of the current shell
##

export D__SHELL=bash

##
## Source the box-specific '.pre.*sh' files
##

[ -f ~/.pre.bash -a -r ~/.pre.bash ] && source ~/.pre.bash
[ -f ~/.pre.sh -a -r ~/.pre.sh ] && source ~/.pre.sh

##
## Source all *.bash and *.sh files in ~/.runcoms dir, sorted
#. alphanumerically
##

# Save current state of 'dotglob' and 'nullglob' options
restore_opts=("$(shopt -p dotglob)" "$(shopt -p nullglob)")

# Set both 'dotglob' and 'nullglob' options
shopt -s dotglob nullglob

## Globbing sorts entries alphanumerically; so the files are sourced in the
#. order of their names.
#
for script_path in ~/.config/shell/*; do case $script_path in
  *.bash | *.sh) source "$script_path" ;;
  esac done
unset script_path

# Restore state of 'dotglob' and 'nullglob' options
for cmd in "${restore_opts[@]}"; do $cmd; done
unset cmd restore_opts

##
## Source the box-specific '.post.*sh' files
##

[ -f ~/.post.bash -a -r ~/.post.bash ] && source ~/.post.bash
[ -f ~/.post.sh -a -r ~/.post.sh ] && source ~/.post.sh

##
## Re-enable keyboard input
##

stty icanon echo

##
## Graceful exit
##

true
