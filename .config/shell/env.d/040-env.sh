##
## $PATH: /usr/local/sbin
##

[[ :$PATH: = *":/usr/local/sbin:"* ]] || export PATH="/usr/local/sbin:$PATH"

##
## $PATH: user-specific bin directories
##   ~/.config/bin  — YADM-tracked personal scripts
##   ~/.local/bin   — externally-managed (meta-projects: halo, bb); see ~/.config/CLAUDE.md
##

for DIR in .config/bin .local/bin; do if [ -d "$HOME/$DIR" ]; then
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
## PATH priority — forcing ~/.config/bin ahead of Homebrew (so the `yadm`
## wrapper shadows brew's yadm) — lives in env.d/999-path.sh, the highest-
## numbered file. It MUST run after the gcloud/cargo/OrbStack/gems prepends
## below (and any pre-polluted inherited $PATH), so it can't sit here.
##

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
## Rust
##

if [ -f "$HOME/.cargo/env" ]; then
    . "$HOME/.cargo/env"
fi

##
## OrbStack
##

if [ -n "$D__SHELL" ]; then
    ORBSTACK_INIT="$HOME/.orbstack/shell/init.$D__SHELL"

    if [ -r "$ORBSTACK_INIT" ]; then
        . "$ORBSTACK_INIT" 2>/dev/null
    fi

    unset ORBSTACK_INIT
fi

##
## Final pass: collapse duplicate $PATH entries (first occurrence wins)
##
## The third-party init blocks above (gcloud, OrbStack) append/prepend their
## dirs with no dedup guard, so a child shell that inherits an already-populated
## $PATH and re-sources this file ends up with doubled entries. Rather than
## patch each upstream snippet, dedup once here as the final step. Subtractive
## only — first occurrences keep their order, so ~/.config/bin stays ahead of
## Homebrew (the yadm-wrapper shadow invariant holds).
##

if [ "$D__SHELL" = "zsh" ]; then
    # zsh doesn't word-split unquoted $PATH, so the POSIX loop below would be a
    # no-op here; use the native uniquing of the tied `path` array instead
    # (also keeps first occurrence).
    typeset -U path PATH
elif [ -n "$PATH" ]; then
    _dedup=""
    _OIFS=$IFS
    IFS=:
    for _dir in $PATH; do
        case ":$_dedup:" in
            *":$_dir:"*) ;;
            *) _dedup="${_dedup:+$_dedup:}$_dir" ;;
        esac
    done
    IFS=$_OIFS
    export PATH="$_dedup"
    unset _dedup _OIFS _dir
fi
