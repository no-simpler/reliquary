#!/bin/bash

# Define variables
ITERM2_PATH="/Applications/iTerm.app"
PREFS_DIR="$HOME/.config/iterm2/preferences"
PLIST_FILE="$PREFS_DIR/com.googlecode.iterm2.plist"

# Function to check if a defaults key is set to a specific value
is_defaults_key_set_to_value() {
    local domain=$1
    local key=$2
    local value=$3
    local current_value=$(defaults read "$domain" "$key" 2>/dev/null)
    [[ "$current_value" == "$value" ]]
}

# Check if iTerm2 is installed
if [ -d "$ITERM2_PATH" ]; then
    print_bold -ad "Configuring iTerm2 app..."

    # Check if the plist file exists in the preferences directory
    if [ -f "$PLIST_FILE" ]; then
        echo "Found $PLIST_FILE. Proceeding with configuration..."

        # Check and set PrefsCustomFolder if not already set to the correct value
        if is_defaults_key_set_to_value "com.googlecode.iterm2" "PrefsCustomFolder" "$PREFS_DIR"; then
            echo "PrefsCustomFolder is already set to the correct value. Skipping this step."
        else
            echo "Setting PrefsCustomFolder to $PREFS_DIR."
            defaults write com.googlecode.iterm2 PrefsCustomFolder -string "$PREFS_DIR"
        fi

        # Check and set LoadPrefsFromCustomFolder if not already set to the correct value
        if is_defaults_key_set_to_value "com.googlecode.iterm2" "LoadPrefsFromCustomFolder" "1"; then
            echo "LoadPrefsFromCustomFolder is already set to the correct value. Skipping this step."
        else
            echo "Setting LoadPrefsFromCustomFolder to true."
            defaults write com.googlecode.iterm2 LoadPrefsFromCustomFolder -bool true
        fi
    else
        print_warning -ad "Configuration file $PLIST_FILE not found. Skipping iTerm2 configuration."
    fi
else
    print_warning -ad "iTerm2 app is not installed. Skipping iTerm2 configuration."
fi
