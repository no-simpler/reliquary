#!/usr/bin/env bash
#
# relic.sh — shared library for Stage 1+2 relic management.
#
# Stage 3 (external relics like ~/Developer/bb, ~/Developer/halo) do NOT
# depend on this lib. They publish via ~/.config/reliquary/lib/install-on-path.sh
# directly. The stable cross-stage API is install-on-path; relic.sh is the
# convenience layer for in-house relics.
#
# Usage:
#   source "$HOME/.config/reliquary/lib/relic.sh"
#   relic::publish ~/.config/relics/<name>
#   relic::test    ~/.config/relics/<name>
#   relic::update  ~/.config/relics/<name>
#
# Each operation reads <dir>/relic.toml, the per-relic manifest. If
# <dir>/scripts/<op>.sh exists and is executable, it is run instead of the
# default behavior — relics override only when they need to.
#
# Manifest schema, TOML keys and the bash names they carry:
#   name                  NAME                  required; published name + META_NAME
#   description           DESCRIPTION           optional; one-line summary
#   runtime               RUNTIME               required; rust by default, see the stance
#   runtime-exemption     RUNTIME_EXEMPTION     required when runtime is not rust; why not
#   min-runtime-version   MIN_RUNTIME_VERSION   optional; semver-ish, enforced at publish
#   entrypoints           ENTRYPOINTS           optional; published names, compiled relics
#   brew-deps             BREW_DEPS             optional; brew package names
#   external-deps         EXTERNAL_DEPS         optional; free-form notes (not enforced)
#   docker                DOCKER                optional; true for docker-run shims
#
# Runtime stance: relics are Rust by default. Any other RUNTIME records why in
# RUNTIME_EXEMPTION. Nothing here refuses to publish over it — `relic doctor`
# reports the omission, because a relic awaiting its rewrite must keep working.
#
# Publishing splits on whether the artifact exists before a build:
#
#   interpreted (python|bash|fish|docker) — <dir>/entrypoints/<published-name>
#       is the source, typically a symlink into <dir>/src/. The filename
#       literally is the published name. install_on_path copies the file (cp
#       follows symlinks) into ~/.local/bin/.
#
#   compiled (rust) — there is nothing on disk to publish until cargo has run,
#       and the artifact lands in the workspace target/ rather than beside the
#       source. So the published names are declared in ENTRYPOINTS (defaulting
#       to NAME) and resolved against <workspace>/target/release/. No
#       entrypoints/ directory is involved: a symlink into an unbuilt target/
#       dangles on a fresh clone, which is what every Rust relic used to
#       override publish to work around.

relic::_die() {
    printf 'relic: %s\n' "$1" >&2
    return 1
}

relic::_version_ge() {
    # Return 0 if version $1 >= $2 (dotted-numeric, sort -V semantics).
    local have="$1" need="$2"
    [[ "$(printf '%s\n%s\n' "$need" "$have" | sort -V | head -1)" == "$need" ]]
}

# Where a relic's manifest is. Nothing else in the tree may test for one by
# name: a second predicate is how one lane comes to disagree with another about
# which directories are relics at all.
relic::manifest_path() {
    local dir="${1:-}"
    [[ -r "$dir/relic.toml" ]] || return 1
    printf '%s' "$dir/relic.toml"
}

relic::has_manifest() {
    relic::manifest_path "${1:-}" >/dev/null
}

# Manifests for many directories as one eval-able record stream, in no
# particular order: it is a lookup table, and the orders callers want they
# already have. Directories with no manifest yield nothing — whether that is an
# error is the caller's to say.
#
# Batched because an interpreter start costs more than every read that follows
# it, and `relic list` reads each manifest more than once.
relic::_manifest_read() {
    local dir found=()
    for dir in "$@"; do
        relic::has_manifest "$dir" && found=(${found[@]+"${found[@]}"} "$dir")
    done
    [[ ${#found[@]} -eq 0 ]] && return 0
    python3 "$HOME/.config/reliquary/lib/manifest.py" "${found[@]}"
}

# One manifest, into the caller's scope. Fields are always assigned, so a value
# from a previous relic cannot survive into this one.
#
# shellcheck disable=SC2034  # the manifest fields are this function's output;
# every one is read by callers after it returns.
relic::load_manifest() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || {
        relic::_die "load_manifest: missing dir"
        return $?
    }
    local manifest
    manifest="$(relic::manifest_path "$dir")" || {
        relic::_die "no manifest at $dir/relic.toml"
        return $?
    }

    __RELIC_ERROR=""
    local record rc
    record="$(relic::_manifest_read "$dir")"
    rc=$?
    # A reader that died says nothing about the manifest. Letting its silence
    # fall through blames the data for the parser's fault: the record is empty,
    # so NAME is unset, and the caller reports "manifest missing name" — or,
    # where the caller runs under `set -u`, dies on an unbound variable. One
    # cause, two errors, neither of them true.
    [[ $rc -eq 0 ]] || {
        relic::_die "manifest reader failed (exit $rc): $manifest"
        return $?
    }
    eval "$record"

    [[ -z "$__RELIC_ERROR" ]] || {
        relic::_die "$__RELIC_ERROR"
        return $?
    }
    [[ -n "$NAME" ]] || {
        relic::_die "manifest missing name: $manifest"
        return $?
    }
    [[ -n "$RUNTIME" ]] || {
        relic::_die "manifest missing runtime: $manifest"
        return $?
    }
}

relic::check_deps() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || {
        relic::_die "check_deps: missing dir"
        return $?
    }
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
                    printf 'relic[%s]: python3 not on PATH\n' "$NAME" >&2
                    fail=1
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
                    printf 'relic[%s]: rustc not on PATH\n' "$NAME" >&2
                    fail=1
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
                    printf 'relic[%s]: fish not on PATH\n' "$NAME" >&2
                    fail=1
                fi
                ;;
            docker)
                if ! command -v docker >/dev/null 2>&1; then
                    printf 'relic[%s]: docker not on PATH\n' "$NAME" >&2
                    fail=1
                fi
                ;;
        esac
    fi

    return "$fail"
}

# The cargo workspace root above <dir>. `cargo locate-project` is the only
# thing that knows, and asking it beats hardcoding a depth that a relocated lane
# would silently invalidate.
relic::_cargo_workspace_root() {
    local dir="${1:-}" manifest
    manifest="$(cd "$dir" 2>/dev/null && cargo locate-project --workspace --message-format plain 2>/dev/null)"
    [[ -n "$manifest" ]] || return 1
    dirname "$manifest"
}

# Package names of the workspace's shared crates, one per line. Read from each
# manifest rather than assumed from the directory name, and the first `name =`
# in a manifest is [package]'s by construction.
relic::_shared_crates() {
    local root="${1:-}" toml
    for toml in "$root"/crates/*/Cargo.toml; do
        [[ -f "$toml" ]] || continue
        sed -n 's/^name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$toml" | head -1
    done
}

# Where a compiled relic's built binary lands. For the rare scripts/update.sh
# that has to run the thing it just published.
relic::rust_binary() {
    local dir="${1:-}" name="${2:-}" root
    root="$(relic::_cargo_workspace_root "$dir")" || return $?
    printf '%s' "$root/target/release/$name"
}

# The published names of a compiled relic: ENTRYPOINTS, or NAME when it is
# silent. Requires a loaded manifest.
relic::_published_names() {
    if [[ ${#ENTRYPOINTS[@]} -gt 0 ]]; then
        printf '%s\n' "${ENTRYPOINTS[@]}"
    else
        printf '%s\n' "$NAME"
    fi
}

relic::_publish_compiled() {
    local dir="${1:-}" root n
    root="$(relic::_cargo_workspace_root "$dir")" || {
        relic::_die "no cargo workspace above $dir"
        return $?
    }

    # Unconditionally, not only when the binary is missing: guarding on absence
    # would publish whatever was built last, shipping a source change as a stale
    # binary. cargo is incremental, so an up-to-date tree makes this a no-op.
    printf 'relic[%s]: building\n' "$NAME"
    (cd "$dir" && cargo build --release --quiet) || return $?

    local names=()
    while IFS= read -r n; do
        [[ -n "$n" ]] && names=("${names[@]}" "$n")
    done < <(relic::_published_names)

    (
        export META_NAME="$NAME"
        # shellcheck disable=SC1091
        source "$HOME/.config/reliquary/lib/install-on-path.sh" || exit $?
        for n in "${names[@]}"; do
            install_on_path "$root/target/release/$n" "$n" || exit $?
        done
    )
}

relic::publish() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || {
        relic::_die "publish: missing dir"
        return $?
    }

    if [[ -x "$dir/scripts/publish.sh" ]]; then
        (cd "$dir" && ./scripts/publish.sh)
        return $?
    fi

    relic::check_deps "$dir" || return $?

    if [[ "$RUNTIME" == "rust" ]]; then
        relic::_publish_compiled "$dir"
        return $?
    fi

    local entrypoints_dir="$dir/entrypoints"
    if [[ ! -d "$entrypoints_dir" ]]; then
        printf 'relic[%s]: no entrypoints/ directory; nothing to publish\n' "$NAME" >&2
        return 0
    fi

    local name_for_meta="$NAME"

    (
        export META_NAME="$name_for_meta"
        # shellcheck disable=SC1091
        source "$HOME/.config/reliquary/lib/install-on-path.sh" || exit $?

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

# Format and lint one relic's shell. Absent, the relic is unlinted and says so
# rather than passing quietly — a gate that silently does nothing is worse than
# no gate, because it also carries the belief that it is on.
relic::_shell_lint() {
    local dir="${1:-}" linter="$HOME/.config/bin/check-shell-lint"
    if [[ ! -x "$linter" ]]; then
        printf 'relic[%s]: check-shell-lint not found — shell unlinted\n' "$NAME" >&2
        return 0
    fi
    "$linter" "$dir"
}

relic::test() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || {
        relic::_die "test: missing dir"
        return $?
    }

    if [[ -x "$dir/scripts/test.sh" ]]; then
        (cd "$dir" && ./scripts/test.sh)
        return $?
    fi

    relic::load_manifest "$dir" || return $?

    # A compiled relic's unit tests live beside the source, so tests/ is not the
    # thing that decides whether there is anything to run — and format and lint
    # are worth running either way.
    if [[ "$RUNTIME" == "rust" ]]; then
        relic::_test_compiled "$dir"
        return $?
    fi

    # The bash branch's format-and-lint station, ahead of the suite for the same
    # reason the rust branch runs fmt first: cheapest gate reports first. Bash
    # has no type system, so this is the whole of what can be verified
    # statically. See "Track 2" in ~/.config/reliquary/HARDENING.md.
    if [[ "$RUNTIME" == "bash" ]]; then
        relic::_shell_lint "$dir" || return $?
    fi

    local tests_dir="$dir/tests"
    if [[ ! -d "$tests_dir" ]]; then
        printf 'relic[%s]: no tests/ directory; nothing to run\n' "$NAME"
        return 0
    fi

    case "$RUNTIME" in
        python)
            if command -v pytest >/dev/null 2>&1; then
                (cd "$dir" && pytest tests/)
            else
                (cd "$dir" && python3 -m unittest discover tests/)
            fi
            ;;
        bash)
            if [[ -x "$tests_dir/run.sh" ]]; then
                (cd "$dir" && ./tests/run.sh)
            else
                local fail=0 t
                for t in "$tests_dir"/*.sh; do
                    [[ -f "$t" ]] || continue
                    bash "$t" || fail=1
                done
                return "$fail"
            fi
            ;;
        *)
            printf 'relic[%s]: no default test runner for RUNTIME=%s\n' "$NAME" "$RUNTIME"
            return 0
            ;;
    esac
}

# The lint ratchet. A suppression is the cheapest repair an agent reaches for
# under a lint it cannot satisfy, and it is invisible in every other measure —
# an #[allow] makes the warning count fall. So the count is committed, and
# raising it is an edit in the same commit as the suppression it accounts for.
#
# Checked over the whole workspace on every `relic test`, not just the relic
# named: a suppression slipped into a package nobody tested is exactly the one
# a per-relic check would miss.
relic::_allow_ratchet() {
    local root="${1:-}" baseline="${1:-}/ratchets/allows.toml"
    [[ -f "$baseline" ]] || return 0

    local fail=0 pkgdir pkg have want
    for pkgdir in "$root"/*/ "$root"/crates/*/; do
        [[ -f "$pkgdir/Cargo.toml" ]] || continue
        pkg="$(sed -n 's/^name[[:space:]]*=[[:space:]]*"\([^"]*\)".*/\1/p' "$pkgdir/Cargo.toml" | head -1)"
        [[ -n "$pkg" ]] || continue

        have="$(find "$pkgdir" -name '*.rs' -not -path '*/target/*' -not -path '*/fixtures/*' \
            -exec grep -hoE '#!?\[(allow|expect)\(' {} + 2>/dev/null | wc -l | tr -d ' ')"
        want="$(awk -v p="$pkg" -F' *= *' '$1 == p { gsub(/[^0-9]/, "", $2); print $2; exit }' "$baseline")"

        if [[ -z "$want" ]]; then
            printf 'lint ratchet: %s has no baseline in %s\n' "$pkg" "$baseline" >&2
            fail=1
        elif [[ "$have" -gt "$want" ]]; then
            printf 'lint ratchet: %s has %s suppressions, baseline %s\n' "$pkg" "$have" "$want" >&2
            printf '  fix the lint, or raise the baseline in the same commit as the suppression\n' >&2
            fail=1
        elif [[ "$have" -lt "$want" ]]; then
            printf 'lint ratchet: %s is down to %s suppressions (baseline %s) — lower it in %s\n' \
                "$pkg" "$have" "$want" "$baseline" >&2
            fail=1
        fi
    done
    return "$fail"
}

# The gate for a compiled relic: format, lint, suite — fail-fast in ascending
# cost, so the cheapest station reports first. Every shared crate is included, so
# code that moved into crates/ is covered from each of its dependents rather than
# by nothing: a path dependency outside the workspace is linted by no one, which
# is the whole reason the lane is a workspace.
relic::_test_compiled() {
    local dir="${1:-}" root c shared
    root="$(relic::_cargo_workspace_root "$dir")" || {
        relic::_die "no cargo workspace above $dir"
        return $?
    }

    local pkgs=(-p "$NAME") shared=0
    while IFS= read -r c; do
        [[ -n "$c" ]] && pkgs=("${pkgs[@]}" -p "$c") && shared=1
    done < <(relic::_shared_crates "$root")

    (
        cd "$dir" || exit $?

        if ! cargo fmt "${pkgs[@]}" --check; then
            printf '\nformatting: run `cargo fmt --all`\n' >&2
            exit 1
        fi

        relic::_allow_ratchet "$root" || exit $?

        # No `-D warnings`: a command-line group flag outranks every entry in
        # [workspace.lints] and collapses `warn` and `deny` into one level. The
        # table denies what this flag used to, and carries the transitional
        # lints at `warn` — policy in a committed file, not in an invocation.
        cargo clippy "${pkgs[@]}" --all-targets --all-features || exit $?

        if command -v cargo-nextest >/dev/null 2>&1; then
            cargo nextest run "${pkgs[@]}"
        else
            cargo test "${pkgs[@]}"
        fi
    ) || return $?

    [[ $shared -eq 1 ]] && relic::_test_attic_lane
}

# The reverse cross-lane gate.
#
# A shared crate is covered from each of its public dependents by the `-p` set
# above. Its *private* dependents are covered by nothing: that is the workspace
# property a lane boundary cannot carry, and the encrypted lane is a second
# workspace precisely because a member's name and version land in a lockfile.
# So when a public relic's run covered a shared crate, run the attic lane's own
# format, lints and suite as well.
#
# Silent no-op when the attic is absent or holds no member — the pattern the
# publish snippet, `up` and `relic list` already use. It names no attic relic:
# it references only the lane, which is already public knowledge. A failure
# prints attic names to the terminal; that is local, and must never be
# redirected into a tracked file.
relic::_test_attic_lane() {
    local attic="$HOME/.config/attic" manifest populated=0
    [[ -r "$attic/Cargo.toml" ]] || return 0
    for manifest in "$attic"/*/Cargo.toml; do
        [[ -r "$manifest" ]] && populated=1
    done
    # cargo refuses a memberless virtual workspace, so an ungated step would
    # fail every public relic's tests until the lane holds its first member.
    [[ $populated -eq 1 ]] || return 0

    printf 'relic[%s]: shared crate changed — gating the private lane\n' "$NAME"
    (
        cd "$attic" || exit $?
        cargo fmt --all --check || exit $?
        cargo clippy --workspace --all-targets --all-features || exit $?
        # An untested private relic is its own `relic test`'s finding, not a
        # reason to fail the public relic that triggered this step.
        if command -v cargo-nextest >/dev/null 2>&1; then
            exec cargo nextest run --workspace --no-tests=pass
        fi
        exec cargo test --workspace
    )
}

# Coverage, the slow gate. `relic test` must stay fast because agents route
# around slow commands, so this is a separate, deliberate invocation.
#
# Coverage alone is gameable by exactly the behaviour it guards against — a test
# that executes a line and asserts nothing scores the same as a real one — so it
# is one of two gates, `relic mutants` being the other and the real one.
relic::cover() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || {
        relic::_die "cover: missing dir"
        return $?
    }
    relic::load_manifest "$dir" || return $?
    [[ "$RUNTIME" == "rust" ]] || {
        relic::_die "cover: only rust relics ($NAME is $RUNTIME)"
        return $?
    }
    command -v cargo-llvm-cov >/dev/null 2>&1 || {
        relic::_die "cover: cargo-llvm-cov not installed (see ~/.config/cargo/crates.txt)"
        return $?
    }

    local root
    root="$(relic::_cargo_workspace_root "$dir")" || {
        relic::_die "no cargo workspace above $dir"
        return $?
    }

    # The whole workspace, not the named relic: the ratchet holds one baseline per
    # package, and a profile collected from one relic's run reports every other
    # package as uncovered. It also covers a shared crate from each of its
    # dependents rather than from nobody, which is the point of `crates/`.
    (
        cd "$root" || exit $?
        if command -v cargo-nextest >/dev/null 2>&1; then
            cargo llvm-cov nextest --workspace --summary-only
        else
            cargo llvm-cov --workspace --summary-only
        fi
    ) || return $?

    relic::_coverage_ratchet "$root"
}

# The coverage ratchet. Inert until the baselines are committed — a baseline
# computed on the fly is not a ratchet, it is a moving target.
#
# The comparison is cargo-llvm-cov's own `--fail-under-regions`, reading the
# profile data the run above already collected. Never a percentage scraped out
# of the summary table: an exit code is the machine-readable interface, and a
# table is human-facing output.
#
# Regions, not lines. llvm-cov counts regions, so every `?` and match arm shows
# the partial coverage that line counting hides.
relic::_coverage_ratchet() {
    local root="${1:-}" baseline="${1:-}/ratchets/coverage.toml"
    if [[ ! -f "$baseline" ]]; then
        printf 'coverage ratchet: no baseline at %s — reported, not gated\n' "$baseline"
        return 0
    fi

    local fail=0 line pkg want
    while IFS= read -r line; do
        pkg="$(printf '%s' "${line%%=*}" | tr -d ' "')"
        want="$(printf '%s' "${line#*=}" | tr -cd '0-9.')"
        [[ -n "$pkg" && -n "$want" ]] || continue
        if ! (cd "$root" && cargo llvm-cov report -p "$pkg" \
            --summary-only --fail-under-regions "$want" >/dev/null 2>&1); then
            printf 'coverage ratchet: %s is under its baseline of %s%% regions\n' "$pkg" "$want" >&2
            fail=1
        fi
    done < <(grep -E '^[^#[:space:]].*=' "$baseline")
    return "$fail"
}

# Mutation testing: the real assertion-quality gate. It mutates the code and
# checks whether tests *fail* — an assertion-free test kills no mutants.
relic::mutants() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || {
        relic::_die "mutants: missing dir"
        return $?
    }
    relic::load_manifest "$dir" || return $?
    [[ "$RUNTIME" == "rust" ]] || {
        relic::_die "mutants: only rust relics ($NAME is $RUNTIME)"
        return $?
    }
    command -v cargo-mutants >/dev/null 2>&1 || {
        relic::_die "mutants: cargo-mutants not installed (see ~/.config/cargo/crates.txt)"
        return $?
    }
    (cd "$dir" && cargo mutants --package "$NAME" "${@:2}")
}

relic::update() {
    local dir="${1:-}"
    [[ -n "$dir" ]] || {
        relic::_die "update: missing dir"
        return $?
    }

    if [[ -x "$dir/scripts/update.sh" ]]; then
        (cd "$dir" && ./scripts/update.sh)
        return $?
    fi

    relic::load_manifest "$dir" || return $?

    case "$RUNTIME" in
        # publish builds, so there is nothing to do first.
        rust) relic::publish "$dir" ;;
        *) return 0 ;;
    esac
}
