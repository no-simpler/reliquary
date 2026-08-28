#!/bin/bash

# Docker's CLI plugins are per-user state, not machine state: the app ships them
# inside its bundle and links them into ~/.docker/cli-plugins the first time it
# runs *for that account*. Bedrock requires the full docker API, so an account
# that has never launched the app fails `check-bedrock` on `docker compose` and
# `docker buildx` while the CLI itself is present and on PATH.
#
# bash-3.2-safe; no sudo; idempotent — an existing link of any provenance is
# left alone, because a working plugin is not this snippet's to relocate.

_docker_plugin_dir=""
for _app in /Applications/OrbStack.app/Contents/MacOS/xbin \
    /Applications/Docker.app/Contents/Resources/cli-plugins; do
    if [ -d "$_app" ]; then
        _docker_plugin_dir="$_app"
        break
    fi
done

if [ -n "$_docker_plugin_dir" ]; then
    print_bold -ad "Linking docker CLI plugins..."
    mkdir -p ~/.docker/cli-plugins
    for _plugin in docker-compose docker-buildx; do
        if [ -e "$HOME/.docker/cli-plugins/$_plugin" ]; then
            print_info -ad "$_plugin already linked"
        elif [ -x "$_docker_plugin_dir/$_plugin" ]; then
            ln -s "$_docker_plugin_dir/$_plugin" "$HOME/.docker/cli-plugins/$_plugin" &&
                print_success -ad "Linked $_plugin"
        else
            print_warning -ad "$_plugin not found in $_docker_plugin_dir"
        fi
    done
else
    print_warning -ad "No docker app bundle found. Skipping CLI plugin links."
fi

unset _docker_plugin_dir _app _plugin
