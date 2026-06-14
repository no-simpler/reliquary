##
## $PATH: /usr/local/sbin
##

fish_add_path /usr/local/sbin

##
## $PATH: user-specific bin directories
##

for dir in .config/bin .local/bin
    test -d "$HOME/$dir"; and fish_add_path --append "$HOME/$dir"
end

##
## macOS Homebrew: Modify $PATH and no analytics
##

if test -f /opt/homebrew/bin/brew
    /opt/homebrew/bin/brew shellenv | source
    set -gx HOMEBREW_NO_ANALYTICS 1
end

##
## Force ~/.config/bin ahead of Homebrew on $PATH
##
## So the `yadm` wrapper (the ~/.config/bin/yadm symlink) shadows brew's yadm.
## Must run after the Homebrew block (brew's shellenv reorders PATH on each
## source). Strip any existing entry, then prepend — idempotent.
##

if test -d "$HOME/.config/bin"
    set -gx PATH "$HOME/.config/bin" (string match -v "$HOME/.config/bin" -- $PATH)
end

##
## Ruby gems
##

if command -q gem
    set -l gems_dir (gem env gemdir 2>/dev/null)/bin
    test -d "$gems_dir"; and fish_add_path $gems_dir
end

##
## SDKMAN (via sdkman-for-fish plugin)
##
## Install plugin: fisher install reitzig/sdkman-for-fish@v2.1.0
## The plugin handles SDKMAN initialization for fish.
##

if test -d "$HOME/.sdkman"
    set -gx SDKMAN_DIR "$HOME/.sdkman"
else
    # Erase any inherited SDKMAN_DIR (e.g. from parent zsh) to prevent
    # sdkman-for-fish plugin from warning about a missing installation
    set -e SDKMAN_DIR
end

##
## Google Cloud SDK
##

if command -q brew
    set -l gcloud_path (brew --prefix)/share/google-cloud-sdk/path.fish.inc
    test -f $gcloud_path; and source $gcloud_path
end

##
## Rust
##

test -d "$HOME/.cargo/bin"; and fish_add_path "$HOME/.cargo/bin"

##
## OrbStack
##

set -l orbstack_init "$HOME/.orbstack/shell/init2.fish"
test -r $orbstack_init; and source $orbstack_init 2>/dev/null

##
## Final pass: collapse duplicate $PATH entries (first occurrence wins)
##
## The third-party init blocks above (gcloud, OrbStack) append/prepend their
## dirs with no dedup guard, so a child shell that inherits an already-populated
## $PATH and re-sources this file ends up with doubled entries. Dedup once here
## as the final step. Subtractive only — first occurrences keep their order, so
## ~/.config/bin stays ahead of Homebrew (the yadm-wrapper shadow invariant).
##

set -l deduped
for dir in $PATH
    contains -- $dir $deduped; or set -a deduped $dir
end
set -gx PATH $deduped
