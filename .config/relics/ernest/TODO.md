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
was measuring the Markdown that survived the gap. Coverage is therefore
load-bearing, not garnish.

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
- **xml** (39) — `<!-- -->`.
- **python** — only 8 files here, but a bedrock member and the uv lane's
  language, so it will not stay at 8. The next one up, and the first format
  whose prose is not delivered by a comment: a module, class or function
  docstring is prose, but the grammar gives it as an `expression_statement`
  holding a `string`, indistinguishable by kind from any other string literal.
  So it needs position, not kind — first statement of a module or of a
  `function_definition` / `class_definition` body — which is a rule the
  `Profile` shape cannot express and the first real case for a bespoke
  `Analyzer`. `#` comments and the `noqa` pragma already work; `# type:` is
  already a pragma prefix. Type annotations and decorators are code.
- **html** (20), **css** / **scss** (11), **javascript** / **typescript** (7,
  plus the benefactor front-ends).
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
- **rst**, **adoc**, **txt** — the `Docs` cohort. reStructuredText is the
  Python world's documentation format and pairs with the python profile.
- **ipynb** — JSON carrying Markdown cells. Needs a bespoke `Analyzer` rather
  than a profile, because the prose lives inside JSON string arrays and has to
  be reassembled before it can be measured. Plausibly the highest-density
  format in common use, and today entirely invisible.

---

## Grammar-health reporting

`every_fixture_parses_without_error_nodes` proves the *fixtures* parse. Nothing
says so about the corpus. A file whose grammar produced `ERROR` nodes has been
measured against the parser's confusion, and it reports as an ordinary row —
which is exactly the failure mode the zsh question is about, generalised to
every profile that borrows a neighbouring dialect's grammar. The report already
tallies files it could not *identify*; it should also tally files it could not
*read*.

## Per-profile pragma prefixes

`PRAGMA_PREFIXES` is a flat global that now carries Rust attribute syntax only
Rust can produce, alongside `phpcs:`, `shellcheck ` and `yaml-language-server:`.
Nothing collides today and the cross-language matching is harmless, but the list
is the wrong shape: a language's directives belong to its profile. Moving them
is mechanical, and the moment to do it is when a prefix in one language would
misread a comment in another.

---

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

## Amend `deprose.md`

To name the metric, once it has proven itself in real use. It currently says
"this is not a quantified metric, this is a mantra".
