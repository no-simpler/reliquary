##
## YADM-encrypted files
##
## The system-update and password-drill nags moved to coop, which draws them
## from ~/.config/coop/sources.d before every prompt rather than once per shell.
## This one stays because it goes away with POSTURE, not into an inbox.
##

function check_yadm_wrapper() {
    local file_path="$HOME/.config/bin/yadm-wrapper"

    if [[ -x "$file_path" ]]; then
        "$file_path" check
    fi
}

# Call the function
check_yadm_wrapper
