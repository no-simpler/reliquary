##
## Fish shell configuration
## Mirrors the sourcing pattern from .zshrc — loads *.fish from ~/.config/shell/
##

## Fail-safe against non-interactive shells
status is-interactive; or return

## Set and export the name of the current shell
set -gx D__SHELL fish

## Source the box-specific '.pre.*' files
for f in ~/.pre.fish ~/.pre.sh
    test -f $f; and test -r $f; and source $f
end

## Source all *.fish files in ~/.config/shell/, sorted alphanumerically
for script_path in ~/.config/shell/*.fish
    source $script_path
end

## Source the box-specific '.post.*' files
for f in ~/.post.fish
    test -f $f; and test -r $f; and source $f
end

## Graceful exit
true
