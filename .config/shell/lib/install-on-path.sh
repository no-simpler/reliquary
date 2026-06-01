#!/usr/bin/env bash
#
# install-on-path: install an executable into ~/.local/bin/ under a chosen name.
#
# Public API: install_on_path + a single shared registry.
# Stable — do not break without coordinated republish of all callers:
#   - ~/.config/reliquary/lib/relic.sh    (Reliquary in-house relics)
#   - all known external relics — currently bb and halo
#     (see ~/.config/reliquary/GRADUATION.md "Known external relics")
#
# This file's location (~/.config/shell/lib/) is grandfathered. A deferred
# change plans to hoist it to ~/.config/reliquary/lib/ alongside the rest of
# relic infrastructure — see ~/.config/reliquary/design/.
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
#   - A single registry file ~/.local/bin/.reliquary-managed lists every
#     managed name, one per line, with an OPTIONAL owner column:
#         <name>[<TAB><owner>]
#     Blank lines and '#' comments are ignored; membership is keyed on the
#     first whitespace-delimited field.  The owner is the publishing
#     meta-repo (META_NAME); it is best-effort provenance, not authoritative.
#   - META_NAME is OPTIONAL.  When set, it is recorded as the owner column
#     and used to detect cross-relic name collisions; when unset, the entry
#     is ownerless.
#
# Unique-name policy (fail fast):
#
#   PATH names must be globally unique, so a relic learns at publish time
#   that it needs a different name.  install_on_path refuses to publish when:
#     - the name is already registered under a DIFFERENT owner, or
#     - the name already resolves elsewhere on $PATH (e.g. a Homebrew
#       binary), or
#     - a foreign (unregistered) file already sits at the target path.
#   Re-publishing a name this owner already manages is allowed (overwrite).
#
# This helper is the rails for the simple, mundane case: copy a built file
# to a chosen name on PATH.  Advanced cases (template substitution, self-
# update, embedded provenance) may sidestep the helper; the owning meta-repo
# documents the convention in its CLAUDE.md.

_INSTALL_ON_PATH_DIR="$HOME/.local/bin"
_INSTALL_ON_PATH_REGISTRY="$_INSTALL_ON_PATH_DIR/.reliquary-managed"

# Capture the owner at SOURCE time. Callers use the idiom
#   META_NAME=bb source install-on-path.sh
# where the assignment is scoped to the source command and is gone by the
# time install_on_path runs on a later line — so we must read it now.
# install_on_path still honours a call-time META_NAME (the export-then-call
# idiom in relic::publish) by preferring it over this captured value.
_INSTALL_ON_PATH_OWNER="${META_NAME:-}"

# True if <name> appears (first field) in the registry.
_install_on_path_registry_contains() {
    local name="$1"
    [[ -f "$_INSTALL_ON_PATH_REGISTRY" ]] || return 1
    awk -v n="$name" '
        /^[[:space:]]*#/ { next }
        /^[[:space:]]*$/ { next }
        $1 == n { found = 1; exit }
        END { exit !found }
    ' "$_INSTALL_ON_PATH_REGISTRY"
}

# Print the owner (second field) recorded for <name>, empty if none/absent.
_install_on_path_registry_owner() {
    local name="$1"
    [[ -f "$_INSTALL_ON_PATH_REGISTRY" ]] || return 0
    awk -v n="$name" '
        /^[[:space:]]*#/ { next }
        /^[[:space:]]*$/ { next }
        $1 == n { print $2; exit }
    ' "$_INSTALL_ON_PATH_REGISTRY"
}

# Record <name> with the current owner (META_NAME, optional). Idempotent on
# name: an existing entry is left untouched, owner column and all.
_install_on_path_registry_add() {
    local name="$1" owner="${META_NAME:-$_INSTALL_ON_PATH_OWNER}"
    _install_on_path_registry_contains "$name" && return 0
    mkdir -p "$_INSTALL_ON_PATH_DIR"
    if [[ -n "$owner" ]]; then
        printf '%s\t%s\n' "$name" "$owner" >> "$_INSTALL_ON_PATH_REGISTRY"
    else
        printf '%s\n' "$name" >> "$_INSTALL_ON_PATH_REGISTRY"
    fi
}

install_on_path() {
    local src="${1:-}" name="${2:-}"
    local owner="${META_NAME:-$_INSTALL_ON_PATH_OWNER}"

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

    local target="$_INSTALL_ON_PATH_DIR/$name"

    if _install_on_path_registry_contains "$name"; then
        # Already ours? Re-publish is fine unless a different owner claims it.
        local existing_owner
        existing_owner="$(_install_on_path_registry_owner "$name")"
        if [[ -n "$owner" && -n "$existing_owner" && "$owner" != "$existing_owner" ]]; then
            printf 'install_on_path: name collision: %s is already managed by %s; %s cannot publish it too.\n' \
                "$name" "$existing_owner" "$owner" >&2
            printf '    PATH names must be unique — choose a different published name.\n' >&2
            return 1
        fi
    elif [[ -e "$target" ]]; then
        # Not ours, but something already sits at the target path.
        printf 'install_on_path: %s exists but is not in %s — refusing to overwrite.\n' "$target" "$_INSTALL_ON_PATH_REGISTRY" >&2
        printf '    If the file is genuinely yours and you want to manage it, adopt it once:\n' >&2
        if [[ -n "$owner" ]]; then
            printf "        printf '%%s\\\\t%%s\\\\n' %s %s >> %s\n" "$name" "$owner" "$_INSTALL_ON_PATH_REGISTRY" >&2
        else
            printf '        echo %s >> %s\n' "$name" "$_INSTALL_ON_PATH_REGISTRY" >&2
        fi
        printf '    Otherwise remove the existing file first:\n' >&2
        printf '        rm %s\n' "$target" >&2
        return 1
    fi

    # System-wide uniqueness: refuse if the name already resolves to some
    # other file on $PATH (e.g. a Homebrew binary). Our own target resolving
    # back to itself is fine.
    local resolved
    resolved="$(command -v "$name" 2>/dev/null)"
    if [[ -n "$resolved" && "$resolved" == /* && "$resolved" != "$target" ]]; then
        printf 'install_on_path: name collision: %s already resolves on $PATH at %s.\n' "$name" "$resolved" >&2
        printf '    PATH names must be unique — choose a different name or remove the conflicting file.\n' >&2
        return 1
    fi

    mkdir -p "$_INSTALL_ON_PATH_DIR"
    cp "$src" "$target"
    chmod +x "$target"

    _install_on_path_registry_add "$name"

    printf 'Installed %s to %s\n' "$name" "$target"
}

# Fold any legacy per-meta registries (~/.local/bin/.<meta>-managed) into the
# single .reliquary-managed file, using each legacy file's <meta> as the owner
# column, then remove the legacy file. Idempotent and safe to re-run: names
# already present are skipped, and once folded the legacy files are gone so the
# glob matches nothing. A name claimed by two different metas is a real
# collision — warn and keep the first; the user resolves it by hand.
install_on_path_migrate_registries() {
    local legacy base meta name existing

    # No nullglob: when the glob matches nothing it stays literal and the
    # [[ -f ]] guard skips it. Keeps this portable to non-bash sourcing.
    for legacy in "$_INSTALL_ON_PATH_DIR"/.*-managed; do
        [[ "$legacy" == "$_INSTALL_ON_PATH_REGISTRY" ]] && continue
        [[ -f "$legacy" ]] || continue
        base="$(basename "$legacy")"
        meta="${base#.}"
        meta="${meta%-managed}"

        while IFS= read -r name || [[ -n "$name" ]]; do
            name="${name%%[[:space:]]*}"
            [[ -z "$name" || "$name" == \#* ]] && continue
            if _install_on_path_registry_contains "$name"; then
                existing="$(_install_on_path_registry_owner "$name")"
                if [[ -n "$existing" && "$existing" != "$meta" ]]; then
                    printf 'install-on-path: migrate: %s claimed by both %s and %s — keeping %s; resolve by hand.\n' \
                        "$name" "$existing" "$meta" "$existing" >&2
                fi
                continue
            fi
            mkdir -p "$_INSTALL_ON_PATH_DIR"
            printf '%s\t%s\n' "$name" "$meta" >> "$_INSTALL_ON_PATH_REGISTRY"
        done < "$legacy"

        rm -f "$legacy"
    done

    return 0
}
