#!/bin/bash

# Define and export color and boldness codes
export RESET="\033[0m"
export BOLD="\033[1m"

# Basic colors
export BLACK="\033[30m"
export RED="\033[31m"
export GREEN="\033[32m"
export YELLOW="\033[33m"
export BLUE="\033[34m"
export MAGENTA="\033[35m"
export CYAN="\033[36m"
export WHITE="\033[37m"

# Bright colors
export BRIGHT_BLACK="\033[90m"
export BRIGHT_RED="\033[91m"
export BRIGHT_GREEN="\033[92m"
export BRIGHT_YELLOW="\033[93m"
export BRIGHT_BLUE="\033[94m"
export BRIGHT_MAGENTA="\033[95m"
export BRIGHT_CYAN="\033[96m"
export BRIGHT_WHITE="\033[97m"

# Background colors
export BG_BLACK="\033[40m"
export BG_RED="\033[41m"
export BG_GREEN="\033[42m"
export BG_YELLOW="\033[43m"
export BG_BLUE="\033[44m"
export BG_MAGENTA="\033[45m"
export BG_CYAN="\033[46m"
export BG_WHITE="\033[47m"

# Bright background colors
export BG_BRIGHT_BLACK="\033[100m"
export BG_BRIGHT_RED="\033[101m"
export BG_BRIGHT_GREEN="\033[102m"
export BG_BRIGHT_YELLOW="\033[103m"
export BG_BRIGHT_BLUE="\033[104m"
export BG_BRIGHT_MAGENTA="\033[105m"
export BG_BRIGHT_CYAN="\033[106m"
export BG_BRIGHT_WHITE="\033[107m"

print_message() {
    local symbols1=("⠄" "⠔" "⠜" "⢜" "⢝" "⢟" "⢿" "⣿" "⡿" "⡾" "⠾" "⠶")
    local symbols2=("⠈" "⡈" "⡊" "⡚" "⡺" "⡾" "⣾" "⣿" "⣾" "⢾" "⠾" "⠶")
    local symbols3=("⠁" "⠅" "⢅" "⣅" "⣇" "⣗" "⣟" "⣿" "⣿" "⣷" "⣷" "⡷")
    local animation_step=0.06
    local freeze_delay=0.5

    local delay=false
    local animate=false
    local type="plain"
    local message

    while (( "$#" )); do
        case "$1" in
            -d|--delay)
                delay=true
                shift
                ;;
            -a|--animate)
                animate=true
                shift
                ;;
            -i|--info)
                type="info"
                shift
                ;;
            -w|--warning)
                type="warning"
                shift
                ;;
            -e|--error)
                type="error"
                shift
                ;;
            -n|--notice)
                type="notice"
                shift
                ;;
            -b|--bold)
                type="bold"
                shift
                ;;
            -p|--plain)
                type="plain"
                shift
                ;;
            -*)
                local opt
                for (( i=1; i<${#1}; i++ )); do
                    opt="${1:i:1}"
                    case "$opt" in
                        d)
                            delay=true
                            ;;
                        a)
                            animate=true
                            ;;
                        i)
                            type="info"
                            ;;
                        w)
                            type="warning"
                            ;;
                        e)
                            type="error"
                            ;;
                        n)
                            type="notice"
                            ;;
                        b)
                            type="bold"
                            ;;
                        p)
                            type="plain"
                            ;;
                        *)
                            echo "Unknown option: -$opt"
                            return 1
                            ;;
                    esac
                done
                shift
                ;;
            *)
                message="$*"
                break
                ;;
        esac
    done

    local color_code
    case "$type" in
        info)
            color_code="GREEN"
            ;;
        warning)
            color_code="YELLOW"
            ;;
        error)
            color_code="RED"
            ;;
        notice)
            color_code="CYAN"
            ;;
        bold)
            color_code=""
            ;;
        plain)
            color_code=""
            ;;
        *)
            color_code="WHITE"
            ;;
    esac

    if $animate; then
        for ((j=11; j>=0; j--)); do
            if [ "$type" = "plain" ]; then
                printf "\r${!color_code}%s%s%s %s${RESET}" "${symbols1[j]}" "${symbols2[j]}" "${symbols3[j]}" "$message"
            else
                printf "\r${!color_code}%s%s%s ${BOLD}%s${RESET}" "${symbols1[j]}" "${symbols2[j]}" "${symbols3[j]}" "$message"
            fi
            sleep "$animation_step"
        done

        for ((j=0; j<12; j++)); do
            if [ "$type" = "plain" ]; then
                printf "\r${!color_code}%s%s%s %s${RESET}" "${symbols1[j]}" "${symbols2[j]}" "${symbols3[j]}" "$message"
            else
                printf "\r${!color_code}%s%s%s ${BOLD}%s${RESET}" "${symbols1[j]}" "${symbols2[j]}" "${symbols3[j]}" "$message"
            fi
            sleep "$animation_step"
        done
    fi

    if [ "$type" = "plain" ]; then
        printf "\r${!color_code}%s%s%s %s${RESET}" "${symbols1[11]}" "${symbols2[11]}" "${symbols3[11]}" "$message"
    else
        printf "\r${!color_code}%s%s%s ${BOLD}%s${RESET}" "${symbols1[11]}" "${symbols2[11]}" "${symbols3[11]}" "$message"
    fi

    if $delay; then
        sleep "$freeze_delay"
    fi

    printf "\n"
}

# Print functions for specific message types
print_info() {
    print_message --info "$@"
}

print_warning() {
    print_message --warning "$@"
}

print_error() {
    print_message --error "$@"
}

print_notice() {
    print_message --notice "$@"
}

print_bold() {
    print_message --bold "$@"
}

print_plain() {
    print_message --plain "$@"
}
