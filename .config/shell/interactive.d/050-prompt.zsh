##
## Prompt
##

## ske: publish the open Touch-ID window into $SKE_WINDOW for the oh-my-posh
## `text` segment to render. Mirrors 050-prompt.{bash,fish}.
##
## Why an env var and not an oh-my-posh `command` segment: that segment type was
## REMOVED upstream (v29's schema has no such type; it silently renders nothing).
## A text segment reading .Env is the supported way to surface external state.
##
## Cheap by construction: the common case (no window) is a single [[ -S ]] test,
## and only pays for `ske prompt` once a ske agent socket actually exists. A ske
## that is not on PATH renders nothing, which is what pure decoration must do.
## Both hooks below save and restore $?. oh-my-posh reads the exit status at the
## top of its own precmd, which runs after these — so a hook that returns its own
## status makes every prompt render as a success, whatever the last command did.
_ske_window() {
    local last=$?
    if [[ -S "${SKE_STATE:-$HOME/.local/state/ske}/agent.sock" ]]; then
        export SKE_WINDOW="$(ske prompt 2>/dev/null)"
    else
        unset SKE_WINDOW
    fi
    return $last
}

## coop: draw the card when the outstanding set has moved, and publish the count
## into $COOP_BADGE for the oh-my-posh `text` segment. One exec serves both: the
## card goes to stdout, the count to a file this reads with a builtin.
_coop_precmd() {
    local last=$?
    command coop tick 2>/dev/null
    local badge="${COOP_ROOT:-$HOME/.local/state/coop}/badge"
    if [[ -r $badge ]] && [[ -s $badge ]]; then
        export COOP_BADGE="$(<$badge)"
    else
        unset COOP_BADGE
    fi
    return $last
}

## One identity per shell, so a second terminal is shown the card too and
## neither repeats it.
export COOP_SESSION="$$-$RANDOM"

if false; then
    :
elif command -v oh-my-posh &>/dev/null && [ "$TERM_PROGRAM" != "Apple_Terminal" ]; then
    ## Registered BEFORE oh-my-posh's own precmd, so $SKE_WINDOW is fresh by the
    ## time oh-my-posh renders; registering after would show a one-prompt-stale value.
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd _coop_precmd
    add-zsh-hook precmd _ske_window
    eval "$(oh-my-posh init zsh --config ~/.config/oh-my-posh/dreamsofautonomy.toml )"
else
    set_prompt() {
        PROMPT="$([ $? -eq 0 ] && echo '%F{green}' || echo '%F{red}')➜%f%b %B%F{cyan}${PWD##*/}%f%b "
    }
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd set_prompt
fi
