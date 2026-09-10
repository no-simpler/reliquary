##
## Prompt: oh-my-posh (mirrors 050-prompt.{zsh,fish}); hand-rolled fallback
##

## ske: publish the open Touch-ID window into $SKE_WINDOW for the oh-my-posh
## `text` segment to render. Mirrors 050-prompt.{zsh,fish}. See the zsh file for
## why this is an env var rather than an oh-my-posh `command` segment (that type
## was removed upstream and silently renders nothing).
## Both hooks below save and restore $?. oh-my-posh reads the exit status at the
## top of its own hook, which runs after these — so a hook that returns its own
## status makes every prompt render as a success, whatever the last command did.
_ske_window() {
    local last=$?
    if [ -S "${SKE_STATE:-$HOME/.local/state/ske}/agent.sock" ]; then
        SKE_WINDOW="$(ske prompt 2>/dev/null)"
        export SKE_WINDOW
    else
        unset SKE_WINDOW
    fi
    return $last
}

## coop: draw the card when the outstanding set has moved, and publish the count
## into $COOP_BADGE for the oh-my-posh `text` segment. Mirrors 050-prompt.{zsh,fish}.
_coop_precmd() {
    local last=$?
    command coop tick 2>/dev/null
    local badge="${COOP_ROOT:-$HOME/.local/state/coop}/badge"
    if [ -r "$badge" ] && [ -s "$badge" ]; then
        COOP_BADGE="$(<"$badge")"
        export COOP_BADGE
    else
        unset COOP_BADGE
    fi
    return $last
}

## One identity per shell, so a second terminal is shown the card too and
## neither repeats it.
export COOP_SESSION="$$-$RANDOM"

if command -v oh-my-posh &>/dev/null && [ "$TERM_PROGRAM" != "Apple_Terminal" ]; then
    ## Prepended to PROMPT_COMMAND so it runs before oh-my-posh's own hook, which
    ## oh-my-posh appends during init below.
    ## Not spelled as a `source ...` entry: oh-my-posh's _omp_install_hook drops
    ## those when it rewrites PROMPT_COMMAND into an array.
    PROMPT_COMMAND="_coop_precmd;_ske_window${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
    eval "$(oh-my-posh init bash --config ~/.config/oh-my-posh/dreamsofautonomy.toml)"
else
    ## Fallback: minimal prompt with color-coded exit status
    set_prompt() {
        local last_command_status=$?
        local dir_name="${PWD##*/}"

        if [ "$dir_name" = "" ]; then
            dir_name="/"
        fi

        PS1="$([ $last_command_status -eq 0 ] && echo '\[\e[1;32m\]' || echo '\[\e[1;31m\]')>\[\e[0m\] \[\e[1;36m\]${dir_name}\[\e[0m\] "
    }
    PROMPT_COMMAND=set_prompt
fi
