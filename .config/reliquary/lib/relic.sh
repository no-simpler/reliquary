#!/usr/bin/env bash
#
# relic.sh — shared library for Stage 1+2 relic management.
#
# Stage 3 (external relics like ~/Developer/bb, ~/Developer/halo) do NOT
# depend on this lib. They publish via ~/.config/shell/lib/install-on-path.sh
# directly. The stable cross-stage API is install-on-path; relic.sh is the
# convenience layer for in-house relics.
#
# Usage:
#   source "$HOME/.config/reliquary/lib/relic.sh"
#   relic::publish ~/.config/relics/<name>
#   relic::test    ~/.config/relics/<name>
#   relic::update  ~/.config/relics/<name>
#
# Each operation reads <dir>/relic.sh (the per-relic manifest). If
# <dir>/scripts/<op>.sh exists and is executable, it is run instead of the
# default behavior — relics override only when they need to.
#
# Manifest schema (<dir>/relic.sh):
#   NAME                  — required; published-name and META_NAME
#   DESCRIPTION           — optional; one-line summary
#   RUNTIME               — required; python|bash|fish|rust|docker|...
#   MIN_RUNTIME_VERSION   — optional; semver-ish, enforced at publish time
#   BREW_DEPS             — optional; array of brew package names
#   EXTERNAL_DEPS         — optional; free-form notes (not enforced)
#   DOCKER                — optional; 1 if entrypoints are docker-run shims
#
# Entrypoint convention: <dir>/entrypoints/<published-name> is the source
# (typically a symlink into <dir>/src/). Filename literally is the published
# name. install_on_path copies the file (cp follows symlinks) into ~/.local/bin/.

relic::_die() {
    printf 'relic: %s\n' "$1" >&2
    return 1
}

relic::_version_ge() {
    # Return 0 if version $1 >= $2 (dotted-numeric, sort -V semantics).
    local have="$1" need="$2"
    [[ "$(printf '%s\n%s\n' "$need" "$have" | sort -V | head -1)" == "$need" ]]
}

relic::load_manifest() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || { relic::_die "load_manifest: missing dir"; return $?; }
    local manifest="$dir/relic.sh"
    [[ -f "$manifest" ]] || { relic::_die "no manifest at $manifest"; return $?; }

    # Reset known fields so prior-load values don't leak when iterating relics.
    NAME=""
    DESCRIPTION=""
    RUNTIME=""
    MIN_RUNTIME_VERSION=""
    BREW_DEPS=()
    EXTERNAL_DEPS=()
    DOCKER=0

    # shellcheck disable=SC1090
    source "$manifest" || { relic::_die "failed to source $manifest"; return $?; }

    [[ -n "$NAME" ]]    || { relic::_die "manifest missing NAME: $manifest"; return $?; }
    [[ -n "$RUNTIME" ]] || { relic::_die "manifest missing RUNTIME: $manifest"; return $?; }
}

relic::check_deps() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || { relic::_die "check_deps: missing dir"; return $?; }
    relic::load_manifest "$dir" || return $?

    local fail=0 pkg

    for pkg in "${BREW_DEPS[@]}"; do
        if ! command -v "$pkg" >/dev/null 2>&1; then
            printf 'relic[%s]: missing dep: %s — install with: brew install %s\n' \
                "$NAME" "$pkg" "$pkg" >&2
            fail=1
        fi
    done

    if [[ -n "$MIN_RUNTIME_VERSION" ]]; then
        case "$RUNTIME" in
            python)
                if ! command -v python3 >/dev/null 2>&1; then
                    printf 'relic[%s]: python3 not on PATH\n' "$NAME" >&2; fail=1
                else
                    local ver
                    ver="$(python3 -c 'import sys; print("%d.%d" % sys.version_info[:2])' 2>/dev/null)"
                    if [[ -z "$ver" ]] || ! relic::_version_ge "$ver" "$MIN_RUNTIME_VERSION"; then
                        printf 'relic[%s]: python3 %s < required %s\n' \
                            "$NAME" "${ver:-unknown}" "$MIN_RUNTIME_VERSION" >&2
                        fail=1
                    fi
                fi
                ;;
            bash)
                local ver="${BASH_VERSION%%[^0-9.]*}"
                if [[ -z "$ver" ]] || ! relic::_version_ge "$ver" "$MIN_RUNTIME_VERSION"; then
                    printf 'relic[%s]: bash %s < required %s\n' \
                        "$NAME" "${ver:-unknown}" "$MIN_RUNTIME_VERSION" >&2
                    fail=1
                fi
                ;;
            rust)
                if ! command -v rustc >/dev/null 2>&1; then
                    printf 'relic[%s]: rustc not on PATH\n' "$NAME" >&2; fail=1
                else
                    local ver
                    ver="$(rustc --version 2>/dev/null | awk '{print $2}')"
                    if [[ -z "$ver" ]] || ! relic::_version_ge "$ver" "$MIN_RUNTIME_VERSION"; then
                        printf 'relic[%s]: rustc %s < required %s\n' \
                            "$NAME" "${ver:-unknown}" "$MIN_RUNTIME_VERSION" >&2
                        fail=1
                    fi
                fi
                ;;
            fish)
                if ! command -v fish >/dev/null 2>&1; then
                    printf 'relic[%s]: fish not on PATH\n' "$NAME" >&2; fail=1
                fi
                ;;
            docker)
                if ! command -v docker >/dev/null 2>&1; then
                    printf 'relic[%s]: docker not on PATH\n' "$NAME" >&2; fail=1
                fi
                ;;
        esac
    fi

    return "$fail"
}

relic::publish() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || { relic::_die "publish: missing dir"; return $?; }

    if [[ -x "$dir/scripts/publish.sh" ]]; then
        ( cd "$dir" && ./scripts/publish.sh )
        return $?
    fi

    relic::check_deps "$dir" || return $?

    local entrypoints_dir="$dir/entrypoints"
    if [[ ! -d "$entrypoints_dir" ]]; then
        printf 'relic[%s]: no entrypoints/ directory; nothing to publish\n' "$NAME" >&2
        return 0
    fi

    local name_for_meta="$NAME"

    (
        export META_NAME="$name_for_meta"
        # shellcheck disable=SC1091
        source "$HOME/.config/shell/lib/install-on-path.sh" || exit $?

        local count=0
        for ep in "$entrypoints_dir"/*; do
            [[ -e "$ep" ]] || continue
            local n
            n="$(basename "$ep")"
            case "$n" in
                .*) continue ;;
            esac
            install_on_path "$ep" "$n" || exit $?
            count=$((count + 1))
        done

        if [[ $count -eq 0 ]]; then
            printf 'relic[%s]: no entrypoints published\n' "$META_NAME" >&2
        fi
    )
}

relic::test() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || { relic::_die "test: missing dir"; return $?; }

    if [[ -x "$dir/scripts/test.sh" ]]; then
        ( cd "$dir" && ./scripts/test.sh )
        return $?
    fi

    relic::load_manifest "$dir" || return $?

    local tests_dir="$dir/tests"
    if [[ ! -d "$tests_dir" ]]; then
        printf 'relic[%s]: no tests/ directory; nothing to run\n' "$NAME"
        return 0
    fi

    case "$RUNTIME" in
        python)
            if command -v pytest >/dev/null 2>&1; then
                ( cd "$dir" && pytest tests/ )
            else
                ( cd "$dir" && python3 -m unittest discover tests/ )
            fi
            ;;
        bash)
            if [[ -x "$tests_dir/run.sh" ]]; then
                ( cd "$dir" && ./tests/run.sh )
            else
                local fail=0 t
                for t in "$tests_dir"/*.sh; do
                    [[ -f "$t" ]] || continue
                    bash "$t" || fail=1
                done
                return "$fail"
            fi
            ;;
        rust)
            ( cd "$dir" && cargo test )
            ;;
        *)
            printf 'relic[%s]: no default test runner for RUNTIME=%s\n' "$NAME" "$RUNTIME"
            return 0
            ;;
    esac
}

relic::update() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || { relic::_die "update: missing dir"; return $?; }

    if [[ -x "$dir/scripts/update.sh" ]]; then
        ( cd "$dir" && ./scripts/update.sh )
        return $?
    fi

    relic::load_manifest "$dir" || return $?

    case "$RUNTIME" in
        rust)
            ( cd "$dir" && cargo build --release ) || return $?
            relic::publish "$dir"
            ;;
        *)
            return 0
            ;;
    esac
}
