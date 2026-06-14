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
## Anaconda
##

if test -d /opt/homebrew/anaconda3
    set -l conda_setup (/opt/homebrew/anaconda3/bin/conda shell.fish hook 2>/dev/null)
    if test $status -eq 0
        eval $conda_setup
    else if test -f /opt/homebrew/anaconda3/etc/fish/conf.d/conda.fish
        source /opt/homebrew/anaconda3/etc/fish/conf.d/conda.fish
    else
        fish_add_path /opt/homebrew/anaconda3/bin
    end
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
