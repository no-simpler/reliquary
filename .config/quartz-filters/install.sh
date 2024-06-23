#!/usr/bin/env bash

set -e
sudo mkdir -p '/Library/PDF Services'
for filter_file in ./*.qfilter; do
    sudo cp "$filter_file" '/Library/PDF Services/'
done
