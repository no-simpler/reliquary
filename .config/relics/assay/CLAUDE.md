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

## A2 — what `yadm doctor` kept, and what it gave up

`_doctor_run` had ten sections. It is a thin caller now: it runs `assay` once and
folds the exit status into its own tally, reading nothing of the report, because a
caller that greps output is a second parser of a format nobody promised to keep.
Where each section went:

| section | fate |
| --- | --- |
| yadm resolves to the wrapper · interactive startup is clean · `$PATH` has no duplicates | **`shell-startup`** — one station, because all three come out of one probe per shell |
| shell alias/function parity | `shell-parity` |
| Claude Code `!` blocks | `md-shell-blocks` |
| bedrock dependencies | `bedrock` |
| Homebrew package health | `brew-health` |
| tracking coverage | `yadm-coverage` |
| shell lint and format | **`shell-lint`** |
| relic build cache | **`relic-cache`** |
| encrypted archive drift · archive vs disk verification | **stays in the wrapper.** Genuinely dotfile-specific, and the second is the one Touch-ID check |
| ske wiring | the registry adapter, once `ske` speaks the protocol — which is migration ④. Until then the wrapper keeps one call, because losing it would lose the check that a broken `gpg.ssh.program` shim breaks every commit |

## Deviations from `yadm doctor`'s shell sections

Parity verified on the live machine: all nine of the wrapper's checks pass and
the station reports nothing, which is the same verdict. The failure paths are
pinned by tests rather than by a fixture run of the wrapper — `_doctor_run` is
one function that cannot be asked for three of its ten sections.

| deviation | why | pinned by |
| --- | --- | --- |
| Three sections become one station | they are three readings of **one probe**, and starting an interactive shell is the whole cost. Splitting them by subject would have tripled it | `a_shell_that_finds_the_wrapper_has_nothing_to_report` |
| The probe is bounded, and a timeout *is* "startup did not complete" | a shell that hangs starting would hang the standing audit. From outside a process a hang and a very slow start are the same fact, and that fact is already what the section reports | `a_shell_that_never_returns_is_a_startup_that_did_not_complete` |
| A shell that is not installed produces nothing, where the script printed a skip line per section | three skip lines saying the same absence is inventory, not verification. Same grade | `a_shell_that_is_not_installed_is_simply_not_asked` |
| Duplicate `$PATH` entries are named once each, in first-repeat order, in the finding's detail | the script printed `sort | uniq -d`, which is duplicate-free but reordered — and a report that reshuffles between runs cannot be diffed | `duplicates_are_named_once_each_in_the_order_they_first_repeat` |
| The dialect difference is a `Dialect` enum with one method, not an `if` on the shell's name | the two dialects differ in exactly one place — how a list variable is joined — and naming that makes it the only thing a fourth shell would have to supply | `the_fish_probe_joins_a_list_and_the_posix_one_does_not` |
| `run_within`, not `capture_within` | an interactive shell's exit status is whatever its last rc line left behind. What is asked is whether the probe reached the end, and the marker answers that | `a_shell_that_never_reaches_the_marker_did_not_start` |

## Deviations from `yadm doctor`'s relic-cache section

Parity verified on the live machine: the same number, the same lane, the same
grade — 10083 MiB in `~/.config/relics`, soft.

| deviation | why | pinned by |
| --- | --- | --- |
| A lane under the ceiling prints nothing, where the script printed its size with a tick | inventory is not verification, and a size nobody needs to act on is a number nobody reads. Same grade | `a_small_build_tree_has_nothing_to_say` |
| A lane `du` cannot measure is a `Note`, where the script silently reported nothing | "could not measure" and "measured, and it is fine" are different facts, and only one of them is a clean bill | `a_directory_that_does_not_exist_cannot_be_measured` |

`du -sk` is kept rather than replaced by a walk, and it is not a house-rule
violation: POSIX fixes the output as a number, a tab and the path, and `-k`
fixes the unit so the answer does not depend on a `BLOCKSIZE` the environment
happens to carry. It also counts **blocks**, which is what the disk is actually
holding — a walk summing apparent sizes would answer a different question and
report a different number from the one `up` acts on.

## Deviations from `bin/check-shell-lint`

Three gates over the shell that stays shell: lint, format, and the count of
inline suppressions. The third exists because the first two cannot see it — a
`# shellcheck disable=` makes a finding vanish from both — so the count is
committed per file and compared as an **equality**. Removing one means lowering
the number in the same commit; an inequality would let slack accumulate, and
slack is suppressions that can be added back unseen.

Parity was measured before the script retires. Over the live tree both select
**the same 63 files** and issue **the same flags** to both tools — verified by
running the station with recording shims in front of `shellcheck` and `shfmt`
and diffing against the script's own `enumerate` — and both grade `ok`.

| deviation | why | pinned by |
| --- | --- | --- |
| `shellcheck -f json1`, not `-f gcc` | file, line, column, level and code arrive as fields. The script counted lines of `path:line:col: level: message`, which a message containing the separator would have split wrongly — and `json1` also carries the `fix` object a line reader never sees | `the_json1_shape_is_the_one_shellcheck_emits` |
| An unparseable file is a `Broken` finding | the script sent `shfmt`'s stderr to `/dev/null`, so a file neither tool could read passed the format gate in silence — and passed the lint gate too whenever `shellcheck` was absent | `a_file_the_formatter_cannot_parse_is_reported_rather_than_silenced` |
| One finding per file, with its position and the lints in the detail | the script emitted one lumped line per gate plus indented text. A finding that carries a `Location` is one an editor can open | `a_lint_is_broken_and_carries_its_code_and_position` |
| Ratchet drift is one finding per file, naming both numbers | the script diffed two sorted `path<TAB>count` lists and printed `baseline only:` / `tree only:` lines, so a *changed* count appeared twice and never said what changed | `a_changed_count_reports_both_numbers` |
| The baseline is parsed as TOML | the script parsed it with an `awk` that split on `=`, stripped quotes from the key and non-digits from the value — so a malformed line was silently dropped rather than refused. An unusable baseline now stops the station | `an_unusable_baseline_stops_the_station_rather_than_grading_a_clean_machine` |
| A repository that cannot be reached is `Broken` | the script's `yadm ls-files 2>/dev/null` returned nothing on a broken machine, printed "no shell files in scope" and exited `0` — a clean bill for a machine it could not read. Same stance as `yadm-coverage` | `no_yadm_on_the_path_is_broken_and_never_a_silent_pass` |
| Scope is decided per file by name and shebang, in `Rel` terms | same rules as the script's `in_scope`, but without a `head` and a `grep` per file. `zsh` and `fish` spell `sh` without naming it, which is what the word boundary is for | `a_shebang_naming_bash_or_sh_is_ours_and_one_naming_anything_else_is_not` |
| `shfmt` output is intersected with the population | `shfmt` has no `-0`, so a path holding a newline would arrive as two lines naming nothing. A line that is not a file the station handed over is said out loud rather than guessed at | `a_formatter_naming_something_it_was_not_given_says_so` |
| No subset mode | the script took paths and then had to disable the ratchet for them, because a subset cannot tell a file with no directives from a file it was not asked about. A station always runs over the whole population, so the special case disappears. `relic test`'s bash branch therefore runs the whole population too — a superset gate, ~3 s over 63 files, whose every finding is true | — |
| The tools are shimmed in tests, and their contract is pinned against captured output | a test that shells out to the machine's `shellcheck` answers for that machine's version. The station's logic and the tool's format are two facts, pinned separately | `the_json1_shape_is_the_one_shellcheck_emits` |

## A3 — the stations that are new

Nothing here has a retired script behind it, so nothing here has a golden master.
Each is a behaviour change, pinned by its own tests and by what it finds on the
live machine the day it lands.

| station | state |
| --- | --- |
| `perf-budgets` | **built** |
| harness permission rules | to build — and it retires `halo`'s `settings_lint.py` one dream cycle later |
| harness hook wiring | to build |
| `~/.claude` skill and plugin health | to build |
| `$PATH` sanity | to build; it and the registry adapter are the whole of `bin/pb`, which is deleted rather than ported |
| git-identity separation | to build — the three mechanisms that must all be right |
| manifest ↔ installed drift | to build, for the cargo and npm lanes |

## The `perf-budgets` station

The fourth ratchet, and the one that guards this programme's own claims. It reads
`reliquary/ratchets/perf-budgets.toml` and times what the file names.

| decision | why |
| --- | --- |
| `--deep` only | it spends the time it measures. A full deep run is ~3 minutes on this machine, most of it one path. Inside an ordinary `yadm doctor` that lands on the dream pre-pass, and the first thing anyone does about a three-minute pre-pass is stop running it |
| `Soft`, never `Broken` | a slow machine is degraded and entirely reproducible from the repo, which is the definition |
| One run, no warm-up, no statistics | a smoke alarm, not a benchmark suite. The tolerance is ×3, so what is detected is a path that changed kind — not one that drifted by a fraction |
| `command` is argv, not a command line | there is no shell, so nothing splits, quotes or globs. A first element carrying a `/` is a path under the home being checked, which is how a hook that is not on `$PATH` is named |
| A path is bounded at `budget × tolerance × 2`, floor 1 s | without a bound a pathological path holds the run open for as long as it likes. Twice the reporting threshold, so a finding still carries a real number wherever a number would tell anyone anything; past that, "over N" is the whole of what a smoke alarm has to say. The floor is because process startup alone is a few ms and `ske-prompt`'s budget is 10 |
| Exit status is ignored | the clock is what is being read. `yadm doctor --quiet` exits 1 on a degraded machine, which is not a failure to time |
| `timed = false` is silent | a recorded decision, in the same sense as a line in `yadm/unmanaged`: the reason sits beside it in the file, and a note repeated on every run is inventory rather than verification |
| A program the machine does not have is a `Note` | one item that could not be judged, which is what `Note` is. The other eleven paths still have answers |
| `stdin` is fed from a temporary file | `modes.py` refuses an empty stdin, and a timing taken from a program that refused is a timing of the refusal. A pipe would need somebody left to write into it |

**The recursion is real and it terminates.** `yadm doctor --quiet` is a budgeted path
and `yadm doctor` runs `assay` — but without `--deep`, so the inner run skips this
station. Depth two, and the number is honest for the same reason: it is what the path
costs today, `assay` included, which is most of what it now is.

**It found a live regression on its first run**: `ske doctor` at 80.6 s against a
budget of 20.3 s recorded one day earlier — 4×, past the tolerance. Migration ④ is
what fixes it; until then the ratchet is the only thing that says so out loud.
