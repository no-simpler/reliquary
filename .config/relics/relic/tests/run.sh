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

rm -rf "$tmp"

printf '\n%d passed, %d failed\n' "$pass" "$fail"
[[ $fail -eq 0 ]]
