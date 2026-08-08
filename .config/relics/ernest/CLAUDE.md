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
ernest --json --by-file > before.json
… make the change …
ernest --json --by-file > after.json
ernest diff before.json after.json
```

Measure, change, look at where the difference came from, act if there is
something to act on.

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

## Architecture

Byte-span classification, not line bucketing — which is what tree-sitter hands
back natively.

```rust
pub enum Class { Prose, Code, Ignored }
pub struct Span { start: usize, end: usize, class: Class }
```

An analyzer names only the spans it recognises; everything uncovered falls to
`Profile::default_class`. For source languages that is `Code`, so an analyzer's
whole job is finding prose. A documentation format inverts the default to
`Prose` and names code blocks instead — same contract, no new machinery.

```
src/
  span.rs             Class, Span, coverage fill, measurement in both units
  analyze/mod.rs      tree walk, pragma rule, annotation post-pass, shebang rule
  analyze/profiles.rs the format registry
  detect.rs           path -> profile
  walk.rs             ignore::WalkBuilder, default excludes
  aggregate.rs        file -> language -> cohort -> report
  report/             human, json, diff
```

`Cohort` splits `Source` from `Docs` and the two are never summed. Only `Source`
exists today; the split has to be there before the first documentation format
lands, or the headline silently absorbs prose that would swamp it.

## Adding a format

One `Profile` constant in `src/analyze/profiles.rs`, listed in `PROFILES`:

```rust
pub static YAML: Profile = Profile {
    language: "yaml",
    language_fn: tree_sitter_yaml::LANGUAGE,
    extensions: &["yaml", "yml"],
    filenames: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["comment"],
    ignored_nodes: &["---", "..."],
    comment_frame: &["#"],
    annotation_line: &[],
};
```

Then add the grammar crate to `Cargo.toml`, and a fixture under
`tests/fixtures/<language>/` with the adversarial cases for that format — the
strings a regex-based classifier would misread. Write a bespoke `Analyzer` only
when a profile genuinely cannot express the format.

To find a grammar's node kinds, parse a sample and print the tree; the kinds are
whatever `Node::kind()` returns, anonymous tokens (`---`) included.

## Tests

```
scripts/test.sh          # cargo nextest, falling back to cargo test
```

Unit tests in `src/` pin each rule's exact semantics. `tests/golden.rs` guards
the rules working together against blessed expectations, and adds two invariants
that hold whatever the rules decide — so they catch span bugs the expectations
would absorb:

- every non-whitespace character is bucketed exactly once;
- no line is counted twice.

Regenerate expectations after a deliberate change with
`ERNEST_BLESS=1 cargo test --test golden`, then **read the diff** — a blessed
wrong answer is still wrong.

## Known imprecisions

- An annotation line carrying trailing prose (`@deprecated Use Foo instead.`) is
  billed wholly as code. Splitting mid-line is possible; it was not worth v1.
- A fully annotated docblock slightly *dilutes* density, since annotations count
  as code. The alternative makes the metric fight PHPStan.
- Density does not compare across languages — brace-heavy languages read lower.
- `--unit lines` resolves a split line by dominant class, so a code line with a
  long trailing comment reads as prose.

## Next steps

- **Markdown**, as the first `Docs`-cohort format. Its default class is `Prose`
  and fenced code blocks are what get named; the ``` fence lines are
  uninteresting, being unavoidable once a block exists.
- **Committed baselines** — an `.ernest-baseline` file and `ernest check`, so
  the before/after loop survives across sessions without carrying snapshots by
  hand.
- **`--unit tokens`.** Characters are a proxy for the cost that actually
  motivates this. The `Counts` shape already carries two units; a third is
  additive.
- **Licence-header detection**, so an unavoidable SPDX block is uninteresting
  rather than prose. Single `SPDX-License-Identifier:` lines are already handled
  by the pragma rule; multi-line blocks are not.
- **Promotion to Stage 3** once the format list branches out — this is a crate
  that will want its own history, benches and CI. See `GRADUATION.md`.
- **Amend `deprose.md`** to name the metric, once v1 has proven itself in real
  use. It currently says "this is not a quantified metric, this is a mantra".
