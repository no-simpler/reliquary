# Checks to run on every shell startup. Must use compatible syntax.

##
## Last upped at
##

# Define the function to check the last update timestamp
check_last_upped() {
    local last_upped_file="$HOME/.last_upped_at"
    local current_time
    local last_upped_time
    local time_diff
    local days_diff

    # ANSI escape codes for formatting
    local bold_arrow='\033[1m==>\033[0m'
    local bold_yellow='\033[1;33m'
    local reset='\033[0m'

    if [ -f "$last_upped_file" ]; then
        # Read only the first line of the file to avoid very large files
        last_upped_time=$(head -n 1 "$last_upped_file")

        # Check if last_upped_time is a valid timestamp (integer)
        if ! [[ $last_upped_time =~ ^[0-9]+$ ]]; then
            last_upped_time=""
        fi
    fi

    if [ -n "$last_upped_time" ]; then
        current_time=$(date +%s)
        time_diff=$((current_time - last_upped_time))
        days_diff=$((time_diff / 86400))

        if [ $days_diff -ge 1 ]; then
            printf "${bold_arrow} ${bold_yellow}Days since last updating the system: %d. Consider running up.${reset}\n" "$days_diff"
        fi
    else
        printf "${bold_arrow} ${bold_yellow}No record of updating the system. Consider running up.${reset}\n"
    fi
}

# Call the function to check the last update
check_last_upped

##
## YADM-encrypted files
##

# Function to check YADM encrypted files
function check_yadm_wrapper() {
    local file_path="$HOME/.pbin/yadm-wrapper"

    if [[ -x "$file_path" ]]; then
        "$file_path" check
    fi
}

# Call the function
check_yadm_wrapper
