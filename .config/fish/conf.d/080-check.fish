## Interactive only
status is-interactive; or return

##
## Last upped at
##

function check_last_upped
    set -l last_upped_file "$HOME/.local/state/up/last_upped_at"
    set -l bold_arrow '\033[1m==>\033[0m'
    set -l bold_yellow '\033[1;33m'
    set -l reset '\033[0m'

    set -l last_upped_time ""

    if test -f $last_upped_file
        set last_upped_time (head -n 1 $last_upped_file)

        # Validate: must be numeric
        if not string match -qr '^\d+$' $last_upped_time
            set last_upped_time ""
        end
    end

    if test -n "$last_upped_time"
        set -l current_time (date +%s)
        set -l time_diff (math $current_time - $last_upped_time)
        set -l days_diff (math --scale=0 $time_diff / 86400)

        if test $days_diff -ge 1
            printf "$bold_arrow $bold_yellow""Days since last updating the system: %d. Consider running up.$reset\n" $days_diff
        end
    else
        printf "$bold_arrow $bold_yellow""No record of updating the system. Consider running up.$reset\n"
    end
end

# Call the function
check_last_upped

##
## YADM-encrypted files
##

function check_yadm_wrapper
    set -l file_path "$HOME/.config/bin/yadm-wrapper"

    if test -x $file_path
        $file_path check
    end
end

# Call the function
check_yadm_wrapper
