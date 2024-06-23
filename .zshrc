# Startup file for Zsh interactive shells

##
## Fail-safe against non-interactive shells
##

# Rely on 'interactive' option to be set
[[ -o interactive ]] || return

##
## Set and export the name of the current shell
##

export D__SHELL=zsh

##
## Source the box-specific '.pre.*sh' files
##

[ -f ~/.pre.zsh -a -r ~/.pre.zsh ] && source ~/.pre.zsh
[ -f ~/.pre.sh -a -r ~/.pre.sh ] && source ~/.pre.sh

##
## Source all *.zsh and *.sh files in ~/.runcoms dir, sorted
#. alphanumerically
##

# If 'nullglob' option is unset, set it and remember to restore it after
[[ -o G ]] || {
  set -G
  restore_nullglob=(set +G)
}

## Globbing sorts entries alphanumerically; so the files are sourced in the
#. order of their names.
#
for script_path in ~/.config/shell/*(D); do case $script_path in
  *.zsh | *.sh) source "$script_path" ;;
  esac done
unset script_path

# Restore state of 'nullglob' option
$restore_nullglob
unset restore_nullglob

##
## Source the box-specific '.post.*sh' files
##

[ -f ~/.post.zsh -a -r ~/.post.zsh ] && source ~/.post.zsh
[ -f ~/.post.sh -a -r ~/.post.sh ] && source ~/.post.sh

##
## Graceful exit
##

true
