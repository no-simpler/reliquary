##
## General purpose helper functions
##

#>  mcd PATH
#
## Creates path using mkdir -p, and, upon success, cd's into it
#
function mcd
    mkdir -p -- $argv[1]; and cd -- $argv[1]
end

#>  tca [SESSION_NAME]
#
## Tmux: create or attach to session
#
function tca
    set -l session_name default
    if test (count $argv) -ge 1; and test -n "$argv[1]"
        set session_name $argv[1]
    end

    if tmux has-session -t $session_name 2>/dev/null
        tmux attach-session -t $session_name
    else
        tmux new-session -s $session_name
    end
end
