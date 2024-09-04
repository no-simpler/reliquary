##
## $PATH: /usr/local/sbin
##

[[ :$PATH: = *":/usr/local/sbin:"* ]] || export PATH="/usr/local/sbin:$PATH"

##
## $PATH: user-specific bin directories
##

for DIR in .bin bin .pbin; do if [ -d "$HOME/$DIR" ]; then
    [[ :$PATH: = *":$HOME/$DIR:"* ]] || export PATH="$PATH:$HOME/$DIR"
fi; done
unset DIR

##
## macOS Homebrew: Modify $PATH and no analytics
##

if [ -f /opt/homebrew/bin/brew ]; then
    eval "$(/opt/homebrew/bin/brew shellenv)"
    export HOMEBREW_NO_ANALYTICS=1
fi

##
## Ruby gems
##

if gem env gemdir &>/dev/null; then
    GEMS_DIR="$(gem env gemdir)/bin"
    if [ -d "$GEMS_DIR" ]; then
        [[ :$PATH: = *":$GEMS_DIR:"* ]] || export PATH="$GEMS_DIR:$PATH"
    fi
    unset GEMS_DIR
fi

##
## Composer globals
##

if [ -d "$HOME/.composer/vendor/bin" ]; then
    [[ :$PATH: = *":$HOME/.composer/vendor/bin:"* ]] ||
        export PATH="$HOME/.composer/vendor/bin:$PATH"
fi

##
## macOS Homebrew: pyenv
##

if command -v pyenv &>/dev/null; then
    export PYENV_ROOT="$HOME/.pyenv"
    if [[ :$PATH: != *":$PYENV_ROOT/bin:"* ]]; then
        export PATH="$PYENV_ROOT/bin:$PATH"
        eval "$(pyenv init --path)"
    fi
    if [[ $PYENV_SHELL != $D__SHELL && $D__SHELL == zsh ]]; then
        eval "$(pyenv init -)"
    fi
fi

##
## pip user-space
##
if [ -d "$HOME/.local/bin" ]; then
    [[ :$PATH: = *":$HOME/.local/bin:"* ]] ||
        export PATH="$HOME/.local/bin:$PATH"
fi

##
## SDKMAN
##

if [ -z "$SDKMAN_DIR" ]; then
    export SDKMAN_DIR="$HOME/.sdkman"
fi
if [ -d "$SDKMAN_DIR" ]; then
    if [ -s "$SDKMAN_DIR/bin/sdkman-init.sh" ]; then
        source "$SDKMAN_DIR/bin/sdkman-init.sh"
    else
        printf >&2 'Missing sdkman init file: %s\n' \
            "$SDKMAN_DIR/bin/sdkman-init.sh"
    fi
fi

##
## macOS Homebrew: Lesspipe
##

if [ -x /usr/local/bin/lesspipe.sh ]; then
    export LESSOPEN='| /usr/local/bin/lesspipe.sh %s'
    export LESS_ADVANCED_PREPROCESSOR=1
    export LESS=' -R '
fi

##
## macOS Catalina & onward: silence warning about default shell change
##

export BASH_SILENCE_DEPRECATION_WARNING=1

##
## Google Cloud SDK
##

# Define the files to source based on the shell type
case "$D__SHELL" in
bash)
    FILES_TO_SOURCE=(
        "$(brew --prefix)/share/google-cloud-sdk/path.bash.inc"
    )
    ;;
zsh)
    FILES_TO_SOURCE=(
        "$(brew --prefix)/share/google-cloud-sdk/path.zsh.inc"
        "$(brew --prefix)/share/google-cloud-sdk/completion.zsh.inc"
    )
    ;;
*)
    FILES_TO_SOURCE=()
    ;;
esac

# Source each file if it exists
for FILE in "${FILES_TO_SOURCE[@]}"; do
    [ -f "$FILE" ] && source "$FILE"
done

##
## Anaconda
##

if [ -d "/opt/homebrew/anaconda3" ]; then
    if [ "$D__SHELL" = "zsh" ]; then
        shell_hook="shell.zsh"
    elif [ "$D__SHELL" = "bash" ]; then
        shell_hook="shell.bash"
    fi

    if [ -n "$shell_hook" ]; then
        __conda_setup="$('/opt/homebrew/anaconda3/bin/conda' "$shell_hook" 'hook' 2>/dev/null)"
        if [ $? -eq 0 ]; then
            eval "$__conda_setup"
        else
            if [ -f "/opt/homebrew/anaconda3/etc/profile.d/conda.sh" ]; then
                . "/opt/homebrew/anaconda3/etc/profile.d/conda.sh"
            else
                export PATH="/opt/homebrew/anaconda3/bin:$PATH"
            fi
        fi
        unset __conda_setup
    fi
fi

##
## rustup
##

if [ -d "/opt/homebrew/opt/rustup/bin" ]; then
    export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
fi
