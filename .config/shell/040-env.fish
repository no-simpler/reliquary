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
## Ruby gems
##

if command -q gem
    set -l gems_dir (gem env gemdir 2>/dev/null)/bin
    test -d "$gems_dir"; and fish_add_path $gems_dir
end

##
## Composer globals
##

test -d "$HOME/.composer/vendor/bin"; and fish_add_path "$HOME/.composer/vendor/bin"

##
## macOS Homebrew: pyenv
##

if command -q pyenv
    set -gx PYENV_ROOT "$HOME/.pyenv"
    fish_add_path "$PYENV_ROOT/bin"
    pyenv init - fish | source
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
## macOS Homebrew: Lesspipe
##

if test -x /usr/local/bin/lesspipe.sh
    set -gx LESSOPEN '| /usr/local/bin/lesspipe.sh %s'
    set -gx LESS_ADVANCED_PREPROCESSOR 1
    set -gx LESS ' -R '
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

test -d /opt/homebrew/opt/rustup/bin; and fish_add_path /opt/homebrew/opt/rustup/bin
test -f "$HOME/.cargo/env.fish"; and source "$HOME/.cargo/env.fish"
# Fallback: cargo bin dir directly
test -d "$HOME/.cargo/bin"; and fish_add_path "$HOME/.cargo/bin"

##
## OrbStack
##

set -l orbstack_init "$HOME/.orbstack/shell/init.fish"
test -r $orbstack_init; and source $orbstack_init 2>/dev/null
