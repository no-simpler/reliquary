#!/bin/bash

print_bold -ad "Running up"

# Store location of up script
UP_PATH="$HOME/.config/bin/up"

# Update packages/plugins
if [ -f "$UP_PATH" ] && [ -x "$UP_PATH" ]; then
    "$UP_PATH"
fi
