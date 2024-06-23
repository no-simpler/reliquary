#!/bin/bash

SOURCE_DIR="$HOME/.config/quartz-filters"
TARGET_DIR="/Library/PDF Services"

# Ensure the target directory exists
if [ ! -d "$TARGET_DIR" ]; then
    echo "Target directory $TARGET_DIR does not exist. Creating it..."
    sudo mkdir -p "$TARGET_DIR"
    if [ $? -ne 0 ]; then
        echo "Failed to create $TARGET_DIR"
        exit 1
    fi
fi

# Loop through .qfilter files in the source directory
for FILE in "$SOURCE_DIR"/*.qfilter; do
    if [ -e "$FILE" ]; then
        FILENAME=$(basename "$FILE")
        TARGET_FILE="$TARGET_DIR/$FILENAME"

        # Check if the target file already exists
        if [ ! -e "$TARGET_FILE" ]; then
            echo "Copying $FILENAME to $TARGET_DIR..."
            sudo cp "$FILE" "$TARGET_FILE"
            if [ $? -eq 0 ]; then
                echo "Successfully copied $FILENAME"
            else
                echo "Failed to copy $FILENAME"
            fi
        else
            echo "$FILENAME already exists in $TARGET_DIR. Skipping."
        fi
    else
        echo "No .qfilter files found in $SOURCE_DIR"
        exit 1
    fi
done
