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

#>  cdf
#
## Changes into the directory that appears first in the sorted order.
#
cdf() {
    cd $(find . -type d | sort | head -1)
}

#>  cdl
#
## Changes into the directory that appears last in the sorted order.
#
cdl() {
    cd $(find . -type d | sort | tail -1)
}

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

#>  deidea
#
## Removes Intellij IDEA files and directories from all subdirectories of the
#. current one
#
deidea() {
    if [ "$1" = '-f' ]; then
        local tmp="$(mktemp)"
        find . -name '*.iml' -type f | tee -a $tmp | xargs rm -f
        find . -name '.idea' -type d | tee -a $tmp | xargs rm -rf
        cat $tmp
        rm -f $tmp
    else
        find . -name '*.iml' -type f
        find . -name '.idea' -type d
    fi
}
