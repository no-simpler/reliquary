#!/bin/bash

# Plug Choosy configuration
if [[ -d "/Applications/Choosy.app" ]]; then
    print_bold -ad "Configuring Choosy app..."

    # Define the source and target paths for the Choosy configuration file
    CHOOSY_SOURCE_CONFIG="$HOME/.config/choosy/behaviours.plist"
    CHOOSY_TARGET_CONFIG="$HOME/Library/Application Support/Choosy/behaviours.plist"

    # Call the function to copy the config file if different
    copy_file_if_different "$CHOOSY_SOURCE_CONFIG" "$CHOOSY_TARGET_CONFIG"
else
    print_warning -ad "Choosy app is not installed. Skipping Choosy configuration."
fi
