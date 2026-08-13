# ernest TODO

Single live queue for ernest work.

> **House rule.** Completed items are **deleted** from this file, not archived —
> `yadm log` is the record of what was done. No DONE section. Keep each item
> independently sized so it can be picked up alone.

---

## Format roadmap

The headline sums every cohort, so a language ernest cannot read does not
abstain from the figure — it skews it, by dropping out of the denominator while
the covered cohorts keep their weight. A Rust repository reads as
documentation-dominated. Coverage is therefore load-bearing, not garnish.

Adding one is a `Profile` constant in `src/analyze/profiles.rs` listed in
`PROFILES`, the grammar crate in `Cargo.toml`, and a fixture under
`tests/fixtures/<language>/` carrying the strings a regex classifier would
misread. See "Adding a format" in `CLAUDE.md`.

Counts below are from a census of `~/Developer` git repos plus reliquary's
tracked set — evidence for the ordering, not targets.

### Tier 1 — the headline is not honest without these

- **`.phpstub` onto the existing PHP profile.** 57 files, one word in
  `extensions`. Cheapest item on the board; do it first.
- **rust** — 707 files, plus ernest's own ~2,900 lines, which it cannot
  currently measure. `///` and `//!` are prose; `#[derive]` and friends are
  code (avoidable, but not prose); `#![allow]` is a pragma candidate.
- **toml** — 262 files. Comments are the only prose; `Cargo.toml` is everywhere.
- **fish** — 14 tracked files, but structurally *half* the shell corpus:
  `check-shell-parity` pairs every `shell/interactive.d/*.sh` with a
  `fish/conf.d/*.fish`, so measuring only the POSIX half is a systematic blind
  spot rather than a small one.
- **zsh** — 4 `.zsh` plus `.zshrc` / `.zshenv` / `.zprofile`, the primary
  interactive shell here and currently unmeasured whole. Likely just
  `extensions` and `filenames` on the existing `SHELL` profile using the bash
  grammar — but that is the open question, and the fixture has to prove
  zsh-isms do not parse to `ERROR` nodes that misclassify. If they do, it needs
  its own grammar.

### Tier 2 — common in target repos

- **json** (88) and **jsonc**. Plain JSON has no comments, so the value is
  denominator accuracy rather than prose; jsonc (tsconfig, VS Code) does.
- **xml** (39) — `<!-- -->`.
- **python** — only 8 files here, but a bedrock member and the uv lane's
  language, so it will not stay at 8. Docstrings need a rule of their own: a
  module or function docstring is prose, but it is also an expression statement.
- **html** (20), **css** / **scss** (11), **javascript** / **typescript** (7,
  plus the benefactor front-ends).
- **neon** (5) — PHPStan configuration. YAML-adjacent; check whether the YAML
  grammar reads it acceptably before reaching for another crate.

### Tier 3 — opportunistic

- **justfile** — a bedrock member, so it is in every repo that has one at all.
  Extensionless: the `filenames` field already covers that shape.
- **dockerfile**, **sql**, **lua**, **vimscript** (`vim/vimrc` is tracked),
  **mdx** (11, and it belongs in `Docs`).

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
