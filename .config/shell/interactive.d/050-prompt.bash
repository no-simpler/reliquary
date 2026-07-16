##
## Prompt: oh-my-posh (mirrors 050-prompt.{zsh,fish}); hand-rolled fallback
##

## ske: publish the open Touch-ID window into $SKE_WINDOW for the oh-my-posh
## `text` segment to render. Mirrors 050-prompt.{zsh,fish}. See the zsh file for
## why this is an env var rather than an oh-my-posh `command` segment (that type
## was removed upstream and silently renders nothing).
_ske_window() {
    if [ -S "${SKE_STATE:-$HOME/.local/state/ske}/agent.sock" ]; then
        export SKE_WINDOW="$($HOME/.config/bin/ske-prompt 2>/dev/null)"
    else
        unset SKE_WINDOW
    fi
}

if command -v oh-my-posh &>/dev/null && [ "$TERM_PROGRAM" != "Apple_Terminal" ]; then
    ## Prepended to PROMPT_COMMAND so it runs before oh-my-posh's own hook, which
    ## oh-my-posh appends during init below.
    PROMPT_COMMAND="_ske_window${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
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
