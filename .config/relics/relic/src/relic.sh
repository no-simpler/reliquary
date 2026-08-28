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
#   test    [<name>] [--cover]
#                         run the tests; --cover adds coverage and the ratchet
#   mutants [<name>]      mutation testing — the assertion-quality gate
#   update  [<name>]      update/rebuild an in-house relic
#   scaffold <name> [-r <rt>] [-e <why>]
#                         promote a Stage-1 bin/ util (or a fresh idea) into a
#                         Stage-2 in-house relic. Rust unless -r says otherwise,
#                         and anything but rust needs -e/--exempt.
#   registry [--migrate|--prune]  show / fold / prune the shared PATH registry
#   migrate               fold legacy per-meta registries into .reliquary-managed
#   doctor                cross-check registry ↔ ~/.local/bin ↔ entrypoints
#   help                  this message
#
# <name> may be omitted for status/publish/test/update when run from inside
# a relic directory (cwd auto-detect).

set -uo pipefail

RELIC_LIB="$HOME/.config/reliquary/lib/relic.sh"
INSTALL_ON_PATH_LIB="$HOME/.config/reliquary/lib/install-on-path.sh"
RELICS_LANE="$HOME/.config/relics"
ATTIC_LANE="$HOME/.config/attic"
BIN_LANE="$HOME/.config/bin"
TEMPLATE_DIR="$HOME/.config/reliquary/template"
GRADUATION="$HOME/.config/reliquary/GRADUATION.md"
LOCAL_BIN="$HOME/.local/bin"
REGISTRY="$LOCAL_BIN/.reliquary-managed"

COMMANDS="list status publish test mutants update scaffold registry migrate doctor help"

# ── output ──────────────────────────────────────────────────────────────────

if [[ -t 1 && -z "${NO_COLOR:-}" ]]; then
    _c_dim=$'\033[2m'
    _c_bold=$'\033[1m'
    _c_red=$'\033[31m'
    _c_grn=$'\033[32m'
    _c_yel=$'\033[33m'
    _c_rst=$'\033[0m'
else
    _c_dim=''
    _c_bold=''
    _c_red=''
    _c_grn=''
    _c_yel=''
    _c_rst=''
fi

info() { printf '%s\n' "$*"; }
warn() { printf '%swarn:%s %s\n' "$_c_yel" "$_c_rst" "$*" >&2; }
err() { printf '%serror:%s %s\n' "$_c_red" "$_c_rst" "$*" >&2; }
die() {
    err "$*"
    exit 1
}

# ── library loading (lazy) ──────────────────────────────────────────────────

# Idempotent, because discovery needs the library too and every command that
# touches a relic reaches for it more than once.
_load_relic_lib() {
    declare -F relic::has_manifest >/dev/null && return 0
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

# Emit every registered name (first field), one per line.
reg_names() {
    [[ -f "$REGISTRY" ]] || return 0
    awk '/^[[:space:]]*#/ { next } /^[[:space:]]*$/ { next } { print $1 }' "$REGISTRY"
}

# ── doctor (registry ↔ ~/.local/bin ↔ entrypoints drift) ─────────────────────

# Registry entries with no backing file in $LOCAL_BIN — `registry --prune`
# fodder. Emits "<name>\t<owner>" per orphan.
doctor_orphans() {
    local name owner
    while IFS= read -r name; do
        [[ -z "$name" ]] && continue
        [[ -e "$LOCAL_BIN/$name" ]] && continue
        owner="$(reg_owner "$name")"
        printf '%s\t%s\n' "$name" "$owner"
    done < <(reg_names)
}

# In-house entrypoints that aren't in the registry — i.e. declared but never
# published (the inverse of an orphan). Emits "<relic>\t<entrypoint>" per gap.
#
# Reads rows on stdin rather than discovering them: doctor asks more than one
# question of the same lane, and discovery is the expensive half. Attic-safe
# through its source — inhouse_relics only surfaces readable manifests.
doctor_unpublished() {
    local name dir lane runtime why eps ep
    while IFS=$'\037' read -r name dir lane runtime why eps; do
        for ep in $eps; do
            reg_has "$ep" && continue
            printf '%s\t%s\n' "$name" "$ep"
        done
    done
}

# Relics that are not Rust and do not say why. Relics are Rust by default; any
# other runtime records its reason in RUNTIME_EXEMPTION. Emits "<relic>\t<runtime>"
# per omission.
#
# Informational, never a failure: a relic awaiting its rewrite has to keep
# publishing, and this list is the worklist for getting through them — it empties
# as each one is either rewritten into the workspace or given its reason.
doctor_runtime_stance() {
    local name dir lane runtime why eps
    while IFS=$'\037' read -r name dir lane runtime why eps; do
        [[ "$runtime" == "rust" ]] && continue
        [[ -n "$why" ]] && continue
        printf '%s\t%s\n' "$name" "$runtime"
    done
}

# Executable files in $LOCAL_BIN (non-dotfiles) absent from the registry —
# foreign or sanctioned-sidestep binaries sharing the lane. Informational.
doctor_unmanaged() {
    local f n
    [[ -d "$LOCAL_BIN" ]] || return 0
    for f in "$LOCAL_BIN"/*; do
        [[ -f "$f" && -x "$f" ]] || continue
        n="$(basename "$f")"
        case "$n" in .*) continue ;; esac
        reg_has "$n" && continue
        printf '%s\n' "$n"
    done
}

# ── discovery ───────────────────────────────────────────────────────────────

# One row per in-house relic, unit-separated (US, \037):
#
#   <name>  <dir>  <lane>  <runtime>  <runtime-exemption>  <entrypoints…>
#
# Every manifest in both lanes is read once, together. A reader start costs
# more than every read after it, and nothing below wants only one field.
#
# Attic-safe: a relic is surfaced only when its manifest is readable, so an
# encrypted attic lane reveals nothing. A manifest that is readable but broken
# is reported rather than skipped — silence there is how a relic disappears.
#
# The last column is space-joined because a published name is a filename on
# PATH, which is where the constraint that it holds no whitespace already lives.
#
# Not tab-separated: tab is IFS whitespace, so `read` collapses an empty
# interior field and every column after it shifts by one — a relic with no
# runtime-exemption would hand its entrypoints to the wrong variable. US is the
# delimiter that exists for this and cannot occur in a path or a published name.
#
# Order is asserted here rather than inherited from glob expansion, so it does
# not turn on which format a manifest happens to be in.
inhouse_relics() {
    _load_relic_lib
    local lane r dirs=()
    for lane in "$RELICS_LANE" "$ATTIC_LANE"; do
        [[ -d "$lane" ]] || continue
        for r in "$lane"/*/; do
            relic::has_manifest "${r%/}" || continue
            dirs=(${dirs[@]+"${dirs[@]}"} "${r%/}")
        done
    done
    [[ ${#dirs[@]} -eq 0 ]] && return 0

    local buffer="" line rank eps
    while IFS= read -r line; do
        buffer="$buffer$line"$'\n'
        [[ "$line" == "__RELIC_END=1" ]] || continue
        __RELIC_DIR=""
        __RELIC_ERROR=""
        NAME=""
        RUNTIME=""
        RUNTIME_EXEMPTION=""
        ENTRYPOINTS=()
        eval "$buffer"
        buffer=""
        if [[ -n "$__RELIC_ERROR" ]]; then
            warn "$__RELIC_ERROR"
            continue
        fi
        case "$__RELIC_DIR/" in
            "$RELICS_LANE"/*) rank=1 lane="$RELICS_LANE" ;;
            "$ATTIC_LANE"/*) rank=2 lane="$ATTIC_LANE" ;;
            *) continue ;;
        esac
        eps="$(entrypoints_of "$__RELIC_DIR" "$RUNTIME" "$NAME" \
            ${ENTRYPOINTS[@]+"${ENTRYPOINTS[@]}"} | tr '\n' ' ')"
        printf '%s\037%s\037%s\037%s\037%s\037%s\037%s\n' \
            "$rank" "$(basename "$__RELIC_DIR")" "$__RELIC_DIR" "$lane" \
            "${RUNTIME:-?}" "$RUNTIME_EXEMPTION" "${eps% }"
    done < <(relic::_manifest_read ${dirs[@]+"${dirs[@]}"}) |
        sort -t$'\037' -k1,1 -k2,2 | cut -d$'\037' -f2-
}

# Emit "<name>\t<path>" per known external (Stage-3) relic, parsed from the
# GRADUATION.md "Known external relics" list. Best-effort: a parse miss or a
# missing file yields no rows rather than an error.
external_relics() {
    [[ -r "$GRADUATION" ]] || return 0
    local in=0 line name path
    while IFS= read -r line; do
        if [[ "$line" == '### Known external relics'* ]]; then
            in=1
            continue
        fi
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
    done <"$GRADUATION"
}

# The names a relic publishes, one per line, from fields a caller already has.
# A compiled relic declares them in its manifest, because its artifact does not
# exist until cargo has run and lands in the workspace target/ rather than
# beside the source; an interpreted one declares them by filename in
# entrypoints/, where the file *is* the artifact.
entrypoints_of() {
    local dir="$1" runtime="$2" name="$3" ep n
    shift 3
    if [[ "$runtime" == "rust" ]]; then
        if [[ $# -gt 0 ]]; then
            printf '%s\n' "$@"
        elif [[ -n "$name" ]]; then
            printf '%s\n' "$name"
        fi
        return 0
    fi
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
    _load_relic_lib
    for lane in "$RELICS_LANE" "$ATTIC_LANE"; do
        d="$lane/$name"
        relic::has_manifest "$d" && {
            printf '%s' "$d"
            return 0
        }
    done
    return 1
}

find_external_path() {
    local name="$1" n p
    while IFS=$'\t' read -r n p; do
        [[ "$n" == "$name" ]] && {
            printf '%s' "$p"
            return 0
        }
    done < <(external_relics)
    return 1
}

detect_cwd_relic() {
    local cwd="$PWD" lane name n p
    _load_relic_lib
    for lane in "$RELICS_LANE" "$ATTIC_LANE"; do
        case "$cwd/" in
            "$lane"/*)
                name="${cwd#"$lane"/}"
                name="${name%%/*}"
                [[ -n "$name" ]] && relic::has_manifest "$lane/$name" && {
                    printf '%s' "$name"
                    return 0
                }
                ;;
        esac
    done
    while IFS=$'\t' read -r n p; do
        case "$cwd/" in "$p"/*)
            printf '%s' "$n"
            return 0
            ;;
        esac
    done < <(external_relics)
    return 1
}

# ── published-state ─────────────────────────────────────────────────────────

inhouse_pubstate() {
    local total=0 have=0 n
    for n in "$@"; do
        total=$((total + 1))
        reg_has "$n" && have=$((have + 1))
    done
    if [[ $total -eq 0 ]]; then
        echo "no-entrypoints"
    elif [[ $have -eq 0 ]]; then
        echo "unpublished"
    elif [[ $have -lt $total ]]; then
        echo "partial"
    else echo "published"; fi
}

external_pubstate() {
    [[ -f "$REGISTRY" ]] || {
        echo "unknown"
        return
    }
    if awk -v o="$1" '/^[[:space:]]*#/{next} $2==o{f=1;exit} END{exit !f}' "$REGISTRY"; then
        echo "published"
    else
        echo "unknown"
    fi
}

state_label() {
    case "$1" in
        published) printf '%s● published%s' "$_c_grn" "$_c_rst" ;;
        partial) printf '%s◐ partial%s' "$_c_yel" "$_c_rst" ;;
        unpublished) printf '%s○ unpublished%s' "$_c_dim" "$_c_rst" ;;
        no-entrypoints) printf '%s— no entrypoints%s' "$_c_dim" "$_c_rst" ;;
        absent) printf '%s○ listed, not present%s' "$_c_dim" "$_c_rst" ;;
        unknown) printf '%s? unknown%s' "$_c_dim" "$_c_rst" ;;
        *) printf '%s' "$1" ;;
    esac
}

state_plain() {
    case "$1" in
        published) echo "yes" ;;
        partial) echo "partial" ;;
        unpublished) echo "no" ;;
        no-entrypoints) echo "n/a (no entrypoints)" ;;
        absent) echo "not present" ;;
        unknown) echo "unknown" ;;
        *) echo "$1" ;;
    esac
}

# ── scaffold helpers (pure / file; unit-tested) ─────────────────────────────

# A relic name is the published binary name: a slug of letters, digits, dash,
# underscore, not starting with a dot or dash. Rejects slashes and empties.
valid_relic_name() {
    case "$1" in
        "" | .* | -*) return 1 ;;
        *[!A-Za-z0-9_-]*) return 1 ;;
        *) return 0 ;;
    esac
}

valid_runtime() {
    case "$1" in
        python | bash | fish | rust | docker) return 0 ;;
        *) return 1 ;;
    esac
}

# The runtime a relic gets when nothing says otherwise. Relics are Rust by
# default; anything else records a RUNTIME_EXEMPTION.
DEFAULT_RUNTIME="rust"

# Add <name> to a cargo workspace manifest's members list. Idempotent, and keeps
# the list sorted so the next insert lands predictably. Assumes the multi-line
# array form, which is why the workspace manifest is written that way.
workspace_add_member() {
    local file="$1" name="$2" tmp
    [[ -f "$file" ]] || return 1
    awk -v n="\"$name\"," 'index($0, n) { f = 1 } END { exit !f }' "$file" && return 0
    tmp="$(mktemp)" || return 1
    awk -v n="$name" '
        BEGIN { inarr = 0; added = 0 }
        /^members[[:space:]]*=[[:space:]]*\[/ { inarr = 1; print; next }
        inarr && /^\]/ {
            if (!added) { printf "    \"%s\",\n", n; added = 1 }
            inarr = 0; print; next
        }
        inarr {
            if (!added) {
                entry = $0
                gsub(/[^A-Za-z0-9_\/*-]/, "", entry)
                if (entry > n) { printf "    \"%s\",\n", n; added = 1 }
            }
            print; next
        }
        { print }
    ' "$file" >"$tmp" && mv "$tmp" "$file"
}

# Lay down the cargo half of a Rust relic: a member manifest that inherits
# everything the workspace holds, a main.rs that runs, and the members entry. No
# entrypoints/ — a compiled relic declares its published names in the manifest,
# because its artifact does not exist until cargo has run.
scaffold_cargo() {
    local name="$1" dir="$2" workspace="$3"
    cat >"$dir/Cargo.toml" <<EOF
[package]
name = "$name"
version = "0.1.0"
edition.workspace = true
rust-version.workspace = true
license.workspace = true
publish.workspace = true

[[bin]]
name = "$name"
path = "src/main.rs"

[dependencies]
EOF
    cat >"$dir/src/main.rs" <<EOF
fn main() {
    println!("$name");
}
EOF
    rm -f "$dir/src/.gitkeep"
    rm -rf "$dir/entrypoints"
    [[ -f "$workspace" ]] && { workspace_add_member "$workspace" "$name" || return 1; }
    return 0
}

# Map a script's shebang to a relic RUNTIME. Emits the runtime, or nothing when
# it can't be inferred (caller then prompts / requires --runtime).
infer_runtime() {
    local script="$1" first
    [[ -r "$script" ]] || return 0
    IFS= read -r first <"$script" || true
    case "$first" in
        '#!'*) ;;
        *) return 0 ;;
    esac
    case "$first" in
        *python*) printf 'python' ;;
        *fish*) printf 'fish' ;;
        *bash*) printf 'bash' ;;
        *[/\ ]sh | *[/\ ]sh\ *) printf 'bash' ;; # /bin/sh, /usr/bin/env sh, sh -e → bash
        *) return 0 ;;
    esac
}

# Rewrite a `key = "..."` assignment in a manifest, preserving any trailing
# `# comment`. Portable (no sed -i): awk to a temp file, then mv.
#
# The value is escaped for a TOML basic string here, and reaches awk through
# the environment: `-v` runs escape processing over its argument, which would
# undo exactly the escaping this just did.
manifest_set() {
    local file="$1" key="$2" val="$3" tmp
    val="${val//\\/\\\\}"
    val="${val//\"/\\\"}"
    tmp="$(mktemp)" || return 1
    MANIFEST_VALUE="$val" awk -v k="$key" '
        $0 ~ ("^"k"[ \t]*=") {
            c = ""
            col = 0
            if (match($0, /#.*/)) { c = substr($0, RSTART); col = RSTART }
            line = sprintf("%s = \"%s\"", k, ENVIRON["MANIFEST_VALUE"])
            if (c != "") {
                # Hold the comment column the file already uses, so a filled-in
                # manifest reads like the template it came from.
                pad = col - 1 - length(line)
                if (pad < 1) pad = 1
                while (pad-- > 0) line = line " "
                line = line c
            }
            print line
            next
        }
        { print }
    ' "$file" >"$tmp" && mv "$tmp" "$file"
}

# Build a relic tree at <dir> from $TEMPLATE_DIR: fill NAME/RUNTIME, drop a
# project CLAUDE.md stub, and (when <src-script> is given) move it into src/ and
# wire the entrypoint symlink. Parameterised on <dir>/$TEMPLATE_DIR so tests can
# drive it against scratch dirs.
scaffold_tree() {
    local name="$1" dir="$2" runtime="$3" src_script="${4:-}" exempt="${5:-}" workspace="${6:-}"
    cp -r "$TEMPLATE_DIR" "$dir" || return 1
    manifest_set "$dir/relic.toml" name "$name" || return 1
    manifest_set "$dir/relic.toml" runtime "$runtime" || return 1
    [[ -n "$exempt" ]] && { manifest_set "$dir/relic.toml" runtime-exemption "$exempt" || return 1; }
    cat >"$dir/CLAUDE.md" <<EOF
# \`$name\` — in-house (Stage-2) relic

Scaffolded from \`~/.config/reliquary/template\`. See
\`~/.config/reliquary/GRADUATION.md\` for the lifecycle, manifest schema, and
publish flow.

TODO: describe what \`$name\` does and any agent context worth keeping.
EOF
    if [[ "$runtime" == "rust" ]]; then
        # A Stage-1 script cannot be promoted into a compiled relic as-is; it is
        # kept beside the skeleton as the thing to port, and the caller says so.
        if [[ -n "$src_script" ]]; then
            mv "$src_script" "$dir/src/$name.port-me" || return 1
        fi
        scaffold_cargo "$name" "$dir" "${workspace:-$RELICS_LANE/Cargo.toml}" || return 1
        return 0
    fi

    if [[ -n "$src_script" ]]; then
        mv "$src_script" "$dir/src/$name" || return 1
        chmod +x "$dir/src/$name"
        rm -f "$dir/src/.gitkeep" "$dir/entrypoints/.gitkeep"
        ln -s "../src/$name" "$dir/entrypoints/$name" || return 1
    fi
}

# ── commands ────────────────────────────────────────────────────────────────

cmd_list() {
    local name dir lane rt why eps state any all

    # One read for both lanes; the sections below are two views of it.
    all="$(inhouse_relics)"

    info "${_c_bold}In-house relics${_c_rst} ${_c_dim}(~/.config/relics)${_c_rst}"
    any=0
    while IFS=$'\037' read -r name dir lane rt why eps; do
        [[ "$lane" == "$RELICS_LANE" ]] || continue
        any=1
        printf '  %-18s %-7s %s\n' "$name" "$rt" \
            "$(state_label "$(inhouse_pubstate $eps)")"
    done <<<"$all"
    [[ $any -eq 0 ]] && printf '  %s(none)%s\n' "$_c_dim" "$_c_rst"

    local priv
    priv="$(printf '%s\n' "$all" | awk -F'\037' -v l="$ATTIC_LANE" '$3==l')"
    if [[ -n "$priv" ]]; then
        info ""
        info "${_c_bold}Private relics${_c_rst} ${_c_dim}(~/.config/attic)${_c_rst}"
        while IFS=$'\037' read -r name dir lane rt why eps; do
            printf '  %-18s %-7s %s\n' "$name" "$rt" \
                "$(state_label "$(inhouse_pubstate $eps)")"
        done <<<"$priv"
    fi

    local ext
    ext="$(external_relics)"
    if [[ -n "$ext" ]]; then
        info ""
        info "${_c_bold}External relics${_c_rst} ${_c_dim}(Stage 3; per GRADUATION.md)${_c_rst}"
        while IFS=$'\t' read -r name dir; do
            if [[ -d "$dir" ]]; then state="$(external_pubstate "$name")"; else state="absent"; fi
            printf '  %-18s %-7s %s\n' "$name" "ext" "$(state_label "$state")"
        done <<<"$ext"
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
    local name="$1" dir="$2" n owner out eps=()
    _load_relic_lib
    relic::load_manifest "$dir" || return 1
    while IFS= read -r n; do
        [[ -n "$n" ]] && eps=(${eps[@]+"${eps[@]}"} "$n")
    done < <(entrypoints_of "$dir" "$RUNTIME" "$NAME" \
        ${ENTRYPOINTS[@]+"${ENTRYPOINTS[@]}"})

    info "${_c_bold}$name${_c_rst} ${_c_dim}$dir${_c_rst}"
    info "  stage:     2 (in-house)"
    info "  runtime:   $RUNTIME"
    info "  published: $(state_plain "$(inhouse_pubstate ${eps[@]+"${eps[@]}"})")"
    for n in ${eps[@]+"${eps[@]}"}; do
        if reg_has "$n"; then
            owner="$(reg_owner "$n")"
            info "    - $n → ~/.local/bin/$n${owner:+ ${_c_dim}(owner: $owner)${_c_rst}}"
        else
            info "    - $n → ${_c_dim}not on PATH${_c_rst}"
        fi
    done
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
cmd_update() { _run_op update "$@"; }
cmd_mutants() { _run_op mutants "$@"; }

# `test` is the fast loop and must stay fast — agents route around slow
# commands. `--cover` is the deliberate slow-gate variant, never a default.
cmd_test() {
    local op=test a
    local args=()
    for a in "$@"; do
        case "$a" in
            --cover) op=cover ;;
            *) args+=("$a") ;;
        esac
    done
    _run_op "$op" ${args+"${args[@]}"}
}

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

cmd_scaffold() {
    local name="" runtime="" exempt=""
    while [[ $# -gt 0 ]]; do
        case "$1" in
            -r | --runtime)
                runtime="${2:-}"
                shift 2 || die "missing value for $1"
                ;;
            -e | --exempt)
                exempt="${2:-}"
                shift 2 || die "missing value for $1"
                ;;
            -*) die "unknown flag: $1" ;;
            *)
                [[ -z "$name" ]] || die "unexpected extra argument: $1"
                name="$1"
                shift
                ;;
        esac
    done

    [[ -n "$name" ]] || die "scaffold: missing <name>"
    valid_relic_name "$name" || die "invalid relic name: $name (use letters, digits, dash, underscore)"
    [[ -z "$runtime" ]] || valid_runtime "$runtime" || die "invalid runtime: $runtime (one of python|bash|fish|rust|docker)"

    if find_inhouse_dir "$name" >/dev/null || [[ -e "$RELICS_LANE/$name" ]]; then
        die "a relic named '$name' already exists"
    fi
    local dir="$RELICS_LANE/$name"

    # Promotion source: an existing Stage-1 one-shot at ~/.config/bin/<name>.
    local src=""
    [[ -f "$BIN_LANE/$name" ]] && src="$BIN_LANE/$name"

    # Resolve RUNTIME: explicit flag → shebang of a Stage-1 source → the default.
    # Inference reads what the promoted script *is*, so it is not overridden by
    # the stance; a rewrite is a deliberate `-r rust`, not a silent one that
    # leaves the old script unrunnable.
    if [[ -z "$runtime" && -n "$src" ]]; then
        runtime="$(infer_runtime "$src")"
        [[ -n "$runtime" ]] && info "${_c_dim}inferred runtime '$runtime' from $src${_c_rst}"
    fi
    if [[ -z "$runtime" ]]; then
        runtime="$DEFAULT_RUNTIME"
        info "${_c_dim}runtime '$runtime' (the default — relics are Rust unless exempted)${_c_rst}"
    fi

    # The stance, enforced at the one moment it is cheap to follow.
    if [[ "$runtime" != "rust" && -z "$exempt" ]]; then
        die "runtime '$runtime' needs a reason: pass --exempt \"<why this one is not Rust>\" (see GRADUATION.md)"
    fi

    scaffold_tree "$name" "$dir" "$runtime" "$src" "$exempt" "$RELICS_LANE/Cargo.toml" ||
        die "failed to scaffold $dir"
    info "${_c_grn}scaffolded${_c_rst} $dir ${_c_dim}(runtime: $runtime)${_c_rst}"

    if [[ "$runtime" == "rust" ]]; then
        info "${_c_dim}added to the workspace members in $RELICS_LANE/Cargo.toml${_c_rst}"
        info ""
        info "next steps:"
        info "  1. write $dir/src/main.rs"
        if [[ -n "$src" ]]; then
            info "     ${_c_yel}$src moved to src/$name.port-me${_c_rst} — port it, then delete it"
        fi
        info "  2. relic test $name"
        info "  3. relic publish $name"
        info "${_c_dim}staging is left until there's something publishable.${_c_rst}"
        return 0
    fi

    if [[ -n "$src" ]]; then
        info "promoted ${_c_bold}$name${_c_rst} from $src → src/$name"
        _load_relic_lib
        relic::publish "$dir" || die "publish failed for $name"
        _stage_in_yadm "$src" "$dir"
        info ""
        cmd_status "$name"
    else
        info ""
        info "next steps ${_c_dim}(fresh relic — no Stage-1 source found)${_c_rst}:"
        info "  1. add your executable under $dir/src/"
        info "  2. ln -s ../src/<file> $dir/entrypoints/$name"
        info "  3. relic publish $name"
        info "${_c_dim}staging is left until there's something publishable.${_c_rst}"
    fi
}

# Stage the scaffold result in yadm: the new tree, plus the moved Stage-1 path's
# deletion when that path was tracked. Best-effort and independent per path — a
# missing yadm, an untracked source, or a non-yadm HOME must not fail the
# scaffold, which already stands on disk. Stages only — the commit is deliberate.
_stage_in_yadm() {
    local old="$1" dir="$2" staged=0
    command -v yadm >/dev/null 2>&1 || return 0
    yadm add "$dir" 2>/dev/null && staged=1
    if yadm ls-files --error-unmatch "$old" >/dev/null 2>&1; then
        yadm add -A "$old" 2>/dev/null && staged=1 # -A records the deletion
    fi
    if [[ $staged -eq 1 ]]; then
        info "${_c_dim}staged in yadm: ${dir#"$HOME"/}${_c_rst}"
    else
        warn "could not stage in yadm; stage manually if this HOME is yadm-tracked"
    fi
}

cmd_registry() {
    if [[ "${1:-}" == "--migrate" ]]; then
        cmd_migrate
        return
    fi
    if [[ "${1:-}" == "--prune" ]]; then
        cmd_prune
        return
    fi
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

cmd_prune() {
    _load_install_on_path
    install_on_path_prune_registry
}

cmd_doctor() {
    local problems=0 any name owner ep runtime rows

    # One discovery for every question below.
    rows="$(inhouse_relics)"

    info "${_c_bold}Orphan registry entries${_c_rst} ${_c_dim}(registered, no file in ~/.local/bin)${_c_rst}"
    any=0
    while IFS=$'\t' read -r name owner; do
        [[ -z "$name" ]] && continue
        any=1
        problems=$((problems + 1))
        info "  ${_c_yel}$name${_c_rst}${owner:+ ${_c_dim}(owner: $owner)${_c_rst}}"
    done < <(doctor_orphans)
    if [[ $any -eq 0 ]]; then
        printf '  %s(none)%s\n' "$_c_grn" "$_c_rst"
    else
        info "  ${_c_dim}fix: relic registry --prune${_c_rst}"
    fi

    info ""
    info "${_c_bold}Unpublished entrypoints${_c_rst} ${_c_dim}(declared by a relic, not in registry)${_c_rst}"
    any=0
    while IFS=$'\t' read -r name ep; do
        [[ -z "$ep" ]] && continue
        any=1
        problems=$((problems + 1))
        info "  ${_c_yel}$ep${_c_rst} ${_c_dim}($name)${_c_rst}"
    done < <(doctor_unpublished <<<"$rows")
    if [[ $any -eq 0 ]]; then
        printf '  %s(none)%s\n' "$_c_grn" "$_c_rst"
    else
        info "  ${_c_dim}fix: relic publish <relic>${_c_rst}"
    fi

    info ""
    info "${_c_bold}Runtime stance${_c_rst} ${_c_dim}(not rust, no runtime-exemption — informational)${_c_rst}"
    any=0
    while IFS=$'\t' read -r name runtime; do
        [[ -z "$name" ]] && continue
        any=1
        info "  ${_c_dim}$name ($runtime)${_c_rst}"
    done < <(doctor_runtime_stance <<<"$rows")
    if [[ $any -eq 0 ]]; then
        printf '  %s(none)%s\n' "$_c_grn" "$_c_rst"
    else
        info "  ${_c_dim}rewrite into the workspace, or record why not in the manifest${_c_rst}"
    fi

    info ""
    info "${_c_bold}Unmanaged files${_c_rst} ${_c_dim}(in ~/.local/bin, not in registry — informational)${_c_rst}"
    any=0
    while IFS= read -r name; do
        [[ -z "$name" ]] && continue
        any=1
        info "  ${_c_dim}$name${_c_rst}"
    done < <(doctor_unmanaged)
    [[ $any -eq 0 ]] && printf '  %s(none)%s\n' "$_c_grn" "$_c_rst"

    info ""
    if [[ $problems -eq 0 ]]; then
        info "${_c_grn}healthy${_c_rst} — registry, PATH lane, and entrypoints agree"
        return 0
    fi
    info "${_c_yel}$problems issue(s) found${_c_rst}"
    return 1
}

usage() {
    cat <<EOF
relic — manage Reliquary relics

usage: relic <command> [<name>]

commands:
  list                  all relics with stage, runtime, published-state
  status [<name>]       one relic's detail (deps, PATH wiring, git dirty)
  publish [<name>]      publish an in-house relic's entrypoints onto PATH
  test    [<name>] [--cover]
                        run the tests; --cover adds coverage and checks it
                        against the committed baseline
  mutants [<name>]      mutate the code and check that the tests fail — the
                        gate coverage cannot be, since a test that asserts
                        nothing kills no mutants
  update  [<name>]      update/rebuild an in-house relic
  scaffold <name> [-r <rt>] [-e <why>]
                        promote a Stage-1 ~/.config/bin util (or fresh idea)
                        into a Stage-2 relic, then publish + stage
  registry [--migrate|--prune]  show / fold / prune the shared PATH registry
  migrate               fold legacy per-meta registries into .reliquary-managed
  doctor                cross-check registry ↔ ~/.local/bin ↔ entrypoints
  help                  this message

<name> is optional for status/publish/test/update when run from inside a
relic directory. Unambiguous command prefixes are accepted (e.g. \`relic st\`).
\`scaffold\` infers RUNTIME from a promoted script's shebang and otherwise
defaults to rust — relics are Rust unless exempted. Pass \`-r/--runtime\` to
choose, and \`-e/--exempt "<why>"\` to record why a non-rust one stays that way.
EOF
}

# ── dispatch ────────────────────────────────────────────────────────────────

resolve_cmd() {
    local input="$1" c match="" n=0
    for c in $COMMANDS; do
        [[ "$c" == "$input" ]] && {
            printf '%s' "$c"
            return 0
        }
    done
    for c in $COMMANDS; do
        case "$c" in "$input"*)
            match="$c"
            n=$((n + 1))
            ;;
        esac
    done
    [[ $n -eq 1 ]] && {
        printf '%s' "$match"
        return 0
    }
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
        list) cmd_list "$@" ;;
        status) cmd_status "$@" ;;
        publish) cmd_publish "$@" ;;
        test) cmd_test "$@" ;;
        mutants) cmd_mutants "$@" ;;
        update) cmd_update "$@" ;;
        scaffold) cmd_scaffold "$@" ;;
        registry) cmd_registry "$@" ;;
        migrate) cmd_migrate "$@" ;;
        doctor) cmd_doctor "$@" ;;
        help) usage ;;
    esac
}

# Run only when executed, not when sourced (tests source this file).
if [[ "${BASH_SOURCE[0]}" == "${0}" ]]; then
    main "$@"
fi
