# ernest

Measures **prose density**: the share of a codebase's text that is prose rather
than code. Stage-2 Reliquary relic; see `~/.config/reliquary/GRADUATION.md` for
the lifecycle it lives in.

Prose density is the canonical term. `ernest` (for Hemingway) is only the tool's
name — there is no "Ernest Index".

## Mission

Agents leave prose in files despite every directive to be terse, and most of it
is re-derivable on demand: it is paid for on every context load and repaid
rarely. The `deprose` mode makes that a mantra. This makes it
a number.

**A helper, not a gate.** It quantifies what otherwise takes a holistic skim
over a large body of text. The workflow:

```
ernest --json --by file,section > before.json
… make the change …
ernest --json --by file,section > after.json
ernest diff before.json after.json
```

Measure, change, look at where the difference came from, act if there is
something to act on. git drives the before and after — `git stash`, a worktree,
a branch checkout — because it already does that well and ernest does not
reimplement it.

**No targets.** The direction is down; how far is judgement. ernest puts a
number on the vibe and stops there. `--max-density` lets a caller assert a
ceiling; nothing in the tool proposes one.

It is not fool-proof and does not try to be. Padding a file with uncommented
filler moves the number without improving anything; that is deliberately not
counteracted. The premise is that a quantified number prompts honest de-prosing
work. If agent behaviour turns out otherwise in practice, *that* is when to
reconsider — not in advance.

## The metric

```
prose density = prose / (prose + code)        0.0% – 100.0%
```

Uninteresting text is excluded from both sides. 0% means no prose was found;
100% means nothing but prose. Aggregation is a ratio of sums, never a mean of
per-file ratios, so small files cannot dominate. Density is `n/a`, not 0%, when
nothing countable was found.

### Every cohort counts toward it

The headline sums `Source` and `Docs`. Markdown is prose describing code, and
prose lifted out of a comment into a document has not gone anywhere — it is
still loaded, still re-derivable, still paid for. A metric that fell when prose
merely changed address would reward the move, and moving is not de-prosing.

Summing makes the headline **relocation-invariant**: the same characters sit in
both numerator and denominator wherever they live. On `lib-offer-backbone`,
moving 10,000 chars of PHP comments into a new `docs/*.md` used to buy a 0.9pp
improvement for no work. It now buys 0.0.

This is not the old objection re-litigated. A `Docs` **density row** really is a
near-constant — Pillar's runs 98% — and the one lever that moves it is *adding*
code fences, a gradient pointing away from de-prosing. That argues against
reporting docs density as a headline, which nothing here does. In a *summed*
ratio the source code base keeps the figure off the ceiling and responsive:
Pillar reads 26.7%, `lib-messenger` 25.9%, `app-demand-planning` 0.8%, and
10,000 chars deleted move the number the same whether they came from a comment
or a document.

The cost, stated: the headline no longer answers "how commented is my code."
The `source` row still does, and `--by language` is where it is read.

### What that assumes

**A code-first repository**, where source code dominates the denominator. Point
ernest at a wiki or a fiction archive and the headline pins near 100%, because
the prose there *is* the product rather than a description of code. That is a
category error, not a reading — see `.ernestignore` below for the narrow case
where a code-first repository also carries such a corpus.

The `Docs` cohort keeps its own volume line and one cross-cohort comparator:
documentation prose against the source code it documents. It complements the
headline rather than repeating it — relocate prose and the headline holds still
by design, while this is the line that rises and says where the prose went.

### Unit: non-whitespace characters

Lines are the wrong unit here. Prose lines run systematically longer than code
lines, so line counting understates prose; and a mixed line (`$x = 1; // note`)
has to be forced whole into one bucket, which by convention makes trailing
comments — the most common agent prose in PHP and YAML — invisible. Characters
let the line split.

Only non-whitespace characters count, which disposes of indentation, trailing
whitespace and blank lines with no special-casing. `--unit lines` reports the
familiar proxy instead; a line is then attributed whole to whichever class holds
most of it, ties going to code.

### Classification principle

**Recognise prose first, then split what remains into code and uninteresting.**

*Uninteresting* means unavoidable — text you cannot write your way out of given
the file exists at all: `<?php`, a shebang, a YAML document marker, a tooling
pragma. An annotation such as `@param` is avoidable (delete the docblock), and
it is not prose, so it is **code**.

A pragma is unavoidable *once you want the tooling*, and that holds whichever
vehicle a language gives it. `# shellcheck disable=…`, `// phpcs:disable` and
`#[allow(dead_code)]` are one rule, not two, so a Rust lint or format attribute
is uninteresting while `#[derive]`, `#[cfg]` and `#[serde]` carry meaning and
are code.

### Delimiters

tree-sitter puts a comment's delimiters inside its node span, and they stay
there.

- **Comment delimiters are prose, unstripped.** They exist only because the
  comment does. On an ordinary docblock that is 2–3% of the span; on a
  decorative `// =====` banner it is nearly all of it, and billing that at full
  weight is the right answer, not an artifact.
- **Code punctuation is code.** Braces scale with the code you chose to write,
  unlike `<?php`. The consequence: density is comparable *within* a language,
  not across them. The total reads as "this repository's mix"; a per-language
  figure is read from `--by language`, and that caveat attaches there.
- **Whitespace is never counted**, so the bytes between spans need no rule.

The same line runs through Markdown's structure: what scales with the construct
is code, what frames it once is prose. A wide table's pipes are mass that is not
text, so they and the delimiter row bill as code; a heading's `#` exists only
because the heading does, so it bills as prose along with list markers and
blockquote markers. Table cells stay prose, which leaves turning a paragraph
into a table rewarded by the characters it saves and by nothing else.

### Prose about the code, not text in general

JSX text — the `Hello world` in `<p>Hello world</p>` — is **code**. It needs no
rule to make it so: nothing names it, so it falls to the source default, which
is the wanted answer. Interface copy is the product rather than a description of
it. It is not re-derivable on demand, it is not paid for on a context load in
the way an explanation is, and it sits on the same side of the line as any other
string literal. A comment *inside* the markup arrives as an ordinary `comment`
and is prose like any other.

The same silence covers the same case in the formats where markup is the whole
file: HTML's `text` and Twig's `text` are named by no rule either, so a page's
copy and a template's copy bill as code for the reason JSX's does. That is also
why `html` sits in `Source` rather than `Docs` — a page is the product, and
`Docs` is for the formats whose default inverts to `Prose`.

This is the same line the Markdown table runs along, and the same line that
makes a wiki a category error for the headline: ernest measures prose *about*
code, not text as such.

## Architecture

Byte-span classification, not line bucketing — which is what tree-sitter hands
back natively.

```rust
pub enum Class { Prose, Code, Ignored }
pub struct Span { start: usize, end: usize, class: Class }
```

An analyzer names only the spans it recognises; everything uncovered falls to
`Profile::default_class`. For source languages that is `Code`, so an analyzer's
whole job is finding prose. Markdown inverts the default to `Prose` and names
code blocks instead — same contract.

```
src/
  span.rs             Class, Span, coverage fill, measurement in both units
  analyze/mod.rs      tree walk, pragma rule, annotation and region post-passes
  analyze/profiles.rs the format registry
  analyze/sections.rs innermost-section decomposition of a document
  detect.rs           path, then shebang -> profile
  walk.rs             scope, provenance, ignore::WalkBuilder, default excludes
  rank.rs             which files the ranked views cover: --changed, --focus
  aggregate.rs        file -> (cohort, provenance, language) -> report
  report/             blocks and notes, then table, human, json, diff
```

`Cohort` splits `Source` from `Docs`. It is a breakdown, not a barrier: the
headline is both summed, and the table rolls up total -> cohort -> language.

## Scope and provenance

Selection asks two questions that git already answers separately. `.gitignore`
is committed and holds the noise — build output, caches, vendored code.
`.git/info/exclude` is local and holds what you keep but do not share, which on
these machines is the second brain: `CLAUDE.md` and `.claude/`. Where both match
a path, `.gitignore` wins and the file is noise.

```
--scope shared   what a clone would see
--scope local    plus locally-excluded files   [default]
--scope all      plus gitignored files
```

**Dependency and build output is the repository's declaration to make, not
ernest's to assume.** `node_modules`, `vendor` and `target` are not named
anywhere in the walk; `.gitignore` is what excludes them, which is what it is
for. A second, hidden list would answer a question the repository has already
answered, and would make `--scope all` quietly not do what it says.

So `--scope all` means all. On a PHP repository with `vendor/` on disk that is
67,989 files rather than 3,022, and a figure dominated by code nobody here
wrote. The levers for not wanting that are the default scope and a declared
`.ernestignore` — both of them visible.

Two consequences worth holding on to:

- **The walk sets `require_git(false)`.** Left at the crate's default, a
  `.gitignore` with no `.git` beside it is inert — and that is exactly the shape
  of a yadm-managed tree, where the work tree is `$HOME` and the git dir lives
  elsewhere. This repository is one, so its own `/target` rule would have gone
  unread and the delegation would have failed in the tree ernest was written in.
- **The VCS directory is the one exclusion that cannot be delegated.** git keeps
  `.git` out structurally rather than by rule — `git check-ignore .git` says
  nothing — and the walk sets `hidden(false)` on purpose, because dotfiles are
  the subject here. Without `VCS_DIRS`, a repository's `.git/hooks/post-commit`
  reads as ordinary shell source.

### `.ernestignore`

git answers "is this shared". It cannot answer "is this what ernest is for" —
so a repository that carries prose which *is* the product, rather than prose
about the code, declares it: gitignore syntax, honored at **every** scope,
because a corpus is not the subject at any of them.

Declared, never inferred. A rule keyed on doc-like filenames would miss
`AGENTIC-TOOLING.md`, and would make renaming `CLAUDE.md` to `notes.md` lower
the headline — trading one gaming vector for a worse one.

The report names an `.ernestignore` whose corpus was **material** — a tenth or
more of what would otherwise have been measured — so an excluded corpus cannot
read as a repository with less prose in it. Below that it is named at `-v`
instead. Materiality rather than verbosity, because behind a level a default run
would under-report in silence, which is the failure the line exists to prevent;
and a corpus that removed one test fixture has not moved the figure it would be
qualifying. The count that decides it costs a second walk, taken only when an
`.ernestignore` was found: the `ignore` crate drops a match before yielding it
and never descends into an excluded directory, so there is no cheaper answer.

This is the narrow case. Most projects never write one.

Every file carries a `Provenance` of `tracked` or `local`, and the report
crosses it with cohort — the prose in a second brain is loaded on every context
load, and the prose in committed docs is not, so summing them hides the thing
worth seeing. Provenance is read from `.git/info/exclude` directly rather than
inferred from the walk, because `ignore` exempts a walk root from its own rules:
without that, `ernest .claude --scope shared` would report the very files a
clone cannot see.

## Output

Three registers. A **headline** that always shows — the figure, and under it the
docs comparator when the `Docs` cohort carries prose. A **body** that shows only
what `--by` names. **Notes**, each firing on its own condition. `report/blocks`
joins them with exactly one blank line and drops the empty ones, so a view that
produced nothing leaves no gap behind it.

**A bare run has no body.** Every repository-wide breakdown is a stationary
statistic: it describes the tree rather than the change just made to it, so the
same rows lead it before and after the work. On the call site that dominates —
an agent measuring its own edit — that costs tokens on every invocation and
reads as though it were the answer. A session drilling into a whole repository
asks for a breakdown on purpose and can afford the second call.

```
--by file           one row per file, most prose first
--by section        one row per innermost heading of a document
--by cohort         the total and the cohorts it sums
--by language       the same, decomposed one level further
--top N             rows in each ranked view; 0 shows every row   [default 20]
--changed[=REF]     rank only what differs from REF               [default HEAD]
--focus PATHSPEC    rank only the paths matching
```

Comma-delimited and composable, and `--by` means the same thing to `ernest diff`
as it does to a measurement — the flags are global, so either side of the
subcommand parses. `--by language` contains `--by cohort`, so asking for both is
asking for the deeper one.

### The ranking is not the headline's scope

`--by file` ranks the repository. That is the right scope for the headline —
relocation-invariance needs it — and the wrong one for the ranking. On the call
site that dominates, an agent measuring before and after its own edit across
fifty files, a repository-wide ranking is stationary: the same large documents
lead it either way, so it says nothing about the change.

So the two split. **`--changed` and `--focus` narrow the ranked views and
nothing else** — the headline, the cohorts and `files_scanned` stay
repository-wide. They intersect, and either implies `--by file` when no `--by`
was named, since a ranking scope with no ranked view scopes nothing visible.

`--focus` takes gitignore glob syntax, the same dialect `.ernestignore` uses, so
`*.php` matches at any depth while `docs/**` is anchored at the working
directory. `--changed` asks git rather than reimplementing it — the same
delegation `.gitignore` gets — with **two-dot** semantics: `git diff <REF>`
against the working tree, plus the untracked files beside it. Merge-base
(three-dot) would answer "what this branch added" and drop uncommitted work,
which is the very thing the measure-edit-measure loop weighs. Its value must be
attached (`--changed=main`), because a bare path follows it.

git is invoked through `relic_core::tool::Tool` and deliberately **not** through
`relic_core::git::Git`. `Git` strips the ambient `GIT_*` environment, and here
that environment is the answer: `GIT_DIR` and `GIT_WORK_TREE` must work by
themselves, or `--changed` would mean something other than what `git diff` means
in the same directory — which is the whole point of delegating. What `Tool`
supplies is the rest, and one piece of it is load-bearing: the `C` locale. The
one failure ernest explains rather than relays is recognised by matching git's
own `not a git repository`, and under a translated locale that match would
silently fail and take the yadm hint with it.

A ranking scope is recorded in the snapshot. Without that, a scoped `files` is
indistinguishable from a smaller repository, and `ernest diff` across a scoped
and an unscoped snapshot bills every out-of-scope file as a full-weight
deletion; it is refused row by row and allowed at the headline, which is
unscoped by construction.

The note under a scoped ranking gives **volume**, never a scoped density. A
per-change density is not relocation-invariant — prose moved out of a changed
file into an untouched one would lower it for free — which is the vector the
summed headline was built to close.

Sections are keyed `path#Heading > Subheading` and exist for the `Docs` cohort,
where a file is too coarse a unit — a 90 KB spec that is 98% prose says nothing
about where to cut. Every byte belongs to its innermost enclosing section, so the
per-character invariants still hold across the rows.

Only `--by section` costs anything: it swaps `analyze_file` for
`analyze_sections` on `Docs` files. `file` is a map and a sort over results
already in hand, and `cohort` and `language` are free — the roll-up is built on
every run, because the headline needs the total and the docs comparator needs the
cohorts.

### Three channels, three promises

`-f`/`--format {text,json,value}`, with `--json` as the short spelling of the
second.

- `--json` **is the contract.** `schema_version` moves when it changes, and
  `ernest diff` refuses a snapshot from another schema.
- `--format value` is one line and one promise: the density as a bare number,
  `40.4`, or `n/a`. It is what a hook or a `--max-density` gate reads, and
  `--max-density` takes the same dialect back. On a diff it is the delta in
  percentage points.
- **The text report carries no stability promise at all.** Columns, wording and
  which lines appear may change in any release. That is git's porcelain
  convention, and it buys the same thing: freedom to keep making the default
  output better without breaking a caller, because a caller parsing it was told
  not to. The render snapshots are what stop that freedom becoming drift — every
  change to the text is a diff someone accepted.

Output carries no colour, on purpose. The primary reader pays for every ANSI byte
and gains nothing from one, and a palette conditional on a TTY would make the
output bimodal for the benefit of the caller that matters least.

### How loud, on one axis

```
-q      the figure, and only what qualifies a block that was printed
        the figure and the caveats on how to read it              [default]
-v      provenance — roots, scope, the tracked/local split, the whole
        unsupported histogram, and any --lang narrowing
-vv     per-file diagnostics — every path set aside and why, and which
        profile read each ranked file
-vvv    parse-level — what each grammar could not read
```

`-q` and `-v` are **one counted axis**, netting off and clamping silently at
both ends, as `ssh` and `rsync` spell it. `-q` naming a *format* would make it
the one flag here a caller could not reason about from any other tool, so
`--format value` is the only spelling of the bare number.

Verbosity is **presentation, and text only**. `--json -vvv` writes the same
bytes as `--json`, and `--format value` is one line whatever the level — but
both are *accepted*, never refused, so a wrapper can pass a flag set it did not
assemble. That is possible because the snapshot is full-fidelity at every level:
the grammar tally, the failure paths and the exclusion count are always in it.

`--by` is a different kind of flag and the distinction is load-bearing: it
selects what is **computed**, so `--json` and `--json --by file` legitimately
differ in shape. Verbosity selects what is **said** about what was computed, and
never changes the shape of anything.

Unbounded per-file lists — a `--scope all` PHP repository passes over sixty
thousand unsupported files — live in `report::Diagnostics`, which is text-only
and collected only at `-vv`. That is the one thing allowed to key on verbosity.
Anything reaching `Report` is collected whatever the flags say, or the
contract's shape starts depending on how loud the caller asked to be.

## Adding a format

One `Profile` constant in `src/analyze/profiles.rs`, listed in `PROFILES`:

```rust
pub static YAML: Profile = Profile {
    language: "yaml",
    language_fn: tree_sitter_yaml::LANGUAGE,
    extensions: &["yaml", "yml"],
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["comment"],
    code_nodes: &[],
    ignored_nodes: &["---", "..."],
    comment_frame: &["#"],
    annotation_line: &[],
    generated_regions: &[],
    pragma_prefixes: &["yaml-language-server:", "yamllint ", "prettier-ignore"],
};
```

`pragma_prefixes` holds the directives that language's toolchain produces. They
belong to the profile rather than to a global, so one language's linter cannot
reclassify another language's comment — a YAML file mentioning `phpcs:` is
talking about PHP, not obeying it. Only what genuinely reaches every format —
an SPDX identifier, an editor modeline — goes in `UNIVERSAL_PRAGMA_PREFIXES`,
and that list is closed.

`interpreters` names the shebang basenames identifying a script that carries no
extension. A file that has one is never second-guessed: an extension is the
author's declaration of format, and only its absence justifies opening the file.

Opening with `#!` is not what makes a shebang — naming an interpreter by
absolute path is, which is why `detect::shebang_len` is the predicate and not a
string test. Rust spells an inner attribute `#![deny(missing_docs)]`, the most
common first line in the language, and writing that off as an unavoidable header
would be silent and wrong.

`generated_regions` pairs the pragma bodies that open and close a region a tool
rewrites — `<!-- TOC -->` and `<!-- /TOC -->`. The pair and everything between it
is uninteresting whatever it holds, because nobody authored it. The rule is a
post-pass over the span list, not a walk rule, since the interior often produces
no spans of its own; it generalises to `@generated` markers in source.

Then add the grammar crate to `Cargo.toml`, and a fixture under
`tests/fixtures/<language>/` with the adversarial cases for that format — the
strings a regex-based classifier would misread. Write a bespoke `Analyzer` only
when a profile genuinely cannot express the format.

### One language may own more than one profile

`language` is what the table reports, not what the registry is keyed on. When a
dialect needs a second grammar but is not a second language, give both profiles
the same `language` and let `aggregate.rs` fold them into one row — it keys on
that string. `TYPESCRIPT` and `TSX` are the case: tree-sitter ships two grammars
because `<T>x` is a type assertion in TypeScript and an element in TSX, and
forcing the `.tsx` fixture through the plain grammar produces 17 `ERROR` nodes.
That is a fact about parsers, not a distinction a reader of the breakdown wants.

Lookups by extension stay unambiguous. Lookups by *name* — `--lang`, and the
`kinds` example's `--lang` — take the first profile carrying it, so list the
dialect that needs no special grammar first in `PROFILES`, and give `kinds` a
path rather than a `--lang` when the extension can choose.

To find a grammar's node kinds, print the tree for a sample; the kinds are
whatever `Node::kind()` returns, anonymous tokens (`---`) included. The kinds a
grammar's `grammar.js` suggests are not always the kinds it produces — Rust
defines a `comment` rule that is unreachable — so read the tree, not the source:

```
cargo run --example kinds -- path/to/sample          # profile by extension
cargo run --example kinds -- --lang toml some-file   # or forced
```

## Verification

```
scripts/test.sh          # fmt, clippy, then the suite — fail-fast
```

One command for humans and agents; `relic test ernest` runs the same file. Stock
rustfmt — the empty `rustfmt.toml` says so deliberately — and clippy at
`-D warnings`. No flag drops a station; `cargo fmt --all` fixes the first one.

Unit tests in `src/` pin each rule's exact semantics. `tests/golden.rs` guards
the rules working together against blessed expectations, and adds three
invariants that hold whatever the rules decide — so they catch bugs the
expectations would absorb:

- every non-whitespace character is bucketed exactly once;
- no line is counted twice;
- no fixture parses to an `ERROR` node. A grammar that cannot read a file still
  returns a tree, and the rules still classify it, so this is the only thing
  standing between a borrowed grammar and a blessed wrong answer. It is what
  decides the open zsh question in `TODO.md`.

The `tests/fixtures/<language>/` directory name must equal `profile.language`:
`tests/cli.rs` derives the languages it expects in the breakdown from those
names, so a new fixture is covered the day it lands rather than the day someone
widens a list.

`tests/render.rs` guards the rendered output against blessed snapshots over the
frozen trees in `tests/render/`, plus layout invariants asserted on every case
rather than eyeballed in each snapshot: no two blank lines in a row, no trailing
whitespace, exactly one final newline, no absolute path. `tests/cli.rs` asserts
what a report *means* and `tests/render.rs` what it *looks like* — a roll-up that
stops summing and a stray blank line are different failures, and neither suite
should be able to absorb the other's.

The render trees are frozen deliberately. `tests/fixtures/` grows every time a
format lands, so snapshots over it would churn on changes that are not about
rendering and drown the diff someone is meant to read.

Both suites are `insta` snapshots. Accept a deliberate change with `cargo insta
review`, which shows one diff at a time and takes them one at a time — read each,
because an accepted wrong answer is still wrong. The golden suite runs under
`insta::glob!`, so a change that moves several fixtures reports all of them in one
run instead of stopping at the first.

## Known imprecisions

- **An unwritten profile skews the headline rather than abstaining from it.**
  Because every cohort is summed, a language ernest cannot read drops out of the
  denominator and pulls the figure toward whichever cohort *is* covered — a
  repository whose only covered format is Markdown reads as
  documentation-dominated, not as unmeasured. Writing the rust and toml
  profiles moved this repository from 82.5% to 34.3% with no text changing, and
  the javascript and typescript profiles took an Angular front-end from 6.3% to
  0.3% — the first figure was the density of the 46,425 characters of Markdown
  and YAML that survived the gap, in a repository holding 6.5 million. The
  report tallies skipped files by extension so the gap is visible and names the
  profile to write next; the format roadmap in `TODO.md` is the queue for
  closing it. The gap does not always flatter: css, html, twig and xml took a
  Symfony front-end from 25.9% to 26.2%, because the 147 files they added were
  denser than the repository around them — its stylesheets read 47.5%.
- **Three of the borrowed grammars are a release behind their language**, which
  shows as `ERROR` nodes on real files: `@media (width >= 48rem)` and
  `@container name (…)` in CSS, `{% props a, b = 'x' %}` and Twig's else-less
  ternary, and Angular's `@if (x > 0) {` in an HTML template. Measured on
  Pillar and the admin front-end, that is 4 of 50 stylesheets, 7 of 94
  templates and 6 of 307 Angular templates. Every one of them sits in an
  expression or an at-rule prelude, where nothing is ever prose — a sweep of
  all 455 files recovered every comment character exactly, so the confusion
  costs tree shape rather than classification. That is what separates these
  from zsh, where the bash grammar loses about an eighth of the comment
  characters and bills glob flags as prose. Grammar-health reporting, queued in
  `TODO.md`, is what would make this visible without a hand sweep.
- **The template and markup profiles cannot see into what they embed.** Twig's
  grammar is template-first and hands all markup back as one `text` node;
  HTML's hands an inline `<script>` or `<style>` back as `raw_text`. So an
  `<!-- -->` inside a `.twig` file, and a comment inside an inline script or
  stylesheet, bill as code. One gap rather than three, and the fence injection
  queued in `TODO.md` closes all of it.
- An HTML `comment` is an `extra`, so the scanner emits one inside a quoted
  attribute value too: `data-x="<!-- x -->"` bills as prose. Contrived, and no
  file in any repository here has one.
- CSS's `/*! preserved banner */` counts as prose. It is the licence-header
  convention rather than a directive, and `!` is too blunt a prefix to key on —
  the licence-header item in `TODO.md` is what claims it.
- A doctest fence inside a Rust doc comment counts as prose, code and all. It is
  compiled, runnable code, and the general fix is the fence injection queued in
  `TODO.md` rather than a Rust-only second parse.
- A comment inside an attribute (`#[derive(\n // note\n Debug)]`) counts as
  code, because the walk stops at `attribute_item`. Legal, and rare enough that
  reaching it is not worth losing the pragma rule that listing buys.
- An annotation line carrying trailing prose (`@deprecated Use Foo instead.`) is
  billed wholly as code. Splitting mid-line is possible; it was not worth v1.
- A pragma is recognised from a comment's **first non-empty line only**, and it
  then reclassifies the node whole. So `// @ts-expect-error` is uninteresting,
  while the same directive buried mid-docblock stays code. Both readings are
  defensible and the first line is where these are actually written.
- JavaScript's Annex B `<!--` line comment is named as prose but cannot appear
  in a fixture: it is script-only syntax, so it errors beside an `export`. The
  unit test carries it instead.
- A fully annotated docblock slightly *dilutes* density, since annotations count
  as code. The alternative makes the metric fight PHPStan.
- Density does not compare across languages — brace-heavy languages read lower.
- `--unit lines` resolves a split line by dominant class, so a code line with a
  long trailing comment reads as prose.
- **`--changed` needs a discoverable git repository**, which a yadm-managed tree
  is not: the work tree is `$HOME` and the git dir lives elsewhere, so `git -C
  ~/.config rev-parse` fails there. `GIT_DIR` and `GIT_WORK_TREE` are the escape
  hatch and the error names them. ernest never sets them itself — that would be
  guessing at git's job, and would make `--changed` mean something other than
  what `git diff` means in the same directory.
- `--focus` matches the path as the report prints it, so a pathspec with a slash
  in it is anchored at the working directory rather than at a walk root.
- A git worktree checked out under a locally-excluded path is walked at the
  default scope and double-counts the repository. The project-side fix is to
  gitignore the agent's runtime state, which makes it noise rather than second
  brain.
- Front-matter is uninteresting whole, so a `description:` field an agent does
  read goes uncounted.
- A `link_reference_definition` counts as prose, URL included.
- Markdown inline structure is not parsed. Code spans and link destinations stay
  prose, which is the wanted answer and costs nothing; separating them would
  need the grammar's second, inline pass.

## Next steps

`TODO.md`, alongside this file. It is read on demand; this one is loaded on
every context load, and a backlog does not earn that.
