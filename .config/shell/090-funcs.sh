##
## General purpose helper functions
##

#>  mcd PATH
#
## Creates path using mkdir -p, and, upon success, cd's into it
#
mcd() {
    mkdir -p -- "$1" && cd -- "$1"
}

# Strip default file-completion from mcd/mmd so zsh doesn't try to complete
# them as plain commands. See: https://github.com/ohmyzsh/ohmyzsh/issues/1895
if typeset -f compdef &>/dev/null; then
    compdef -d mmd
    compdef -d mcd
fi

#>  tca
#
## Tmux: create or attach to session
#
tca() {
    local session_name
    if [ -z "$1" ]; then
        session_name="default"
    else
        session_name="$1"
    fi

    # Check if a session with the given name exists
    tmux has-session -t "$session_name" 2>/dev/null

    if [ $? != 0 ]; then
        # If the session does not exist, create it
        tmux new-session -s "$session_name"
    else
        # If the session exists, attach to it
        tmux attach-session -t "$session_name"
    fi
}
