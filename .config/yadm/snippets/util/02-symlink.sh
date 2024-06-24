#!/bin/bash

create_symlink() {
    local source_file=$1
    local target_file=$2

    # Check if the source file exists
    if [[ -f "$source_file" ]]; then
        echo "Source file found: $source_file"

        # Check if the parent directory of the target file exists
        local target_dir
        target_dir=$(dirname "$target_file")
        if [[ ! -d "$target_dir" ]]; then
            echo "Target directory does not exist. Creating: $target_dir"
            mkdir -p "$target_dir"
        fi

        # Check if the target file already exists
        if [[ -e "$target_file" ]]; then
            if [[ -L "$target_file" && "$(readlink "$target_file")" == "$source_file" ]]; then
                echo "Target file is already a symlink to the source file: $target_file"
                return 0
            fi

            print_warning "Target file already exists: $target_file"

            # Prompt the user to overwrite the existing file
            read -p "Do you want to overwrite the existing file? (y/n) " -n 1 -r
            echo # move to a new line
            if [[ $REPLY =~ ^[Yy]$ ]]; then
                echo "Overwriting existing file..."
                ln -sf "$source_file" "$target_file"
                echo "Linked $source_file to $target_file"
            else
                print_warning "Skipped linking $source_file to $target_file"
            fi
        else
            # Create the symlink if the target file does not exist
            ln -s "$source_file" "$target_file"
            echo "Linked $source_file to $target_file"
        fi
    else
        print_warning "Source file $source_file does not exist. No action taken."
    fi
}
