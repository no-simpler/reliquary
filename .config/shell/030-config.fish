##
## Fish configuration
##
## Most zsh setopt equivalents are fish defaults:
##   - Shared history with deduplication: built-in
##   - Extended globbing: built-in (** works natively)
##   - Ignore commands starting with space: built-in (fish 4.x)
##   - Emacs keybindings: default mode
##   - Ctrl+P/N history search: works in emacs mode
##

## Suppress the default "Welcome to fish" greeting
set -g fish_greeting

## History: ignore commands that start with a space (fish 4.x+)
set -g fish_history_ignore_regex '^ '
