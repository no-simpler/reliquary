##
## Prompt
##

if false; then
    :
elif command -v oh-my-posh &>/dev/null && [ "$TERM_PROGRAM" != "Apple_Terminal" ]; then
    eval "$(oh-my-posh init zsh --config ~/.config/oh-my-posh/dreamsofautonomy.toml )"
# elif command -v starship &>/dev/null; then
#     eval "$(starship init zsh)"
else
    set_prompt() {
        PROMPT="$([ $? -eq 0 ] && echo '%F{green}' || echo '%F{red}')➜%f%b %B%F{cyan}${PWD##*/}%f%b "
    }
    autoload -Uz add-zsh-hook
    add-zsh-hook precmd set_prompt
fi
