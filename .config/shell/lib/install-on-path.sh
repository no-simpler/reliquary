#!/usr/bin/env bash
#
# install-on-path: install an executable into ~/.local/bin/ under a chosen name.
#
# Usage (source from a meta-repo or aspect publish.sh):
#
#   META_NAME=halo source "$HOME/.config/shell/lib/install-on-path.sh"
#   install_on_path /path/to/built-binary chosen-name
#
# A meta-repo or any of its aspects may publish zero, one, or many PATH
# artifacts; call install_on_path once per artifact.  The <name> is chosen
# freely and need not match the aspect or meta-repo name.
#
# Convention:
#
#   - ~/.local/bin/ is the canonical PATH lane for meta-managed binaries.
#     It is on $PATH via reliquary's env.d/040-env.sh and is not YADM-tracked.
#   - Each meta-repo owns a registry file ~/.local/bin/.<META_NAME>-managed
#     listing the filenames it manages.  The helper refuses to overwrite any
#     file not in the registry, so foreign or hand-placed files are safe.
#
# This helper is the rails for the simple, mundane case: copy a built file
# to a chosen name on PATH.  Advanced cases (template substitution, self-
# update, embedded provenance) may sidestep the helper; the owning meta-repo
# documents the convention in its CLAUDE.md.

if [[ -z "${META_NAME:-}" ]]; then
    printf 'install-on-path: META_NAME must be set before sourcing\n' >&2
    return 1 2>/dev/null || exit 1
fi

_INSTALL_ON_PATH_DIR="$HOME/.local/bin"
_INSTALL_ON_PATH_REGISTRY="$_INSTALL_ON_PATH_DIR/.${META_NAME}-managed"

_install_on_path_registry_contains() {
    local name="$1"
    [[ -f "$_INSTALL_ON_PATH_REGISTRY" ]] || return 1
    grep -qxF "$name" "$_INSTALL_ON_PATH_REGISTRY"
}

_install_on_path_registry_add() {
    local name="$1"
    if ! _install_on_path_registry_contains "$name"; then
        mkdir -p "$_INSTALL_ON_PATH_DIR"
        printf '%s\n' "$name" >> "$_INSTALL_ON_PATH_REGISTRY"
    fi
}

install_on_path() {
    local src="${1:-}" name="${2:-}"

    if [[ -z "$src" || -z "$name" ]]; then
        printf 'install_on_path: usage: install_on_path <source-file> <target-name>\n' >&2
        return 1
    fi

    if [[ "$name" == */* || "$name" == .* ]]; then
        printf 'install_on_path: target-name must be a plain filename (no slashes, no leading dot): %s\n' "$name" >&2
        return 1
    fi

    if [[ ! -f "$src" ]]; then
        printf 'install_on_path: source not found: %s\n' "$src" >&2
        return 1
    fi

    if [[ ":$PATH:" != *":$_INSTALL_ON_PATH_DIR:"* ]]; then
        printf 'install_on_path: %s is not on $PATH\n' "$_INSTALL_ON_PATH_DIR" >&2
        printf '    Reliquary adds it via env.d/040-env.sh — see ~/.config/CLAUDE.md.\n' >&2
        return 1
    fi

    mkdir -p "$_INSTALL_ON_PATH_DIR"

    local target="$_INSTALL_ON_PATH_DIR/$name"

    if [[ -e "$target" ]] && ! _install_on_path_registry_contains "$name"; then
        printf 'install_on_path: %s exists but is not in %s — refusing to overwrite.\n' "$target" "$_INSTALL_ON_PATH_REGISTRY" >&2
        printf '    If the file is genuinely yours and you want %s to manage it,\n' "$META_NAME" >&2
        printf '    adopt it by appending its name to the registry once:\n' >&2
        printf '        echo %s >> %s\n' "$name" "$_INSTALL_ON_PATH_REGISTRY" >&2
        printf '    Otherwise remove the existing file first:\n' >&2
        printf '        rm %s\n' "$target" >&2
        return 1
    fi

    cp "$src" "$target"
    chmod +x "$target"

    _install_on_path_registry_add "$name"

    printf 'Installed %s to %s\n' "$name" "$target"
}
