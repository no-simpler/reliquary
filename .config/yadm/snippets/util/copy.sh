#!/bin/bash

copy_file_if_different() {
    local source="$1"
    local target="$2"
    local YELLOW='\033[1;33m'
    local NC='\033[0m' # No Color

    # Check if source file exists
    if [ -e "$source" ]; then
        # Check if the target file does not exist or if the files are different
        if [ ! -e "$target" ] || ! cmp -s "$source" "$target"; then
            if [ -e "$target" ]; then
                # Prompt the user to overwrite the existing file
                read -p "Target file $target already exists and is different. Do you want to overwrite it? (y/n) " -n 1 -r
                echo # move to a new line
                if [[ $REPLY =~ ^[Yy]$ ]]; then
                    cp -f "$source" "$target"
                    echo "Copied $source to $target."
                else
                    echo "Skipped copying $source to $target."
                fi
            else
                cp -f "$source" "$target"
                echo "Copied $source to $target."
            fi
        else
            echo "Target $target is already up-to-date."
        fi
    else
        echo -e "${YELLOW}Source file $source does not exist. No action taken.${NC}"
    fi
}
