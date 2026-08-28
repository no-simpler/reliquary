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

## Deviations from `bin/check-md-shell-blocks`

Parity verified on a fixture home reproducing every rule, plus this machine's
own tree: the same findings, and the same exit code at all four grades.

| deviation | why | pinned by |
| --- | --- | --- |
| No root directory exists → `Outcome::Skipped`, where the script printed a note and exited 0 | a skip is a fact the run can see; a note is text nobody reads twice. Same grade | `a_machine_with_no_claude_markdown_is_skipped_not_passed` |
| The patterns compile per run instead of at module scope | a `LazyLock<Regex>` needs an `unwrap`, and a construction that cannot fail is worth spelling as one that returns `Result` when the alternative is a suppression | the station's own tests, which build `Patterns` directly |
| A grant is a `Grant::Exact` / `Grant::Prefix`, not a `(kind, text)` tuple | the two forms answer `covers` differently, and a tuple lets a caller read the wrong half | `a_grant_covers_what_its_form_says_and_no_more` |

Carried over unchanged, and worth knowing: the **empty-block check is reachable
only through a fence**. The inline pattern requires at least one character, so
``!`` `` matches nothing in either implementation and is not a block at all.

## Deviations from `bin/check-brew-health`

Parity verified against the live machine, and against fixture machines — a fake
`brew` answering from files — for the states a healthy machine cannot produce.

| deviation | why | pinned by |
| --- | --- | --- |
| A formula and a cask are separate types, not one struct carrying both identifier fields | they genuinely disagree: a formula's `name` is a string, a **cask's `name` is a list** of its titles, with `token` carrying the identifier. The shell checker never read a cask's `name`, so it never met this; the first typed draft refused the whole document and lost every other check with it | `a_cask_whose_name_is_a_list_still_parses` |
| `NOTE` became `Severity::Note`, a finding that prints and does not grade | the shell helper's fourth level had no counterpart in a two-level severity, and folding "cannot judge this" into a warning is how a gate that cannot be cleared where it fires teaches people to bypass it | `what_cannot_be_judged_is_a_note_and_never_a_verdict` |
| Brewfiles are parsed by splitting on the first quoted field, not by `grep`+`sed` per keyword | one pass over each file instead of three, and a line's keyword decides its lane instead of three patterns that can disagree | `every_scope_is_read_not_only_the_base` |
| A replacement hint is a `fix`, not a separate `NOTE` line | it is what to do about the finding it follows, which is the field that already means that | `a_deprecated_package_is_soft_and_carries_its_deadline` |
| No `HOMEBREW_NO_AUTO_UPDATE` export into the caller's environment | it is set per invocation instead, so the station cannot change what runs after it | the `Brew::command` constructor, which is the only way a `brew` runs here |

## Rebuilt, not ported: `bin/check-shell-parity`

The one A1 station with no golden master, because the design settled that
absorbing the awk scanner would carry its defects forward. Three it had, and two
of them were live in the tracked files:

| defect | what it did | pinned by |
| --- | --- | --- |
| It scanned every field of every line for the token `alias` | the comment "…so no alias is needed for the wrapper." defined a phantom alias named `is`. It passed only because the fish twin carries the same sentence, so the phantom cancelled on both sides | `prose_in_a_comment_defines_nothing` |
| It split nothing on quotes | `alias gl="glfr -10 \| less -R"` read as a body of `"glfr -10`. Invisible while only names were compared; every one of those aliases reported a false body divergence the moment they were | `a_separator_inside_quotes_does_not_end_the_statement` |
| It knew only `NAME()` | `080-check.sh` writes `function check_yadm_wrapper()`, so its function read as fish-only. The pair was not in the hardcoded list, so nothing ever noticed | `posix_functions_are_found_in_all_three_spellings` |

What the station does that the checker could not:

- **Pairs are discovered by stem**, across `shell/env.d`, `shell/interactive.d`
  and `fish/conf.d`, so a new paired file is covered without an edit. The six
  hardcoded pairs missed three that exist.
- **Bodies are compared** after normalising the dialects away — quotes, spacing,
  and the leading backslash POSIX uses to bypass alias expansion. That is how
  `gu` was found meaning `&>/dev/null` on one side and `2>/dev/null` on the
  other, which the checker's own header conceded it could never see.
- **A name defined more than once has no body to compare.** `ls` has three POSIX
  definitions, one per platform; which is live depends on the machine, so
  `Body::Conditional` says so rather than picking the last one read.
- **The decisions are data**, in `shell/parity.toml`: `allow` for a name that
  belongs to one dialect, `diverge` for one that means the same thing and cannot
  be spelled the same way, `unpaired` for a file that stands alone and why.
  Without an entry, an unpaired file that defines names is a finding — so a twin
  that gets deleted is loud, which is what the hardcoded pair list bought and
  auto-discovery would otherwise have given away.
- **An absent side is a skip**, not a hard failure. The checker's pair list named
  an encrypt-lane file, so it failed on any machine before the first
  `yadm decrypt`.
