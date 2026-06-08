#!/usr/bin/env bash
#
# Tests for the `relic` CLI. Sources src/relic.sh (which self-guards against
# running main when sourced) and exercises the pure helpers against fixtures.
# Picked up by `relic::test` (bash runner looks for tests/run.sh).

set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck disable=SC1091
source "$ROOT/src/relic.sh"

pass=0
fail=0
check() {
    local desc="$1" got="$2" want="$3"
    if [[ "$got" == "$want" ]]; then
        pass=$((pass + 1))
    else
        fail=$((fail + 1))
        printf 'FAIL: %s\n  got:  %q\n  want: %q\n' "$desc" "$got" "$want" >&2
    fi
}

# ── command prefix resolution ───────────────────────────────────────────────
check "exact list"        "$(resolve_cmd list)"     "list"
check "prefix st→status"  "$(resolve_cmd st)"       "status"
check "prefix pub→publish" "$(resolve_cmd pub)"     "publish"
check "single l→list"     "$(resolve_cmd l)"        "list"
check "single r→registry" "$(resolve_cmd r)"        "registry"
check "single m→migrate"  "$(resolve_cmd m)"        "migrate"
check "single u→update"   "$(resolve_cmd u)"        "update"
check "single h→help"     "$(resolve_cmd h)"        "help"
if resolve_cmd zz >/dev/null 2>&1; then check "unknown rejected" "ok" "rejected"; else check "unknown rejected" "rejected" "rejected"; fi

# ── registry parsing ────────────────────────────────────────────────────────
tmp="$(mktemp -d)"
REGISTRY="$tmp/.reliquary-managed"
printf '# comment\n\nbiogen\tbb\ntess\thalo\nownerless\n' > "$REGISTRY"
if reg_has biogen;  then check "reg_has biogen" yes yes;  else check "reg_has biogen" no yes; fi
if reg_has nope;    then check "reg_has nope"  yes no;    else check "reg_has nope"  no no; fi
check "reg_owner biogen"    "$(reg_owner biogen)"    "bb"
check "reg_owner ownerless" "$(reg_owner ownerless)" ""
check "reg_owner missing"   "$(reg_owner missing)"   ""
check "external_pubstate by owner" "$(external_pubstate halo)" "published"
check "external_pubstate unknown"  "$(external_pubstate nobody)" "unknown"

# ── GRADUATION.md external parsing ──────────────────────────────────────────
GRADUATION="$tmp/GRADUATION.md"
cat > "$GRADUATION" <<'MD'
## Stages
intro text
### Known external relics

- `bb`   — `~/Developer/bb/`   — github.com/decaland/bb-meta
- `halo` — `~/Developer/halo/` — local-only

Append to this list when you promote a relic to Stage 3.

## In-house relic anatomy
- `not-a-relic` — `~/elsewhere/` — should not be parsed
MD
ext="$(external_relics)"
check "external bb path"   "$(printf '%s\n' "$ext" | awk -F'\t' '$1=="bb"{print $2}')"   "$HOME/Developer/bb"
check "external halo path" "$(printf '%s\n' "$ext" | awk -F'\t' '$1=="halo"{print $2}')" "$HOME/Developer/halo"
check "external stops at next header" "$(printf '%s\n' "$ext" | grep -c 'not-a-relic')" "0"
check "external count" "$(printf '%s\n' "$ext" | grep -c .)" "2"

# ── attic safety: undecrypted/unreadable manifests are never surfaced ───────
RELICS_LANE="$tmp/relics"
ATTIC_LANE="$tmp/attic"
mkdir -p "$RELICS_LANE/pub" "$ATTIC_LANE/secret"
printf 'NAME="pub"\nRUNTIME="bash"\n'    > "$RELICS_LANE/pub/relic.sh"
printf 'NAME="secret"\nRUNTIME="bash"\n' > "$ATTIC_LANE/secret/relic.sh"
chmod 000 "$ATTIC_LANE/secret/relic.sh"   # simulate undecrypted/unreadable
listing="$(inhouse_relics)"
check "public relic surfaced"     "$(printf '%s\n' "$listing" | awk -F'\t' '$1=="pub"{print "y"}')"    "y"
check "unreadable attic hidden"   "$(printf '%s\n' "$listing" | awk -F'\t' '$1=="secret"{print "y"}')" ""
chmod 644 "$ATTIC_LANE/secret/relic.sh"
listing="$(inhouse_relics)"
check "readable attic surfaced"   "$(printf '%s\n' "$listing" | awk -F'\t' '$1=="secret"{print "y"}')" "y"

# ── state labels ────────────────────────────────────────────────────────────
check "state_plain published" "$(state_plain published)" "yes"
check "state_plain unpublished" "$(state_plain unpublished)" "no"

# ── doctor: registry ↔ ~/.local/bin ↔ entrypoints drift ─────────────────────
LOCAL_BIN="$tmp/bin"
REGISTRY="$tmp/bin/.reliquary-managed"
RELICS_LANE="$tmp/drelics"
ATTIC_LANE="$tmp/dattic"          # absent dir → inhouse_relics skips it
mkdir -p "$LOCAL_BIN" "$RELICS_LANE/tool"
printf '# header\n\nalive\towner1\ndead\towner2\ntool\trelicowner\n' > "$REGISTRY"
: > "$LOCAL_BIN/alive";   chmod +x "$LOCAL_BIN/alive"
: > "$LOCAL_BIN/tool";    chmod +x "$LOCAL_BIN/tool"
: > "$LOCAL_BIN/foreign"; chmod +x "$LOCAL_BIN/foreign"   # on PATH lane, unregistered
# relic 'tool' declares two entrypoints; only 'tool' is registered, 'extra' is not
printf 'NAME="tool"\nRUNTIME="bash"\n' > "$RELICS_LANE/tool/relic.sh"
mkdir -p "$RELICS_LANE/tool/entrypoints"
: > "$RELICS_LANE/tool/entrypoints/tool"
: > "$RELICS_LANE/tool/entrypoints/extra"

check "doctor_orphans flags dead"   "$(doctor_orphans | awk -F'\t' '$1=="dead"{print $2}')"  "owner2"
check "doctor_orphans skips alive"  "$(doctor_orphans | awk -F'\t' '$1=="alive"{print "y"}')" ""
check "doctor_orphans count"        "$(doctor_orphans | grep -c .)"                            "1"
check "doctor_unpublished flags extra" "$(doctor_unpublished | awk -F'\t' '$2=="extra"{print $1}')" "tool"
check "doctor_unpublished skips tool"  "$(doctor_unpublished | awk -F'\t' '$2=="tool"{print "y"}')"  ""
check "doctor_unmanaged flags foreign" "$(doctor_unmanaged | grep -x foreign)"                 "foreign"
check "doctor_unmanaged skips registry dotfile" "$(doctor_unmanaged | grep -c '^\.')"          "0"
check "doctor_unmanaged skips registered" "$(doctor_unmanaged | grep -xc alive)"               "0"

# ── prune: install_on_path_prune_registry drops dead, keeps live + owner ────
prune_out="$(
    source "$INSTALL_ON_PATH_LIB"
    _INSTALL_ON_PATH_DIR="$LOCAL_BIN"
    _INSTALL_ON_PATH_REGISTRY="$REGISTRY"
    install_on_path_prune_registry >/dev/null
    cat "$REGISTRY"
)"
check "prune keeps alive+owner"  "$(printf '%s\n' "$prune_out" | awk '$1=="alive"{print $2}')" "owner1"
check "prune keeps tool"         "$(printf '%s\n' "$prune_out" | awk '$1=="tool"{print "y"}')"  "y"
check "prune drops dead"         "$(printf '%s\n' "$prune_out" | awk '$1=="dead"{print "y"}')"  ""
check "prune preserves comment"  "$(printf '%s\n' "$prune_out" | grep -c '^# header')"          "1"

# ── scaffold: name/runtime validation, shebang inference, manifest patch, tree ─
if valid_relic_name foo-bar_1; then check "valid name foo-bar_1" ok ok; else check "valid name foo-bar_1" no ok; fi
if valid_relic_name ".hidden";  then check "reject leading dot"  no rej; else check "reject leading dot"  rej rej; fi
if valid_relic_name "-dash";    then check "reject leading dash" no rej; else check "reject leading dash" rej rej; fi
if valid_relic_name "a/b";      then check "reject slash"        no rej; else check "reject slash"        rej rej; fi
if valid_relic_name "";         then check "reject empty"        no rej; else check "reject empty"        rej rej; fi
if valid_runtime bash;          then check "valid runtime bash"  ok ok; else check "valid runtime bash"  no ok; fi
if valid_runtime perl;          then check "reject runtime perl" no rej; else check "reject runtime perl" rej rej; fi

sdir="$(mktemp -d)"
printf '#!/usr/bin/env bash\necho hi\n'   > "$sdir/b";  check "infer bash"   "$(infer_runtime "$sdir/b")"  "bash"
printf '#!/bin/sh\necho hi\n'             > "$sdir/s";  check "infer sh→bash" "$(infer_runtime "$sdir/s")"  "bash"
printf '#!/usr/bin/env python3\nprint(1)\n' > "$sdir/p"; check "infer python" "$(infer_runtime "$sdir/p")"  "python"
printf '#!/usr/bin/env fish\necho hi\n'   > "$sdir/f";  check "infer fish"   "$(infer_runtime "$sdir/f")"  "fish"
printf 'no shebang here\n'                > "$sdir/n";  check "infer none"   "$(infer_runtime "$sdir/n")"  ""

mf="$sdir/relic.sh"
printf 'NAME=""                # x\nRUNTIME=""             # y\nDOCKER=0\n' > "$mf"
manifest_set "$mf" NAME widget
manifest_set "$mf" RUNTIME bash
( source "$mf"; printf '%s\n' "$NAME" )    > "$sdir/_name"; check "manifest NAME"    "$(cat "$sdir/_name")"    "widget"
( source "$mf"; printf '%s\n' "$RUNTIME" ) > "$sdir/_rt";   check "manifest RUNTIME" "$(cat "$sdir/_rt")"      "bash"
check "manifest keeps comment" "$(grep -c '# x' "$mf")" "1"

# scaffold_tree against a scratch template
TEMPLATE_DIR="$sdir/template"
mkdir -p "$TEMPLATE_DIR/src" "$TEMPLATE_DIR/entrypoints" "$TEMPLATE_DIR/tests"
printf 'NAME=""\nRUNTIME=""\n' > "$TEMPLATE_DIR/relic.sh"
printf 'template doc\n'        > "$TEMPLATE_DIR/CLAUDE.md"
: > "$TEMPLATE_DIR/src/.gitkeep"; : > "$TEMPLATE_DIR/entrypoints/.gitkeep"; : > "$TEMPLATE_DIR/tests/.gitkeep"

# fresh build (no src script)
mkdir -p "$sdir/relics"
fresh="$sdir/relics/fresh"
scaffold_tree fresh "$fresh" bash ""
( source "$fresh/relic.sh"; printf '%s %s\n' "$NAME" "$RUNTIME" ) > "$sdir/_fresh"
check "scaffold fresh manifest" "$(cat "$sdir/_fresh")" "fresh bash"
check "scaffold fresh CLAUDE stub" "$(grep -c 'in-house (Stage-2) relic' "$fresh/CLAUDE.md")" "1"
check "scaffold fresh keeps gitkeep" "$([[ -f "$fresh/src/.gitkeep" ]] && echo y)" "y"

# promotion (move a src script, wire entrypoint)
printf '#!/usr/bin/env bash\necho promoted\n' > "$sdir/prom-src"; chmod +x "$sdir/prom-src"
prom="$sdir/relics/prom"
scaffold_tree prom "$prom" bash "$sdir/prom-src"
check "scaffold promo src moved"   "$([[ -f "$prom/src/prom" ]] && echo y)"               "y"
check "scaffold promo source gone" "$([[ -e "$sdir/prom-src" ]] && echo present)"          ""
check "scaffold promo symlink"     "$(readlink "$prom/entrypoints/prom")"                  "../src/prom"
check "scaffold promo drops gitkeep" "$([[ -e "$prom/entrypoints/.gitkeep" ]] && echo y)"  ""
rm -rf "$sdir"

rm -rf "$tmp"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[[ $fail -eq 0 ]]
