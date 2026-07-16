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
## and only pays for ske-prompt (~10ms) once a ske agent socket actually exists.
_ske_window() {
    if [[ -S "${SKE_STATE:-$HOME/.local/state/ske}/agent.sock" ]]; then
        export SKE_WINDOW="$($HOME/.config/bin/ske-prompt 2>/dev/null)"
    else
        unset SKE_WINDOW
    fi
}

if false; then
    :
elif command -v oh-my-posh &>/dev/null && [ "$TERM_PROGRAM" != "Apple_Terminal" ]; then
    ## Registered BEFORE oh-my-posh's own precmd, so $SKE_WINDOW is fresh by the
    ## time oh-my-posh renders; registering after would show a one-prompt-stale value.
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd _ske_window
    eval "$(oh-my-posh init zsh --config ~/.config/oh-my-posh/dreamsofautonomy.toml )"
else
    set_prompt() {
        PROMPT="$([ $? -eq 0 ] && echo '%F{green}' || echo '%F{red}')➜%f%b %B%F{cyan}${PWD##*/}%f%b "
    }
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd set_prompt
fi
