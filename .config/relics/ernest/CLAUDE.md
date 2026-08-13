# ernest

Measures **prose density**: the share of a codebase's text that is prose rather
than code. Stage-2 Reliquary relic; see `~/.config/reliquary/GRADUATION.md` for
the lifecycle it lives in.

Prose density is the canonical term. `ernest` (for Hemingway) is only the tool's
name — there is no "Ernest Index".

## Mission

Agents leave prose in files despite every directive to be terse, and most of it
is re-derivable on demand: it is paid for on every context load and repaid
rarely. `~/.claude/commands/modes/deprose.md` makes that a mantra. This makes it
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
The `source` row still does, and the table rolls up through it.

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
  not across them, so the per-language table is the primary view and the total
  reads as "this repository's mix".
- **Whitespace is never counted**, so the bytes between spans need no rule.

The same line runs through Markdown's structure: what scales with the construct
is code, what frames it once is prose. A wide table's pipes are mass that is not
text, so they and the delimiter row bill as code; a heading's `#` exists only
because the heading does, so it bills as prose along with list markers and
blockquote markers. Table cells stay prose, which leaves turning a paragraph
into a table rewarded by the characters it saves and by nothing else.

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
  aggregate.rs        file -> (cohort, provenance, language) -> report
  report/             table, human, json, diff
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

Dependency and build directories are excluded at every level, so widening the
scope cannot drag `vendor/` back in.

### `.ernestignore`

git answers "is this shared". It cannot answer "is this what ernest is for" —
so a repository that carries prose which *is* the product, rather than prose
about the code, declares it: gitignore syntax, honored at **every** scope,
because a corpus is not the subject at any of them.

Declared, never inferred. A rule keyed on doc-like filenames would miss
`AGENTIC-TOOLING.md`, and would make renaming `CLAUDE.md` to `notes.md` lower
the headline — trading one gaming vector for a worse one. The report names any
`.ernestignore` in effect, so an excluded corpus cannot read as a repository
with less prose in it.

This is the narrow case. Most projects never write one.

Every file carries a `Provenance` of `tracked` or `local`, and the report
crosses it with cohort — the prose in a second brain is loaded on every context
load, and the prose in committed docs is not, so summing them hides the thing
worth seeing. Provenance is read from `.git/info/exclude` directly rather than
inferred from the walk, because `ignore` exempts a walk root from its own rules:
without that, `ernest .claude --scope shared` would report the very files a
clone cannot see.

## Views

`--by file,section`. Sections are keyed `path#Heading > Subheading` and exist
for the `Docs` cohort, where a file is too coarse a unit — a 90 KB spec that is
98% prose says nothing about where to cut. Every byte belongs to its innermost
enclosing section, so the per-character invariants still hold across the rows.

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
};
```

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

Regenerate expectations after a deliberate change with
`ERNEST_BLESS=1 cargo test --test golden`, then **read the diff** — a blessed
wrong answer is still wrong.

## Known imprecisions

- **An unwritten profile skews the headline rather than abstaining from it.**
  Because every cohort is summed, a language ernest cannot read drops out of the
  denominator and pulls the figure toward whichever cohort *is* covered — a
  repository whose only covered format is Markdown reads as
  documentation-dominated, not as unmeasured. Writing the rust and toml
  profiles moved this repository from 82.5% to 34.3% with no text changing. The
  report tallies skipped files by extension so the gap is visible and names the
  profile to write next; the format roadmap in `TODO.md` is the queue for
  closing it.
- A doctest fence inside a Rust doc comment counts as prose, code and all. It is
  compiled, runnable code, and the general fix is the fence injection queued in
  `TODO.md` rather than a Rust-only second parse.
- A comment inside an attribute (`#[derive(\n // note\n Debug)]`) counts as
  code, because the walk stops at `attribute_item`. Legal, and rare enough that
  reaching it is not worth losing the pragma rule that listing buys.
- An annotation line carrying trailing prose (`@deprecated Use Foo instead.`) is
  billed wholly as code. Splitting mid-line is possible; it was not worth v1.
- A fully annotated docblock slightly *dilutes* density, since annotations count
  as code. The alternative makes the metric fight PHPStan.
- Density does not compare across languages — brace-heavy languages read lower.
- `--unit lines` resolves a split line by dominant class, so a code line with a
  long trailing comment reads as prose.
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
