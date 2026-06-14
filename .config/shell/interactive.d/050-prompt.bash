##
## Prompt: oh-my-posh (mirrors 050-prompt.{zsh,fish}); hand-rolled fallback
##

if command -v oh-my-posh &>/dev/null && [ "$TERM_PROGRAM" != "Apple_Terminal" ]; then
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
