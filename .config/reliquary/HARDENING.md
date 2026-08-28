# Hardening — the verification standard

Reliquary's executable surface — relics, `~/.config/bin/`, hooks, and the shell that stays
shell. `GRADUATION.md` owns a relic's lifecycle, `BEDROCK.md` the substrate,
`AGENTIC-TOOLING.md` third-party admission. This owns **how any of it is made verifiable**,
and what to do where it cannot be.

The work here is overwhelmingly agentic, and an agent under pressure cuts whatever is
optional. So the standard is stated as controls, not as advice: a rule that depends on being
read and honoured degrades silently under exactly the conditions that make it matter.

## The three tracks

Rust closes the types/borrows/exhaustiveness corner. It says nothing about whether tests were
written — `cargo build` gates **compilation**, not **testing**. Three tracks, because any one
alone is a partial answer.

1. **Rust under a typed-domain standard** — for code with logic in it.
2. **Hardened shell** — for the residue that will never migrate. Not the cheap alternative to
   track 1; the correct answer for a class that has no other one.
3. **Verification ratchets** — the deterministic controls that close what Rust leaves open.

## The triage rule

Applied in order. A decision function, not a list.

1. **Does it produce or install the first binary, or stand between a bare machine and one?**
   → shell, permanently. The **self-hosting fixed point**: nothing on that path may presuppose
   a binary. That is a property of the path, not of the toolchain — shipping prebuilt binaries
   instead of building them does not dissolve it.

   > **Corollary.** Fail-closed with a legible remedy is acceptable on the critical path;
   > fail-open is not; silent failure never is. A guard whose binary is missing must refuse and
   > name the fix. The exception that proves it: output that is pure decoration, where the
   > failure is already visible as its absence, must fail **silent** — a prompt segment
   > printing a diagnostic on every render is worse than printing nothing.

   > **Fail-closed on a write path needs a break-glass.** A guard that can brick the very
   > commit that would fix it must ship one explicit, noisy override, logged under
   > `~/.local/state/` so using it leaves a trace. Break-glass, not a flag reached for twice.

2. **Must it run *in* the calling shell's process?** → shell. Exports, `PATH` mutation,
   `eval "$(brew shellenv)"`, `setopt`/`bindkey`/`compinit`, prompt hooks, aliases, `cd`. A
   child cannot mutate its parent. Rust may **generate** these; it can never **be** them.
3. **Thin orchestration of an interactive external CLI?** → leave, or delete. There is no
   parsing, no state machine, no set algebra — nothing for types to protect. A port yields
   `Command::new` boilerplate restating what `$()` already said.
4. **Logic density — parsing, set algebra, tree walks, state machines?** → Rust. Prefer
   **absorption** into an existing or new relic over a 1:1 port.

**The unit of migration is a capability, not a script.** Where a port reads as "translate this
file", it has failed; the correct read is "this capability now has one owner, one type, and
one test suite". Guard the opposite failure equally: this is not a licence to rewrite
`~/.config` as one program, which is what rule 3 and the residue below exist to prevent.

## The irreducible shell residue

Enumerated by *why*, because the defence differs by class and only one of them is a judgement
call. Track 2 exists for exactly this set.

**Class 1 — the recovery critical path.** The gpg shims, `ske-sign`, the `yadm` and `gh`
passthrough shims, `timeout`, all of `yadm/snippets/**`, and `reliquary/lib/relic.sh`'s
bootstrap seed.

> **The bootstrap paradox: the thing that builds and publishes the first Rust binary cannot be
> a Rust binary.** A fixed point, not a preference a faster toolchain could overturn. The gpg
> shims sit at the same point one step earlier — they are how the archive decrypts, and
> decryption is what delivers everything else.

**Class 2 — must execute in the caller's process.** The root and `ZDOTDIR` entry points,
`shell/env.d/*`, `shell/interactive.d/*`, `fish/conf.d/*`.

> Process semantics, not taste. Generating this text from Rust changes the residue's size and
> not its shape: the executed artifact was always going to be shell.

**Class 3 — thin orchestration of an interactive CLI.** `bbs`, `mm`, `dbee`.

> The one judgement call. Verify the premise before invoking it — that the body really is an
> external command plus argument shuffling, with no logic underneath.

**Class 4 — a stable cross-repo sourced ABI.** `reliquary/lib/install-on-path.sh`.

> Its interface *is* "source this file, call this function", and external relics do exactly
> that. A binary breaks two repositories and a documented contract. Class 2 one level up: a
> shell caller can only get a function into its own process by sourcing shell. It may gain a
> binary sibling; the sourced form outlives it.

**Class 5 — vendored.** Files their own managers regenerate. Already in `yadm/unmanaged`.

## Track 1 — the typed-domain standard

A port that is `Vec<String>` everywhere with `Command::new` underneath is bash with a compile
step: the compiler verifies nothing about the domain. The standard is the point.

> Opaque representations — bare `String`, `serde_json::Value`, stringly-typed flags, integer
> status codes, untyped maps — are permitted **only at the I/O boundary, and only for the width
> of one function.** Everything inward is a named type.
>
> - **Parse, don't validate.** Untyped input converts **once**, at the edge, via
>   `FromStr`/`TryFrom`, into a type whose existence proves the invariant. Nothing downstream
>   re-checks; nothing downstream *can*.
> - **Make illegal states unrepresentable.** Enums for states, newtypes for identifiers, the
>   **typestate pattern** where sequence matters — an invalid transition is a compile error,
>   not a runtime branch.
> - **No wildcard arms on our own enums.** A `_ =>` is how a new variant silently gets
>   mishandled; exhaustive matching makes adding one a compiler-guided refactor.
> - **Typed errors** (`thiserror`) in library crates, `anyhow` only at binary edges. Never
>   exit-code arithmetic, never `LEVEL<TAB>message`.
> - Newtypes are zero-cost. This buys static analysis, not runtime overhead.

**Two ambient types the standard names outright**, because every relic meets both:

- **Paths are `camino::Utf8Path`.** A relic's paths are program data — keys it compares, stores
  and prints. `to_string_lossy` maps two different directories onto one key, and serde's
  `PathBuf` refuses a path it cannot spell deep inside a save rather than at the edge.
  `relic_core::path::utf8` is the parse; past it a path is a string by construction. `std::path`
  stays where a path is an arbitrary filesystem entry being walked rather than data.
- **Filesystem calls go through `fs_err`.** A bare `io::Error` says "permission denied" and
  leaves the reader to guess which of a write's four paths it meant. With the path in the error,
  a caller's own context supplies the verb instead of restating the path.

**The boundary carve-out is real, and naive application is fragile.** Third-party schemas
(`brew info --json=v2`, `docker inspect`) gain fields on every upgrade — deserialize them into
typed structs, but never with `#[serde(deny_unknown_fields)]`. Reserve that for schemas **we**
own, where an unknown key is a typo worth failing on. Likewise exhaustive matching applies to
**our** enums; an upstream `#[non_exhaustive]` type forces a wildcard and that is correct.

**Enforced, not aspirational.** One `[workspace.lints]` table per workspace root plus
`lints.workspace = true` in every member. A selected lint is then non-bypassable the same way
`cargo build` is. Written as a table entry, never a per-crate attribute — those drift between
members.

Two cargo constraints shape this, and neither is optional:

- **The table is the only lint policy; `relic test` passes no `-D warnings`.** A command-line
  group flag outranks every table entry and collapses `warn` and `deny` into one level, which
  makes a transitional worklist impossible to express. So the groups that flag used to deny
  are named in the table at `deny` — `clippy::all`, and rustc's `deprecated`,
  `future_incompatible`, `nonstandard_style`, `unused` — and the transitional lints sit at
  `warn` beside them. Policy in a committed file can be ratcheted; policy in an invocation has
  one setting. A group carries a lower `priority` than the specific lints that override it.
- **`lints.workspace = true` is exclusive of every other entry in a package's `[lints]`
  table**, so a deliberately per-crate lint has no table form at all. `missing_docs` is the
  case: it belongs on platform crates and is noise on binaries, so it lives as a crate-root
  `#![deny(missing_docs)]` in each platform crate. That is not the drift the rule warns about —
  it is one line, in the one place, for the one lint cargo cannot express.

Baseline `clippy::pedantic`, plus the `restriction` lints that map onto the rules above:
`wildcard_enum_match_arm`, `match_wildcard_for_single_variants`, `as_conversions`,
`unwrap_used`, `expect_used`, `panic`, `indexing_slicing`, `string_slice`, `str_to_string`,
`missing_errors_doc`, `missing_panics_doc`. Two `[lints.rust]` entries join them:
`missing_docs = "deny"` on platform crates, which makes doctests carry real weight, and
`unsafe_code = "forbid"`.

**A `clippy.toml` is required, not optional** — `allow-unwrap-in-tests`,
`allow-expect-in-tests`, `allow-panic-in-tests`. Without it those lints fire on every test
file, and the cheapest repair an agent reaches for is a module-level `#![allow]` that also
disarms the lint for the production code beside it. **A restriction lint that has to be
suppressed to write a test is a lint that will be suppressed everywhere.**

**Ratchet a lint table in; never land it hot.** Turning a dozen lints to `deny` across existing
code in one commit produces a wall of failures and a matching wall of `#[allow]`. Land at
`warn`, flip to `deny` once the offending code has been rewritten for other reasons. Each
surviving `#[allow]` carries a reason comment, and the count of them is itself a ratchet.

## Track 2 — hardened shell

`shellcheck` + `shfmt` over all tracked bash — `*.sh`, `*.bash`, and bash shebangs; zsh and
fish are excluded because neither is parseable by the linter, fixtures because lints written
into one are their point, symlinks because they would lint their target twice.

**Zero findings at `-S warning`, and no baseline of findings.** A finding that is genuinely
wrong for this codebase is accepted *where it happens*, with an inline disable directive
carrying its reason — provenance beside the code, the same rule the deviation list follows.
The **ratchet is the count of those directives**, committed per file: silencing a finding makes
the finding count fall, so nothing else can see it happen. Identical control to the Rust lane's
`#[allow]` count, identical equality semantics. One mechanism, two languages.

The predicate that counts a directive must **mirror the tool's own parsing rule** — a directive
is a comment of its own line. Anything that merely mentions one mid-line, prose included, is
not a suppression and must not count as one.

Wired into `relic test`'s bash branch and a verification station. **Deliberately not into
`pre_commit`**, for two reasons that agree:

- `pre_commit` exists because a credential leak is *unrecoverable*. A stale format or an
  unquoted expansion is recovered by one command. Mixing the two dilutes the hook that must
  never be bypassed into one people learn to bypass.
- It sits in front of a Touch ID prompt. Two linter startups per commit is how a hook measured
  in milliseconds goes back to seconds.

Bash has no type system, so refactors here stay unverified. Lint and format are the whole
budget; spend them.

## Track 3 — the verification ratchet

**What a ratchet is** — the definition most likely to be reinvented wrongly. A ratchet is a
**deterministic control, not an agentic directive**: a committed baseline file, plus a check
that compares today's measurement against it and exits non-zero when it is worse. No judgement,
no model in the loop — a program comparing two numbers.

The governing property is not "the number may never worsen". It is: **you can always move the
number, you just cannot move it silently.** A legitimate regression is an edit to the baseline
in the same commit, which puts it in a diff someone reads. A tripwire with a signature line,
not a ceiling — which is what makes it survivable in a codebase under active change.

| ratchet | measurement | baseline | fails when |
| --- | --- | --- | --- |
| coverage | `cargo llvm-cov --json`, per crate | committed per-crate % | any crate drops, or falls under the floor |
| lint | count of `#[allow(...)]`, per package | committed integer | the count *changes* |
| shell | count of disable directives, per file | committed integer | the count *changes* |
| perf | timing of the declared hot paths | committed budget | over budget by the stated multiple |

Two calibration rules, both load-bearing:

- **A threshold set too tight becomes noise, and noise gets disabled.** Hence a deliberately
  low coverage floor, generous perf headroom, and `warn` before `deny`. **A gate people switch
  off is worth less than no gate**, because it also carries the belief that it is on.
- **A baseline computed on the fly is not a ratchet**, it is a moving target. It must be
  committed, and moving it must be an edit.

**A baseline lives with what it measures**, so a subsystem that relocates carries its ratchets
with it: the workspace's coverage and lint baselines under `relics/ratchets/`, the shell and
whole-surface ones under `reliquary/ratchets/`.

**The lint ratchet is an equality, not a ceiling** — a count that *falls* fails too, and the
repair is to lower the baseline in the same commit. An inequality lets slack accumulate: five
suppressions removed and never accounted for is five that can be added back unseen. The
ergonomics are `insta`'s — a changed measurement fails until it is accepted.

### The test harness is not written here

A CLI suite hand-rolls the same four things every time: a unique temporary tree with a `Drop`
that cleans it, a process invocation, a status assertion, and a stream assertion. The
ecosystem owns all four — **`tempfile`, `assert_cmd`, `predicates`** — and the reason to take
them is not the line count. A bespoke fixture prints what its author thought to print; a
failing `assert_cmd` assertion prints the invocation, both streams and the exit code, every
time. And the next reader recognises the idiom instead of learning one.

**Two rules the crates do not enforce.** A fixture holds its `TempDir` in a field, or the tree
is deleted before the first command runs. And a suite's *domain* helpers — the ones that name
what a test is about — stay hand-written: they are the readable part, and replacing them buys
nothing.

**`proptest` for properties an example can only sample** — idempotence, monotonicity, the
invariants of a normal form, a round trip that must not lose bytes. Not a replacement for
example tests, which is what says the behaviour is the *intended* one. **`insta`** where the
assertion is a whole output, replacing hand-managed `*.expected.*` fixtures; a suite whose
assertions are structural gains nothing from it and should not adopt it for symmetry.

### Coverage, and what it is worth

`cargo-llvm-cov`, **70% floor, 85% for platform crates**, plus a committed per-crate baseline
that may never regress. The floor is deliberately low so the ratchet starts working
immediately rather than after a catch-up phase; the ratchet does the real work.

Coverage alone is gameable by exactly the behaviour this guards against — a test that executes
a line and asserts nothing scores the same as a real one. So it is layered:

- **`cargo-mutants` is the real assertion-quality gate.** It mutates the code and checks
  whether tests *fail*; an assertion-free test kills no mutants. Run at wave and milestone
  boundaries, not in the loop.
- **Fast loop / slow gate.** `relic test` stays `fmt → clippy → nextest` and must stay fast —
  **agents route around slow commands.** Coverage and mutation are separate, deliberate
  invocations.
- **Free wins Rust already gives**: doctests (`///` examples compile and run — documentation
  that lies fails the build), `#[must_use]`, exhaustive matches.
- **`proptest`** for parser-heavy work — globs, shell tokenizing, diff hunks, TOML,
  frontmatter. A generator plus invariants is less code than example tests and catches more.

**On 100% coverage** — not achievable, and the reasons set the honest band: `main`'s
process-exit path, clap-derive generated code, `#[derive]` impls, `unreachable!`/`panic!` arms
kept as invariant documentation, `#[cfg]`-gated platform code, and OS error branches that
cannot be portably induced. `llvm-cov` counts **regions**, not lines, so every `?` and match
arm shows partial coverage that line-counting hides. **80–85% for a CLI crate, 90%+ for a
pure-logic crate.** Above that is coverage theatre.

### Performance budgets

Every latency win is a win nothing otherwise prevents from eroding. A committed budget file
covers the declared hot paths; a station times them and reports when one exceeds its budget by
the stated multiple, graded **degraded, not broken**. Deliberately loose: timings are
machine-dependent, and a laptop under thermal load must not trip a gate people then switch
off. **A smoke alarm, not a benchmark suite** — no `hyperfine`, no criterion harness.

### Supply chain

`cargo deny` (licences, advisories, duplicate versions) replaces dependency *counting* with
dependency *hygiene*, against a committed `deny.toml`. Not `cargo-audit` as well:
`cargo deny check advisories` reads the same RustSec database, and two tools doing one job is
two things to keep current. `cargo machete` covers the one thing it does not — a dependency
declared and never used — on stable and in milliseconds, where `cargo-udeps` needs nightly and
a full build.

**An installed tool nobody invokes is an unratcheted dependency** — upgraded forever, audited
by nobody, its absence never noticed. Every entry in `cargo/crates.txt` carries a one-line
reason comment, the convention `brew/undeclared` already uses.

## The reuse ladder

> For any capability a relic needs:
> 1. **A maintained ecosystem crate.** Default. No dependency budget.
> 2. **A workspace platform crate** (`relic-*`) — when the ecosystem has no fit, *or* when one
>    opinionated wrapper is needed so every relic behaves identically.
> 3. **Relic-local code** — only what is genuinely unique to that relic.
>
> Writing at 3 what exists at 1 or 2 is a review failure.

**Platform code is reuse-first. Domain code keeps the second-consumer gate.** The split
matters. "Second consumer" exists to prevent premature abstraction, and the wrong abstraction
is more expensive than duplication — a real risk for **domain** code. It is not real for
**platform** code, where the second consumer exists by construction: every relic needs colour
resolution, atomic writes, locking, subprocess capability.

## Four house rules

Each is the class behind a defect found in this repo, not a preference.

**1 — Never parse human-facing output.** Use a **machine-readable interface** — `--porcelain`,
`-z`, `--format json`, an exit code — or own the data. Where none exists, pin `LC_ALL=C` at the
call site; the platform subprocess capability sets it **by construction**, the same way it
scrubs `GIT_*`, so a caller cannot forget. This class has produced silent-success bugs under
localized git, English-string branching on Docker errors, a health check grepping its own
stdout, and a parity checker extracting a phantom alias out of a prose comment.

**2 — Ambient authority is injected, never read.** **Clock, randomness, environment, filesystem
root and subprocess are capabilities passed in**, not things reached for. It is the
constructor-level form of the `GIT_*` scrub — `GIT_DIR` outranks `-C`, so a relic run from a
git hook otherwise answers for the hook's repository — and it is what makes monotonic deadlines
and id generation testable at all.

**3 — Deterministic output.** Stable ordering, LF endings, one trailing newline, no timestamps
or absolute paths in default output. It is what keeps snapshots stable and diffs reviewable,
and it costs nothing as a rule from the first relic rather than a retrofit after the tenth.

**4 — Retirement is a separate commit from replacement.** Never delete a script in the commit
that lands its port. `git revert` of the retirement is the rollback, and once the script is gone
there is no other one: `install-on-path.sh` overwrites in place and keeps no previous binary.

## Replacing a script

**Characterization tests first** (Feathers' golden-master). Porting untested code without
capturing current behaviour is how behaviour changes silently. Per target: capture
stdout/stderr/exit on the live machine into fixtures (`insta` manages them), port, assert
parity.

**Deliberate-deviation list.** Golden-master would otherwise lock in the bugs that motivated
the port. So: capture behaviour, then maintain an explicit list of intended deviations — one
recorded fix, one test each. **A deviation not on the list is a regression.**

**Where the list lives: with the code that replaced the script.** Each replacement relic's own
`CLAUDE.md` carries a `## Deviations from <retired script>` section, one entry per deviation,
each naming the test that pins it. Not a central register — provenance belongs beside the
implementation, and a central file goes stale the moment a relic moves.

**Strangler fig.** The new binary lands alongside; the old script retires only when parity is
proven. `install-on-path.sh` fails fast on name collision, so the transitional name must differ
or the old script must be unpublished first.

**Transcribed tables carry an obligation.** Any check whose tables are derived from a
third-party binary must record the re-derivation recipe and the version read against, and must
**fail loudly when that binary moves** — otherwise it silently stops checking.
