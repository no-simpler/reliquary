#!/bin/bash

export PM_PATH="$HOME/.pbin/pm"

pm() {
    $PM_PATH "$@"
}

print_plain() {
    pm --plain "$@"
}

print_notice() {
    pm --notice "$@"
}

print_info() {
    pm --info "$@"
}

print_success() {
    pm --success "$@"
}

print_warning() {
    pm --warning "$@"
}

print_error() {
    pm --error "$@"
}

print_festive() {
    pm --festive "$@"
}

print_bold() {
    pm --bold "$@"
}
