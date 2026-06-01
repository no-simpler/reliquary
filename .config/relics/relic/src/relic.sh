#!/usr/bin/env bash
#
# relic — manage Reliquary relics.
#
# The first in-house (Stage-2) relic; it dogfoods the very pipeline it
# manages. A single self-contained file: the published entrypoint is one
# copied file in ~/.local/bin, so it sources its libraries by absolute path
# at runtime rather than relying on sibling files.
#
# Commands (unambiguous prefixes accepted, e.g. `relic st`):
#   list                  all relics with stage, runtime, published-state
#   status [<name>]       one relic's detail (deps, PATH wiring, git dirty)
#   publish [<name>]      publish an in-house relic's entrypoints onto PATH
#   test    [<name>]      run an in-house relic's tests
#   update  [<name>]      update/rebuild an in-house relic
#   registry [--migrate]  show the shared PATH registry (name → owner)
#   migrate               fold legacy per-meta registries into .reliquary-managed
#   help                  this message
#
# <name> may be omitted for status/publish/test/update when run from inside
# a relic directory (cwd auto-detect).

set -uo pipefail

RELIC_LIB="$HOME/.config/reliquary/lib/relic.sh"
INSTALL_ON_PATH_LIB="$HOME/.config/shell/lib/install-on-path.sh"
RELICS_LANE="$HOME/.config/relics"
ATTIC_LANE="$HOME/.config/attic"
GRADUATION="$HOME/.config/reliquary/GRADUATION.md"
REGISTRY="$HOME/.local/bin/.reliquary-managed"

COMMANDS="list status publish test update registry migrate help"

# ── output ──────────────────────────────────────────────────────────────────

if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
    _c_dim=$'\033[2m'; _c_bold=$'\033[1m'
    _c_red=$'\033[31m'; _c_grn=$'\033[32m'; _c_yel=$'\033[33m'; _c_rst=$'\033[0m'
else
    _c_dim=''; _c_bold=''; _c_red=''; _c_grn=''; _c_yel=''; _c_rst=''
fi

info() { printf '%s\n' "$*"; }
warn() { printf '%swarn:%s %s\n'  "$_c_yel" "$_c_rst" "$*" >&2; }
err()  { printf '%serror:%s %s\n' "$_c_red" "$_c_rst" "$*" >&2; }
die()  { err "$*"; exit 1; }

# ── library loading (lazy) ──────────────────────────────────────────────────

_load_relic_lib() {
    [[ -f "$RELIC_LIB" ]] || die "relic library not found: $RELIC_LIB"
    # shellcheck disable=SC1090
    source "$RELIC_LIB"
}

_load_install_on_path() {
    [[ -f "$INSTALL_ON_PATH_LIB" ]] || die "install-on-path not found: $INSTALL_ON_PATH_LIB"
    # shellcheck disable=SC1090
    source "$INSTALL_ON_PATH_LIB"
}

# ── registry (read-only helpers; writes go through install-on-path) ──────────

reg_has() {
    [[ -f "$REGISTRY" ]] || return 1
    awk -v n="$1" '
        /^[[:space:]]*#/ { next } /^[[:space:]]*$/ { next }
        $1 == n { f = 1; exit } END { exit !f }
    ' "$REGISTRY"
}

reg_owner() {
    [[ -f "$REGISTRY" ]] || return 0
    awk -v n="$1" '
        /^[[:space:]]*#/ { next } /^[[:space:]]*$/ { next }
        $1 == n { print $2; exit }
    ' "$REGISTRY"
}

# ── discovery ───────────────────────────────────────────────────────────────

# Read RUNTIME from a manifest without leaking globals into our process.
relic_runtime() {
    ( unset RUNTIME 2>/dev/null; source "$1/relic.sh" 2>/dev/null; printf '%s' "${RUNTIME:-?}" )
}

# Emit "<name>\t<dir>\t<lane>" per in-house relic. Attic-safe: a relic is
# only surfaced when its manifest is readable, so an encrypted/undecrypted
# attic lane reveals nothing.
inhouse_relics() {
    local lane r name
    for lane in "$RELICS_LANE" "$ATTIC_LANE"; do
        [[ -d "$lane" ]] || continue
        for r in "$lane"/*/; do
            [[ -r "${r}relic.sh" ]] || continue
            name="$(basename "$r")"
            printf '%s\t%s\t%s\n' "$name" "${r%/}" "$lane"
        done
    done
}

# Emit "<name>\t<path>" per known external (Stage-3) relic, parsed from the
# GRADUATION.md "Known external relics" list. Best-effort: a parse miss or a
# missing file yields no rows rather than an error.
external_relics() {
    [[ -r "$GRADUATION" ]] || return 0
    local in=0 line name path
    while IFS= read -r line; do
        if [[ "$line" == '### Known external relics'* ]]; then in=1; continue; fi
        if [[ $in -eq 1 && "$line" == '#'* ]]; then break; fi
        if [[ $in -eq 1 && "$line" == '- '* ]]; then
            if [[ "$line" =~ \`([^\`]+)\`[^\`]*\`([^\`]+)\` ]]; then
                name="${BASH_REMATCH[1]}"
                path="${BASH_REMATCH[2]}"
                path="${path/#\~/$HOME}"
                path="${path%/}"
                printf '%s\t%s\n' "$name" "$path"
            fi
        fi
    done < "$GRADUATION"
}

relic_entrypoints() {
    local dir="$1" ep n
    [[ -d "$dir/entrypoints" ]] || return 0
    for ep in "$dir"/entrypoints/*; do
        [[ -e "$ep" ]] || continue
        n="$(basename "$ep")"
        case "$n" in .*) continue ;; esac
        printf '%s\n' "$n"
    done
}

find_inhouse_dir() {
    local name="$1" lane d
    for lane in "$RELICS_LANE" "$ATTIC_LANE"; do
        d="$lane/$name"
        [[ -r "$d/relic.sh" ]] && { printf '%s' "$d"; return 0; }
    done
    return 1
}

find_external_path() {
    local name="$1" n p
    while IFS=$'\t' read -r n p; do
        [[ "$n" == "$name" ]] && { printf '%s' "$p"; return 0; }
    done < <(external_relics)
    return 1
}

detect_cwd_relic() {
    local cwd="$PWD" lane name n p
    for lane in "$RELICS_LANE" "$ATTIC_LANE"; do
        case "$cwd/" in
            "$lane"/*)
                name="${cwd#"$lane"/}"; name="${name%%/*}"
                [[ -n "$name" && -r "$lane/$name/relic.sh" ]] && { printf '%s' "$name"; return 0; }
                ;;
        esac
    done
    while IFS=$'\t' read -r n p; do
        case "$cwd/" in "$p"/*) printf '%s' "$n"; return 0 ;; esac
    done < <(external_relics)
    return 1
}

# ── published-state ─────────────────────────────────────────────────────────

inhouse_pubstate() {
    local dir="$1" total=0 have=0 n
    while IFS= read -r n; do
        [[ -z "$n" ]] && continue
        total=$((total + 1))
        reg_has "$n" && have=$((have + 1))
    done < <(relic_entrypoints "$dir")
    if   [[ $total -eq 0 ]]; then echo "no-entrypoints"
    elif [[ $have  -eq 0 ]]; then echo "unpublished"
    elif [[ $have  -lt $total ]]; then echo "partial"
    else echo "published"; fi
}

external_pubstate() {
    [[ -f "$REGISTRY" ]] || { echo "unknown"; return; }
    if awk -v o="$1" '/^[[:space:]]*#/{next} $2==o{f=1;exit} END{exit !f}' "$REGISTRY"; then
        echo "published"
    else
        echo "unknown"
    fi
}

state_label() {
    case "$1" in
        published)      printf '%s● published%s'           "$_c_grn" "$_c_rst" ;;
        partial)        printf '%s◐ partial%s'             "$_c_yel" "$_c_rst" ;;
        unpublished)    printf '%s○ unpublished%s'         "$_c_dim" "$_c_rst" ;;
        no-entrypoints) printf '%s— no entrypoints%s'      "$_c_dim" "$_c_rst" ;;
        absent)         printf '%s○ listed, not present%s' "$_c_dim" "$_c_rst" ;;
        unknown)        printf '%s? unknown%s'             "$_c_dim" "$_c_rst" ;;
        *)              printf '%s' "$1" ;;
    esac
}

state_plain() {
    case "$1" in
        published)      echo "yes" ;;
        partial)        echo "partial" ;;
        unpublished)    echo "no" ;;
        no-entrypoints) echo "n/a (no entrypoints)" ;;
        absent)         echo "not present" ;;
        unknown)        echo "unknown" ;;
        *)              echo "$1" ;;
    esac
}

# ── commands ────────────────────────────────────────────────────────────────

cmd_list() {
    local name dir lane rt state any

    info "${_c_bold}In-house relics${_c_rst} ${_c_dim}(~/.config/relics)${_c_rst}"
    any=0
    while IFS=$'\t' read -r name dir lane; do
        [[ "$lane" == "$RELICS_LANE" ]] || continue
        any=1
        rt="$(relic_runtime "$dir")"
        printf '  %-18s %-7s %s\n' "$name" "$rt" "$(state_label "$(inhouse_pubstate "$dir")")"
    done < <(inhouse_relics)
    [[ $any -eq 0 ]] && printf '  %s(none)%s\n' "$_c_dim" "$_c_rst"

    local priv
    priv="$(inhouse_relics | awk -F'\t' -v l="$ATTIC_LANE" '$3==l')"
    if [[ -n "$priv" ]]; then
        info ""
        info "${_c_bold}Private relics${_c_rst} ${_c_dim}(~/.config/attic)${_c_rst}"
        while IFS=$'\t' read -r name dir lane; do
            rt="$(relic_runtime "$dir")"
            printf '  %-18s %-7s %s\n' "$name" "$rt" "$(state_label "$(inhouse_pubstate "$dir")")"
        done <<< "$priv"
    fi

    local ext
    ext="$(external_relics)"
    if [[ -n "$ext" ]]; then
        info ""
        info "${_c_bold}External relics${_c_rst} ${_c_dim}(Stage 3; per GRADUATION.md)${_c_rst}"
        while IFS=$'\t' read -r name dir; do
            if [[ -d "$dir" ]]; then state="$(external_pubstate "$name")"; else state="absent"; fi
            printf '  %-18s %-7s %s\n' "$name" "ext" "$(state_label "$state")"
        done <<< "$ext"
    fi
}

cmd_status() {
    local name="${1:-}"
    [[ -n "$name" ]] || name="$(detect_cwd_relic)" || die "no relic given and none detected from cwd"

    local dir path
    if dir="$(find_inhouse_dir "$name")"; then
        _status_inhouse "$name" "$dir"
    elif path="$(find_external_path "$name")"; then
        _status_external "$name" "$path"
    else
        die "unknown relic: $name"
    fi
}

_status_inhouse() {
    local name="$1" dir="$2" rt n owner out
    _load_relic_lib
    rt="$(relic_runtime "$dir")"
    info "${_c_bold}$name${_c_rst} ${_c_dim}$dir${_c_rst}"
    info "  stage:     2 (in-house)"
    info "  runtime:   $rt"
    info "  published: $(state_plain "$(inhouse_pubstate "$dir")")"
    while IFS= read -r n; do
        [[ -z "$n" ]] && continue
        if reg_has "$n"; then
            owner="$(reg_owner "$n")"
            info "    - $n → ~/.local/bin/$n${owner:+ ${_c_dim}(owner: $owner)${_c_rst}}"
        else
            info "    - $n → ${_c_dim}not on PATH${_c_rst}"
        fi
    done < <(relic_entrypoints "$dir")
    if out="$(relic::check_deps "$dir" 2>&1)"; then
        info "  deps:      ${_c_grn}ok${_c_rst}"
    else
        info "  deps:      ${_c_red}missing${_c_rst}"
        printf '%s\n' "$out" | sed 's/^/    /'
    fi
}

_status_external() {
    local name="$1" path="$2"
    info "${_c_bold}$name${_c_rst} ${_c_dim}$path${_c_rst}"
    info "  stage:     3 (external)"
    if [[ -d "$path" ]]; then
        info "  present:   yes"
        if [[ -d "$path/.git" ]] && command -v git >/dev/null 2>&1; then
            if [[ -n "$(git -C "$path" status --porcelain 2>/dev/null)" ]]; then
                info "  git:       ${_c_yel}dirty${_c_rst}"
            else
                info "  git:       ${_c_grn}clean${_c_rst}"
            fi
        fi
        info "  published: $(state_plain "$(external_pubstate "$name")") ${_c_dim}(best-effort, by owner column)${_c_rst}"
        if [[ -f "$REGISTRY" ]]; then
            awk -v o="$name" '/^[[:space:]]*#/{next} $2==o{print "    - "$1" → ~/.local/bin/"$1}' "$REGISTRY"
        fi
    else
        info "  present:   ${_c_dim}no (listed in GRADUATION.md, not on this machine)${_c_rst}"
    fi
    info "  manage:    in its own repo — $path"
}

cmd_publish() { _run_op publish "$@"; }
cmd_test()    { _run_op test    "$@"; }
cmd_update()  { _run_op update  "$@"; }

_run_op() {
    local op="$1" name="${2:-}" dir path
    [[ -n "$name" ]] || name="$(detect_cwd_relic)" || die "no relic given and none detected from cwd"
    if dir="$(find_inhouse_dir "$name")"; then
        _load_relic_lib
        "relic::$op" "$dir"
    elif path="$(find_external_path "$name")"; then
        die "$name is an external (Stage 3) relic — run its own $op flow in $path"
    else
        die "unknown in-house relic: $name"
    fi
}

cmd_registry() {
    if [[ "${1:-}" == "--migrate" ]]; then cmd_migrate; return; fi
    if [[ ! -f "$REGISTRY" ]]; then
        info "${_c_dim}registry empty — $REGISTRY does not exist yet${_c_rst}"
        return
    fi
    info "${_c_bold}PATH registry${_c_rst} ${_c_dim}$REGISTRY${_c_rst}"
    awk '
        /^[[:space:]]*#/ { next } /^[[:space:]]*$/ { next }
        { printf "  %-20s %s\n", $1, ($2=="" ? "-" : $2) }
    ' "$REGISTRY"
}

cmd_migrate() {
    _load_install_on_path
    install_on_path_migrate_registries
    info "folded any legacy per-meta registries into $REGISTRY"
}

usage() {
    cat <<EOF
relic — manage Reliquary relics

usage: relic <command> [<name>]

commands:
  list                  all relics with stage, runtime, published-state
  status [<name>]       one relic's detail (deps, PATH wiring, git dirty)
  publish [<name>]      publish an in-house relic's entrypoints onto PATH
  test    [<name>]      run an in-house relic's tests
  update  [<name>]      update/rebuild an in-house relic
  registry [--migrate]  show the shared PATH registry (name → owner)
  migrate               fold legacy per-meta registries into .reliquary-managed
  help                  this message

<name> is optional for status/publish/test/update when run from inside a
relic directory. Unambiguous command prefixes are accepted (e.g. \`relic st\`).
EOF
}

# ── dispatch ────────────────────────────────────────────────────────────────

resolve_cmd() {
    local input="$1" c match="" n=0
    for c in $COMMANDS; do
        [[ "$c" == "$input" ]] && { printf '%s' "$c"; return 0; }
    done
    for c in $COMMANDS; do
        case "$c" in "$input"*) match="$c"; n=$((n + 1)) ;; esac
    done
    [[ $n -eq 1 ]] && { printf '%s' "$match"; return 0; }
    return 1
}

main() {
    local raw="${1:-help}" cmd
    shift || true
    if ! cmd="$(resolve_cmd "$raw")"; then
        err "unknown or ambiguous command: $raw"
        usage
        exit 1
    fi
    case "$cmd" in
        list)     cmd_list "$@" ;;
        status)   cmd_status "$@" ;;
        publish)  cmd_publish "$@" ;;
        test)     cmd_test "$@" ;;
        update)   cmd_update "$@" ;;
        registry) cmd_registry "$@" ;;
        migrate)  cmd_migrate "$@" ;;
        help)     usage ;;
    esac
}

# Run only when executed, not when sourced (tests source this file).
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
