# `assay` — the machine's verification surface

Nine check surfaces and three exit conventions, hand-wired in bash inside a git
wrapper, become one roster of stations over one finding type.

`assay` is an **aggregator**, not a tool that absorbs everything. A station lives
here only when nothing else owns the check; a relic that knows its own health
keeps that knowledge and answers `doctor --format json`, which the registry
adapter collects. The dependency direction never reverses — `assay` consumes a
published protocol and never reads another repository's source.

Design: `docket` spec `7d1m`, `design-verification-surface.md`.

## The station contract

A station is a `Station` impl: an id, a title, and `check(&Context) -> Result<Outcome>`.

- **It reports; it never grades.** No station counts its own warnings, decides an
  exit status, or prints. `Grade::of` derives the verdict from the finding set,
  which is what deletes the mutable `FAILS`/`WARNS` counters `check-bedrock`'s own
  header warned about ("they mutate FAILS/WARNS via the helpers, so never call in
  `$(...)`").
- **`Severity` has no `Ok`.** A finding that says nothing is wrong is not a
  finding. The absence of findings is `Grade::Ok`.
- **`Soft` or `Broken`** by one test: does this mean the machine is no longer
  reproducible from the repo, or that something is silently disarmed? Then
  `Broken`. Degraded-but-reproducible is `Soft`.
- **`Outcome::Skipped` is a fact, not a fault** — it grades `Ok`. A station that
  cannot run says so instead of passing silently: a registered binary with no
  `doctor` subcommand, a check whose data is still encrypted.
- **`Err` means the station broke**, not that the machine did. The runner turns it
  into a `Broken` finding naming the station, so a check that throws is never a
  quiet pass.
- **Ambient authority is injected.** `Context` carries the home directory and the
  search path; a station that reads `$PATH` itself is testable only by mutating a
  process environment, which `unsafe_code = "forbid"` makes unsafe and no two
  tests can share.
- **Expensive is opt-in.** Detect-only, offline and side-effect-free unless
  `--deep`. The docker daemon is never woken by a `yadm doctor` dream pre-pass.

`Finding`, `Severity`, `Grade`, `Outcome` and `Report` are `relic_core::finding` —
platform, not local, because the same contract has to be speakable by a relic in
another process. The shape is SARIF-derived, reduced to what this machine uses.

## Inherited from `halo/alfred/scripts/verify`

The prior-art survey the design scoped. Taken:

- The **station vocabulary** itself, and the three-way outcome (pass / notice /
  findings) that `Grade` and `Outcome::Skipped` re-express as types.
- **A station error is reported, never raised.** alfred's runner surfaces no
  traceback of its own, and a station executable that does not exist yet is a
  clean station error.
- **The verdict is the last line.** A terminal block is read from the bottom up.
- **A notice is the channel for a signal worth reading and not worth gating on** —
  which is the `Soft` grade's whole justification, and why the performance-budget
  station can never redden a commit.

Deliberately **not** taken:

- **Halting at the first failure.** alfred's runner is a commit gate, where a later
  station's findings are wasted work. `assay` is a standing audit: stopping early
  hides how many problems there are. Every station runs, always.
- **Fastest-first ordering.** It only buys anything when the run halts early.
  Stations are ordered by subject.
- **Findings to a file, terminal to a digest.** alfred's evidence is large enough
  that a partial list reads as a complete one. A machine health check's findings
  are read where they are printed, the way `yadm doctor` already prints them.
- **The `--strict` advisory/gate split.** It exists because alfred's linters are
  also hand-run tools. Severity carries that distinction here.

## Deviations from `bin/check-bedrock`

Parity is on the **findings and the grade**, not on the passing chatter. Verified
across four search paths, including one that reproduces every warning class.

| deviation | why | pinned by |
| --- | --- | --- |
| A passing member prints nothing; the script printed a `✓` line with its version | inventory is not verification, and the roster line already says the station ran | `a_whole_bedrock_has_nothing_to_say` |
| `--daemon` became the run-wide `--deep` | one opt-in for everything that costs the network, a passphrase or real time, rather than one flag per station | `the_docker_daemon_is_left_asleep_unless_the_run_asks` |
| The search path is a `Context` field, not `$PATH` | the station's policy is testable against a directory a test built, with no process-environment mutation | every `bedrock` fixture test |
| A duplicate is judged by `canonicalize`, not `readlink -f` | same rule, and it does not depend on GNU coreutils being the `readlink` that wins on PATH | `one_install_reached_twice_is_not_two_installs` |
| An unknown argument exits 2; `assay` exits 3 for its own failures | 0/1/2 are graded verdicts. A tool that could not run has not verified anything, so none of the three is true | `an_unknown_station_is_refused_and_nothing_runs` |
