##
## Prompt
##

set_prompt() {
    local last_command_status=$?
    local dir_name="${PWD##*/}"

    if [ "$dir_name" = "" ]; then
        dir_name="/"
    fi

    PS1="$([ $last_command_status -eq 0 ] && echo '\[\e[1;32m\]' || echo '\[\e[1;31m\]')>\[\e[0m\] \[\e[1;36m\]${dir_name}\[\e[0m\] "
}
PROMPT_COMMAND=set_prompt
