##
## Zsh configuration
##

# Enable extended glob
setopt extended_glob

# Automatically change to a directory by typing its name without the 'cd' command
setopt auto_cd

# Use 'cd' as 'pushd', pushing the current directory onto the stack before changing
setopt auto_pushd

# Prevent duplicate directories from being added to the directory stack
setopt pushd_ignore_dups

# Modify 'pushd' to rotate the directory stack instead of swapping directories.
setopt pushdminus

# Skip verification of insecure directories
ZSH_DISABLE_COMPFIX=true
