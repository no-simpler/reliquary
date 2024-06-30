##
## Prompt
##

set_prompt() {
    PROMPT="$([ $? -eq 0 ] && echo '%F{green}' || echo '%F{red}')➜%f%b %B%F{cyan}${PWD##*/}%f%b "
}
autoload -Uz add-zsh-hook
add-zsh-hook precmd set_prompt
