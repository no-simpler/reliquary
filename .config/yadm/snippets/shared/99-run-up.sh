#!/bin/bash

print_bold -ad "Running up"

# Store location of up script
UP_PATH="$HOME/.pbin/up"

# Update packages/plugins
if [ -f "$UP_PATH" -a -x "$UP_PATH" ]; then
    "$UP_PATH"
fi
