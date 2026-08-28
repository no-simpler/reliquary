#!/bin/bash
#
# PATH invariants for the bootstrap process.
#
# `lib/` is sourced first — before `util/` — so this file defines functions and
# nothing else: no output, no side effects at source time. `print_*` does not
# exist yet.
#
# Why it exists: a guarded fast path must still establish its postcondition,
# not merely skip the work. `01-homebrew.sh` evaluated `brew shellenv` only on
# the branch that *installed* Homebrew, so any run finding brew already present
# continued with whatever PATH it inherited. That is how a stock
# /usr/bin/python3 (3.9, no `tomllib`) won over Homebrew's and took every relic
# publish down with it — invisible on this machine, where `env.d` had already
# ordered PATH, and fatal in a scratch account, which always takes that branch.
#
# bash-3.2-safe: sourced into stock macOS bash. No associative arrays, no
# ${x,,}, no `local -n`. Pattern substitution (${x//y/z}) is bash 2.0 and fine.

# bootstrap::path_prepend <dir> [<dir>…]
#
# Put each existing directory at the front of PATH, exactly once. Idempotent by
# construction: a directory already on PATH is moved to the front rather than
# duplicated, so repeated sourcing cannot grow PATH.
bootstrap::path_prepend() {
    local dir tmp
    for dir in "$@"; do
        [ -d "$dir" ] || continue
        tmp=":$PATH:"
        tmp="${tmp//:$dir:/:}"
        tmp="${tmp#:}"
        tmp="${tmp%:}"
        if [ -n "$tmp" ]; then
            PATH="$dir:$tmp"
        else
            PATH="$dir"
        fi
    done
    export PATH
}

# bootstrap::brew_shellenv
#
# Put Homebrew on PATH for the rest of the bootstrap run, whether or not this
# run installed it. Returns 1 when no brew is found, so the caller can say so.
#
# The dialect is passed explicitly. `brew shellenv` otherwise infers it from the
# parent process, which is ambient authority: correct here today only because
# the bootstrap interpreter happens to be bash. Naming it costs nothing and
# removes the inference (house rule 2 — ambient authority is injected, never
# read).
#
# Idempotent: `brew shellenv` emits no `export PATH=` when the prefix already
# leads PATH, so repeated calls cannot duplicate entries.
bootstrap::brew_shellenv() {
    local brew
    for brew in /opt/homebrew/bin/brew /usr/local/bin/brew; do
        if [ -x "$brew" ]; then
            eval "$("$brew" shellenv bash)"
            return 0
        fi
    done

    # Not at a canonical prefix, but possibly on an inherited PATH.
    brew="$(command -v brew 2>/dev/null)"
    if [ -n "$brew" ] && [ -x "$brew" ]; then
        eval "$("$brew" shellenv bash)"
        return 0
    fi

    return 1
}
