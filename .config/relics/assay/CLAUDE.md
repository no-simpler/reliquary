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

## Deviations from `bin/check-yadm-coverage`

Golden-master parity verified three ways: on the live machine, on a fixture
machine reproducing every rule, and at all three grades — the same findings,
item for item, and the same exit code (0, 1 and 2). 809 ms → 241 ms.

| deviation | why | pinned by |
| --- | --- | --- |
| Archive membership is **asked of git** — `--glob-pathspecs ls-files --others --exclude=…`, yadm's own query against yadm's own repository — instead of reimplemented with Python `glob` plus a hand-written ancestor test | one matcher, and it is the one that decides what actually gets encrypted. The reimplementation was also over-eager: its exclusion test fell back to comparing **basenames**, so `!a/b/keep.txt` excluded every file called `keep.txt` anywhere. Latent today, and gone rather than fixed. Verified identical over this machine's lane (69 of 69 paths) before the swap | live and fixture parity |
| Both-lanes (R1) is git's **second** query, not a set intersection | `--others` is untracked-only, so a path in both lanes is invisible to the first query. yadm asks twice for exactly this reason | `a_pattern_matching_only_a_tracked_file_is_live_not_dead` |
| `yadm/unmanaged` globs get the semantics **the file documents** — `*` stops at a separator, `**` crosses one | it was matched with `fnmatch`, whose `*` crosses `/` and whose `**` means nothing in particular, so the documented contract and the enforced one had drifted. Verified no change on this machine: 511 paths, none decided differently | `a_declared_star_stops_at_a_separator_and_a_double_star_does_not` |
| One finding **per path**, carrying a `Location`; the script printed one grouped warning per rule with a twelve-path sample and a `-v` to see the rest. The flag is gone | a partial list reads as a complete one, and `Location` is the field that already means "where". The count the summary line carried is what a grouped finding was for | live and fixture parity |
| A credential finding names the **shape** — "a GitHub token" — and never the match | a report that quotes the secret has moved it somewhere new. The same rule governs the identity findings, which say *that* a term matched and never which | `a_credential_in_a_tracked_file_is_broken_and_the_secret_is_never_quoted` |
| Paths arrive NUL-delimited | the script split `ls-files` on newlines, which is the same class as the unquoted expansion the commit guard carried | `Repo::ls_files` |
| A clean machine prints nothing, where the script printed a tally of what it counted | inventory is not verification, and the roster line already says the station ran. Same as `bedrock` | `a_database_is_bloated_only_when_it_is_both_big_and_dead` and the fixture parity run |
| An unreadable `yadm/encrypt` stops the station instead of being one finding among many | the lane is undefined, so every rule downstream would answer about a lane that does not exist. The runner turns it into a `Broken` finding naming the station, which is the same grade the script reached | `an_undefined_encrypt_lane_stops_the_station_rather_than_reporting_a_clean_machine` |

**The identity rules are `warden`'s, and there are now three of them.** The hook
guards the staged set. This station guards two whole sets: every **tracked** file
— the standing full-tree sweep the commit guard shed when it narrowed, migrated
here as the design scoped — and every **undecided** one, which is the same guard
run backwards, where a hit is positive evidence of which lane a file belongs in.
One definition, one matcher, three callers. `warden`'s binary-allowlist is
honoured on the tracked sweep, because that sweep is the hook's test and the hook
refuses what nothing can vouch for.

**Found by porting: an empty pathspec list means *everything* to git.** An
encrypt lane naming nothing would have swept in the entire work tree and reported
every path as managed — a silent, total false clean bill, and the same shape as
the empty regex that would have matched every line in the identity guard. It is a
`Scope` enum rather than a pathspec list now, so the caller names what it wants
and cannot assemble the empty list that means the opposite.

**Its own fixtures caught it, correctly.** The first draft of the credential
tests wrote literal `ghp_…` and `glpat-…` strings into this file, and both the
retired script and the station reported the tracked source file as holding two
credentials. They are assembled at runtime now: a scanner whose own fixtures trip
it teaches the reader that its findings are noise.

## The registry adapter

The one A1 station that checks nothing. A relic knows its own invariants better
than any central checker could, so this asks rather than tests: it puts
`<name> doctor --format json` to every name in `~/.local/bin/.reliquary-managed`
and folds the answers into the one report. That file is the whole interface — a
Stage-3 relic in its own repository answers on exactly the same terms as one in
this workspace, and `assay` never reads either.

**Not answering is a fact, not a fault, and it is silent.** Nineteen of the
twenty-one registered binaries have no `doctor` subcommand; reporting that
would fill a standing audit with things nobody is going to act on. Two states
are not silent, because in both something is wrong rather than absent:

- **JSON that is not a report** — it meant to answer and got the shape wrong.
  Output that is not JSON at all is a program that never meant to answer, and
  that stays silent. `dewey`'s own `{"checks":…}` shape is the live example.
- **Still running when the budget expires** — a `Note`, because from outside a
  program a hang and slow work are the same fact: this one could not be judged.

| decision | why |
| --- | --- |
| Probe every registered name; do not require an opt-in declaration | it is what the design locked ("a registered binary with no `doctor` subcommand is a skip, not a finding"), and it needs no coordination with repositories this one does not own. Checked rather than assumed, 2026-08-28: of the eleven binaries this workspace does not own, ten have no `doctor` at all and the eleventh — halo's `dewey` — has one whose own test is `test_doctor_reports_and_never_writes`, and which refuses `--format json` with a usage error. Nothing is mutated by asking |
| A **2-second** budget, not a generous one | a health report is data the program already holds, and a binary without the subcommand refuses in milliseconds. `ske doctor` measures 20.3 s and never speaks the protocol anyway, so a budget wide enough to sit through it would put twenty seconds onto every standing audit to learn nothing — which is how a station gets switched off |
| The probes run at once | twenty-one process starts, several of them interpreters. Serially that is seconds on the path `yadm update` and the dream pre-pass both take. Order is restored afterwards, because a report that reshuffles between runs cannot be diffed |
| A report is refused unless its station is the binary's name or a name under it (`docket`, `docket-git`) | findings are minted through a `StationId` locally so a station cannot stamp another's name on its own report. Across a process boundary that cannot be enforced, only checked, and this is the check |
| A registered name that is not on `PATH` is silent here | registered-and-absent is real drift and it is `relic doctor`'s finding to report. This station only collects, and A1 introduces no new checks |

**Found by building it, and both were silent failures:**

- **The exit status is the answer's grade, not a verdict on whether there was
  an answer.** `Grade` is reported as `0`/`1`/`2`, so a speaker that found
  something exits non-zero *by contract* — and the first draft used
  `Tool::capture`'s reading, which discards stdout on a non-zero exit. It would
  have collected only from speakers that had nothing to say. `relic_core::tool`
  grew `run_within` and `Exit` for exactly this: a caller to whom the status is
  data.
- **Killing a shell does not kill what the shell started.** The bounded wait
  drained both pipes on scoped threads, and a scope joins before it returns — so
  a `sh` that had spawned `sleep 30` was killed at the deadline while its
  grandchild kept the write end of the pipe open, and the "bounded" wait ran the
  full thirty seconds. Nothing portable kills a process this one did not start,
  so the readers are detached and left to end on their own.

The renderer now names a finding's own station when it differs from the report's.
For all five built-in stations the two are the same name and nothing changed;
here it is the difference between "the registry said so" and knowing which
binary did.
