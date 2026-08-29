# `relic` — the relic-management CLI

The first in-house (Stage-2) relic, and the last one this repository's
Rustification rewrote. It manages the relic lifecycle and **dogfoods the very
pipeline it manages**: it is published onto PATH by the same
`install-on-path.sh` rails it exposes.

For the lifecycle, stages, and registry model, see
`~/.config/reliquary/GRADUATION.md`.

## Why it was last

**It publishes everything else.** Nothing on the path from a bare machine to its
first binary may presuppose a binary, and the bootstrap seed that produces *this*
one cannot be this one. That fixed point is `~/.config/reliquary/lib/relic.sh` —
82 lines whose whole job is `cargo build` → `install_on_path` → `relic publish
--all`, reading no manifest and starting no interpreter but the shell already
running it. Everything past that hand-off is here.

## Anatomy

- `relic.toml` — manifest (`name = "relic"`, `runtime = "rust"`).
- `src/` — a library plus a thin `main.rs`. The old single-file constraint is
  gone with the interpreter: a compiled relic publishes one built artifact, so
  there is nothing for a sibling file to be missing from.
- No `entrypoints/`. A compiled relic's artifact does not exist until cargo has
  run and lands in the workspace `target/`, so its published names are declared
  in the manifest — a symlink into an unbuilt `target/` dangles on a fresh clone.
- `tests/` — a sandboxed `HOME` per test, with a deliberately bare `PATH`. One
  module builds a real cargo workspace and runs the gates over it, because that
  is the branch every binary on this machine comes through.

## The surface

**The binary is the single source of truth for its own surface** — `relic
--help`, and `relic <command> --help`. Not restated here: a hand-maintained
command list drifts from the parser with nothing to notice, which is the class
of drift this programme was about.

Two commands are worth explaining beyond their help text.

`publish --all` is what the bootstrap seed hands off to. One relic's failure
does not stop the rest, because a machine with nine relics published and one
broken is a machine you can work on.

`test` is the fast loop and must stay fast — agents route around slow commands.
`--cover` and `mutants` are the slow gates, run at wave boundaries. Coverage
alone is gameable by exactly the behaviour it guards against, which is why
mutation testing is the real one.

## Conventions

- **Attic-safe.** Private relics under `~/.config/attic/` are surfaced only when
  their manifest is *readable*. An undecrypted lane reveals nothing — not a
  name, not a count, and the private section of `list` does not print at all.
  Never add code that enumerates the lane or prints a count that leaks it.
- **A broken manifest is reported, not skipped.** Silence there is how a relic
  disappears: it gets a row saying it cannot be read, because "declares nothing"
  and "cannot be read" look identical when one of them is simply absent.
- **Reads, never writes, the registry.** All PATH and registry mutation goes
  through `install-on-path.sh`, which is a *sourced shell ABI* two external
  repositories also call from their own publish scripts. This binary shells out
  to it rather than reimplementing it — one implementation, or the lane grows
  two opinions about who owns a name. The fork is paid only on a publish.
- **One reader, one predicate.** `manifest::Manifest` is the only thing that
  parses `relic.toml` and `manifest::present` the only thing that decides a
  directory is a relic. A second predicate is how one lane comes to disagree
  with another about what a relic is.

## Deviations from `src/relic.sh`

Thirty-three CLI scenarios were captured from the shell in a hermetic `HOME`
before a line of Rust existed, and the read commands were then compared against
the live machine, where `list`, `doctor`, `registry` and `status` over all ten
relics come back byte-identical. Every remaining difference is here.

**1 — The command line is the parser's.** `help`, usage, unknown and ambiguous
subcommands, missing arguments and bad values are rendered by `clap`, so a
command-line error exits **2** where the shell exited 1 for most of them.
Prefix inference is unchanged, and an ambiguous one now names the candidates.
The retired `usage()` printed the file's own leading comment block — a
hand-maintained list that could drift from what the parser accepted.
*Pinned by* `an_unambiguous_prefix_resolves_and_an_ambiguous_one_names_the_candidates`,
`an_unknown_relic_is_a_refusal_and_an_unknown_command_is_misuse`.

**2 — A broken manifest gets a row.** The shell warned about one on stderr and
then left it out of the table the operator actually reads.
*Pinned by* `a_broken_manifest_is_reported_in_the_table_and_on_stderr`.

**3 — A scaffolded Rust relic passes its own gate.** `relic scaffold <name> &&
relic test <name>` failed — on the very step the scaffold prints as next —
because `cargo nextest` exits non-zero over a package with no tests and the
skeleton had none. Rather than teach the gate to pass an empty suite (a gate
with nothing to run is not a gate), the skeleton carries one real test.
*Pinned by* `compiled::a_compiled_relic_builds_tests_and_publishes`,
`a_fresh_skeleton_carries_a_test_so_its_own_gate_passes`.

**4 — `test` exits 1, never `assay`'s grade.** The shell returned the
`shell-lint` station's exit status verbatim, so a bash relic's test could exit 2
where a Rust relic's exited 1 — two contracts for one gate. A gate passes or it
does not, and it now says which stage refused.

**5 — One message where there were two.** The shell had `relic:` and `error:`
prefixes for the same thing, and both "unknown relic" and "unknown in-house
relic" for one condition.

**6 — Manifests are parsed, not sourced through an interpreter.** The `toml`
crate replaces a `python3` reader needing `tomllib` — the dependency that failed
every publish on a machine whose `python3` was 3.9 and reported it as a missing
manifest field. `relic list` goes from 320ms to ~3ms, and a parse failure names
its line and column from the parser's own span.

**7 — `publish --all` is new**, and is what the bootstrap seed hands off to.
*Pinned by* `publish_all_is_what_bootstrap_hands_off_to_and_one_failure_does_not_stop_it`.
