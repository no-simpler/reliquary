#!/bin/bash

# Plug iterm2 config
ITERM2_PATH="/Applications/iTerm.app"
if [ -d "$ITERM2_PATH" ]; then
    echo "Plugging iTerm2 config..."
    # Specify the preferences directory
    defaults write com.googlecode.iterm2 PrefsCustomFolder -string "~/.config/iterm2/preferences"
    # Tell iTerm2 to use the custom preferences in the directory
    defaults write com.googlecode.iterm2 LoadPrefsFromCustomFolder -bool true
else
    echo "iTerm2 app is not installed. Skipping iTerm2 configuration."
fi
