#!/bin/bash

print_bold -ad "Applying .DS_Store write policy"

# Stop Finder from writing .DS_Store files on network shares (SMB/AFP) and USB
# volumes. NOTE: macOS offers no supported toggle for LOCAL internal-disk
# volumes, so this does NOT cover ~/Developer or the XDG depots — those rely on
# the global git excludes (~/.config/git/ignore) plus periodic cleanup.

# Set a com.apple.desktopservices bool key only if it's not already that value.
set_dsds_bool() {
    local key="$1" want="$2"
    local current
    current="$(defaults read com.apple.desktopservices "$key" 2>/dev/null)"
    if [ "$current" = "$want" ]; then
        print_info "com.apple.desktopservices $key already set to $want. Skipping."
        return 1
    fi
    defaults write com.apple.desktopservices "$key" -bool true
    print_success "Set com.apple.desktopservices $key to true."
    return 0
}

changed=1
set_dsds_bool DSDontWriteNetworkStores 1 && changed=0
set_dsds_bool DSDontWriteUSBStores 1 && changed=0

# Relaunch Finder so the policy takes effect immediately (only if we changed it).
if [ "$changed" -eq 0 ]; then
    print_info "Relaunching Finder to apply the new policy..."
    killall Finder 2>/dev/null || true
fi
