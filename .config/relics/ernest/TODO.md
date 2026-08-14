# ernest TODO

Single live queue for ernest work.

> **House rule.** Completed items are **deleted** from this file, not archived —
> `yadm log` is the record of what was done. No DONE section. Keep each item
> independently sized so it can be picked up alone.

---

## Format roadmap

The headline sums every cohort, so a language ernest cannot read does not
abstain from the figure — it skews it, by dropping out of the denominator while
the covered cohorts keep their weight. Adding rust and toml moved this
repository from 82.5% to 34.3% without a character changing; the first figure
was measuring the Markdown that survived the gap. Adding javascript and
typescript took an Angular front-end from 6.3% to 0.3% — 99.3% of that
repository had been invisible. Coverage is therefore load-bearing, not garnish.
It does not only ever flatter: css, html, twig and xml moved a Symfony
front-end from 25.9% to 26.2%, because the 147 files they let in were denser
than the repository around them — the stylesheets read 47.5%.

A census is a snapshot and ages badly: this file rated javascript/typescript
Tier 2 on a count of **7 files**, against the ~7,000 that were actually there.
Re-count before trusting an ordering below.

Adding one is a `Profile` constant in `src/analyze/profiles.rs` listed in
`PROFILES`, the grammar crate in `Cargo.toml`, and a fixture under
`tests/fixtures/<language>/` carrying the strings a regex classifier would
misread. `cargo run --example kinds -- <file>` prints the tree the profile has
to be written against. See "Adding a format" in `CLAUDE.md`.

Counts below are from a census of `~/Developer` git repos plus reliquary's
tracked set — evidence for the ordering, not targets.

### Tier 1 — the headline is not honest without these

- **fish** — 14 tracked files, but structurally *half* the shell corpus:
  `check-shell-parity` pairs every `shell/interactive.d/*.sh` with a
  `fish/conf.d/*.fish`, so measuring only the POSIX half is a systematic blind
  spot rather than a small one.
- **zsh** — 4 `.zsh` plus `.zshrc` / `.zshenv` / `.zprofile`, the primary
  interactive shell here and currently unmeasured whole. Likely just
  `extensions` and `filenames` on the existing `SHELL` profile using the bash
  grammar — but that is the open question, and the fixture has to prove
  zsh-isms do not parse to `ERROR` nodes that misclassify. If they do, it needs
  its own grammar. `every_fixture_parses_without_error_nodes` in
  `tests/golden.rs` is what decides it: add the fixture and read the verdict.

### Tier 2 — common in target repos

- **json** (88) and **jsonc**. Plain JSON has no comments, so the value is
  denominator accuracy rather than prose; jsonc (tsconfig, VS Code) does.
- **scss** (286, all in one Angular front-end). Blocked rather than merely
  unwritten, and blocked the way fish is: both `tree-sitter-scss` and the
  `cortexkit-` fork still expose the pre-`LanguageFn` API,
  `pub fn language() -> Language`, which `Profile.language_fn` cannot hold.
  Reaching the raw `tree_sitter_scss` symbol by redeclaring the extern would
  work and is not worth the unsafe FFI; wait for a crate that has moved, or
  vendor the grammar. Do **not** widen the css profile's extensions instead —
  `$var`, `@mixin` and `@include` all error under the CSS grammar, which is
  what `declines_everything_else` in `src/detect.rs` pins.
- **python** — only 8 files here, but a bedrock member and the uv lane's
  language, so it will not stay at 8. The next one up, and the first format
  whose prose is not delivered by a comment: a module, class or function
  docstring is prose, but the grammar gives it as an `expression_statement`
  holding a `string`, indistinguishable by kind from any other string literal.
  So it needs position, not kind — first statement of a module or of a
  `function_definition` / `class_definition` body — which is a rule the
  `Profile` shape cannot express and the first real case for a bespoke
  `Analyzer`. `#` comments work already; `noqa` and `type:` are the profile's
  `pragma_prefixes` when it lands, having come out of the old global list with
  nowhere to go. Type annotations and decorators are code.
- **neon** (5) — PHPStan configuration. YAML-adjacent; check whether the YAML
  grammar reads it acceptably before reaching for another crate.

### Tier 3 — opportunistic

- **justfile** — a bedrock member, so it is in every repo that has one at all.
  Extensionless: the `filenames` field already covers that shape.
- **dockerfile**, **sql**, **lua**, **vimscript** (`vim/vimrc` is tracked),
  **mdx** (11, and it belongs in `Docs`).

### Tier 4 — portability, not census

The tiers above are ordered by evidence from `~/Developer`. These have none
there and are not ranked against them; they are here because a tool that
graduates to Stage 3 gets pointed at repositories this machine has never held,
and a language ernest cannot read skews rather than abstains. Any one of them
becomes Tier 1 the day such a repository turns up.

- **go** — `//` and `/* */`, and `//go:generate` / `//go:build` fall out of the
  existing pragma rule as prefixes. Godoc is an ordinary comment above the
  declaration, so there is no doc-comment kind to decide about. Note that
  `vendor` is already in `walk.rs`'s default excludes, which matters more here
  than anywhere else.
- **java**, **kotlin** — Javadoc and KDoc are `@param`-annotated exactly as
  PHPDoc is, so `annotation_line: &["@"]` transfers verbatim. Java annotations
  (`@Override`) are code, and they are not inside a comment, so nothing
  collides.
- **c**, **cpp**, **csharp** — Doxygen `///` and `/** */`. C# is the one that
  needs a decision the others do not: its doc comments carry XML markup, so
  `<summary>` tags are structure inside prose, which is the Markdown-table
  question in a different costume.
- **ruby** — `#`, `=begin` / `=end`, and YARD's `@param`.
- **swift** — `///` and `/** */`, plus `// MARK:` section banners, which are
  navigation furniture rather than prose and belong with the pragmas.
- **hcl** / **terraform** — worth measuring but read the number carefully: an
  infrastructure repository is configuration nearly end to end, so the
  denominator is thin and density runs high for reasons that are not prose.
- **make** — `#`; recipe lines and `.PHONY` are code.
- **powershell** — comment-based help (`.SYNOPSIS`, `.PARAMETER`, `.EXAMPLE`)
  is a large, formulaic, entirely re-derivable prose vector, and it is
  annotation-shaped, so `annotation_line` may take it directly.
- **graphql**, **protobuf** — `"""` descriptions and `//` respectively.
- **jinja**, **nunjucks**, **django** — `tree_sitter_jinja_dialects` already
  parses the union of the family, so each is a second `Profile` constant over
  the grammar `TWIG` loads and nothing else. Not written because there is no
  `.j2`, `.jinja` or `.njk` file on this machine to verify one against.
- **rst**, **adoc**, **txt** — the `Docs` cohort. reStructuredText is the
  Python world's documentation format and pairs with the python profile.
- **ipynb** — JSON carrying Markdown cells. Needs a bespoke `Analyzer` rather
  than a profile, because the prose lives inside JSON string arrays and has to
  be reassembled before it can be measured. Plausibly the highest-density
  format in common use, and today entirely invisible.

---

## Grammar health — what to do about it

The **measurement** landed: `report.grammar` tallies unread files, error nodes
and missing nodes per language on every run, `-vvv` prints the table, and `-vv`
names the paths. The sweeps below took two throwaway scripts each; they are now
a flag.

What is left is the judgement the numbers were gathered for. On this machine
they read: **0 of 7,048** hand-written front-end files error under the
JavaScript and TypeScript grammars, while **4 of 50** stylesheets, **7 of 94**
Twig templates and **6 of 307** Angular templates do — on `@media (width >=
48rem)`, `@container name (…)`, `{% props a, b = 'x' %}`, Twig's else-less
ternary, Angular's `@if (x > 0) {`. Every one sits in an expression or an
at-rule prelude, where nothing is ever prose, and a sweep comparing ernest's
prose against a regex over each format's comment delimiters found a shortfall of
**0 characters across all 455 files**. So today the confusion costs tree shape
rather than classification, and the answer is to do nothing.

Revisit when a tally says otherwise — in particular for a profile that borrows a
neighbouring dialect's grammar, which is what the open zsh question in the
format roadmap is really asking. The options, when it comes to that: upgrade the
grammar, vendor a fork, or give the dialect its own profile.

---

## Change-scoped ranking

`--by file` ranks the repository. That is the right scope for the headline —
relocation-invariance needs it — and the wrong one for the ranking, and today
they are the same scope. On the call site that matters, an agent measuring
before and after its own edit across fifty files, a repository-wide ranking is
stationary: the same large documents lead it either way, so it says nothing
about the change. Making the body opt-in was the interim answer; splitting the
two scopes is the real one.

Two shapes, sketched and unbuilt:

- `--changed [REF]` — rank only what differs from `REF`, default `HEAD`, while
  the headline stays repository-wide. git already answers this, so ernest asks
  it rather than reimplements it — the same delegation `.gitignore` gets.
- `--focus <pathspec>` — rank only what matches, for a change that is not yet a
  commit or spans a directory rather than a diff. `--changed` is then sugar
  over it.

Neither touches the headline. The split to make first is between what is
*computed* (`aggregate::Views`) and what is *ranked*: `aggregate::run` already
holds every `Outcome`, so the ranking scope is a predicate applied before the
`by_file` map, not a second walk.

## Verbosity levels

`--verbose` / `-v`, counted, so the notes register has an axis instead of a
single default. clap's `ArgAction::Count` on a `u8`, global like the other
presentation flags, mapped at once into a named enum — a raw count threaded to
call sites reads as a magic number. Do not take `clap-verbosity-flag`: it exists
to drive a `log` filter, and there is nothing to log here.

Three levels carry distinct content. A fourth would be padding, so cap at three
and clamp a fourth `-v` silently rather than erroring, as `ssh` and `rsync` do.

```
default   the figure, and the notes that change how it should be read
-v        provenance — what was excluded and by whose declaration
-vv       per-file diagnostics — which paths, which failures, which profile
-vvv      parse-level — grammar ERROR tallies, timings
```

`-vvv` is where "Grammar-health reporting" above lands: a corpus-wide `ERROR`
node tally is exactly a level-three diagnostic, and giving it a home is half of
what that item needs.

**Text only.** `--format json` is the contract and its shape cannot depend on a
presentation flag; `--format value` is one line by definition. Verbosity applies
to neither, and asking for it with either should be refused rather than ignored.

**`-q` is not the other end of this axis.** In most tools `-q` and `-v` are one
scale. Here `--quiet` is a *format* — the bare density, for a gate — so the two
are orthogonal and a caller cannot walk from one to the other. Either accept
that and say so in `--help`, or rename the format (`--format value` already
spells it) and free `-q` to mean one step down the verbosity scale. Decide
before implementing; the second is closer to what a reader expects, and costs
one alias.

### Moving `.ernestignore` behind a level

The ask, and it reverses a commitment worth restating before it goes: *the
report names any `.ernestignore` in effect, so an excluded corpus cannot read as
a repository with less prose in it.* Behind `-v`, a default run under-reports
silently, which is the failure that line was written to prevent.

The motivating case is real — a test fixture's declared corpus appearing in
every measurement of the tree above it — but it argues that an exclusion
removing almost nothing should not announce itself, not that exclusions should
go quiet. So prefer **materiality over verbosity**: name the corpus at the
default level when it is large enough to matter, and at `-v` otherwise.

That needs a number the walk does not currently keep. Excluded paths are never
measured, so their prose is unknown — but `walk::collect` sees every path an
`.ernestignore` matched and can count them, and a count against `files_scanned`
is enough to separate "one fixture" from "half the repository". Cheap, no second
walk, no schema change beyond a field.

If that is judged more machinery than the line is worth, `-v` is the acceptable
fallback — but then amend the `.ernestignore` section of `CLAUDE.md` in the same
change, or the doc keeps promising something the tool no longer does.

## Committed baselines

An `.ernest-baseline` file and `ernest check`, so the before/after loop survives
across sessions without carrying snapshots by hand.

## `--unit tokens`

Characters are a proxy for the cost that actually motivates this. The `Counts`
shape already carries two units; a third is additive.

## Licence-header detection

So an unavoidable SPDX block is uninteresting rather than prose. Single
`SPDX-License-Identifier:` lines are already handled by the pragma rule;
multi-line blocks are not.

## Fence injection

`INJECTION_QUERY_BLOCK` would let a ```php fence be measured by the PHP profile
and counted toward `Source`, making the cohort split follow the content rather
than the file extension.

## Promotion to Stage 3

Once the format list branches out — this is a crate that will want its own
history, benches and CI. See `~/.config/reliquary/GRADUATION.md`.
