//! Lint, format and suppression drift over the shell that stays shell.
//!
//! Bash has no type system, so a refactor here is unverified by construction.
//! Lint and format are the whole budget, and this spends them — the Rust lane's
//! `clippy` and `rustfmt` gates, aimed at the half that is not moving.
//!
//! Three gates, and the third is why there is no register of accepted findings:
//! a lint that is genuinely wrong for this codebase is accepted *at the site*,
//! with an inline `# shellcheck disable=` carrying its reason. That mechanism is
//! invisible to the other two — silencing a finding makes the finding count fall
//! — so the count of those directives is committed per file and compared as an
//! **equality**. Removing one means lowering the number in the same commit. Same
//! control, same semantics, as the Rust lane's suppression ratchet.
//!
//! **Nothing here parses human-facing output.** `shellcheck -f json1` is the
//! machine format, and every finding's file, line, column and code arrive as
//! fields rather than as a line someone split on colons. The retired script read
//! `-f gcc` and counted lines.
//!
//! **Scope is what `ls-files` reports**, so the encrypt lane is out: a baseline
//! for archived bash could not live in a public file without naming what the
//! encrypt patterns exist to hide.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::{Context as _, Result};
use camino::Utf8Path;
use regex::Regex;
use relic_core::finding::{Detail, Finding, FixHint, Outcome, StationId, Summary};
use relic_core::tool::Tool;
use serde::Deserialize;

use crate::repo::{Rel, Repo};
use crate::station::{Context, Station};

/// The committed suppression counts, `$HOME`-relative.
const BASELINE: &str = ".config/reliquary/ratchets/shell-suppressions.toml";

/// The level at and above which a lint is a finding.
const SEVERITY: &str = "warning";

/// Unfollowable sources — a path built at runtime, or one outside the tree.
/// Every one of them is deliberate here.
const UNFOLLOWABLE: &str = "SC1090,SC1091";

/// The house style: four spaces, and case arms indented inside their case.
const STYLE: &[&str] = &["-i", "4", "-ci"];

/// Long enough that neither tool is ever cut off mid-answer over a tree this
/// size, short enough that one which stops answering is a finding rather than a
/// hung `yadm doctor`.
const BUDGET: Duration = Duration::from_secs(60);

/// Suffixes that are shell but not bash. Neither tool can read them, and
/// pretending otherwise would lint a file as something it is not.
const FOREIGN_SUFFIXES: &[&str] = &[".zsh", ".fish"];

/// Whole names in the same position.
const FOREIGN_NAMES: &[&str] = &[".zshenv", ".zshrc", ".zprofile"];

/// Suffixes that are ours regardless of what the first line says.
const OURS_SUFFIXES: &[&str] = &[".sh", ".bash"];

/// Whole names in the same position.
const OURS_NAMES: &[&str] = &[".bashrc", ".bash_profile", ".bash_env"];

/// Test data. A lint written into a fixture is the fixture's whole point.
const FIXTURES: &str = "fixtures";

/// The patterns, compiled once per run.
///
/// Built fallibly rather than through a `LazyLock` that has to unwrap: these
/// are literals in this file, so a failure is impossible — and a construction
/// that cannot fail is worth spelling as one that returns `Result` when the
/// alternative is a suppression.
struct Patterns {
    /// A shebang naming bash or POSIX sh — and not `zsh` or `fish`, whose
    /// spellings contain those letters without naming them.
    shebang: Regex,
    /// An inline suppression. The linter's own rule: a directive is a comment
    /// of its own, so anything mentioning one mid-line — prose, or a grep for
    /// it — is not a suppression and must not count as one.
    suppression: Regex,
}

impl Patterns {
    fn new() -> Result<Self> {
        Ok(Self {
            shebang: Regex::new(r"^#!.*\b(bash|sh)\b")?,
            suppression: Regex::new(r"(?m)^[ \t]*#[ \t]*shellcheck[ \t]+disable=")?,
        })
    }
}

/// The station.
pub struct ShellLint {
    id: StationId,
    budget: Duration,
}

impl Default for ShellLint {
    fn default() -> Self {
        Self {
            id: StationId::from_static("shell-lint"),
            budget: BUDGET,
        }
    }
}

impl Station for ShellLint {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "the bash we own is linted, formatted, and silenced only where recorded"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let repo = match Repo::discover(cx) {
            Ok(repo) => repo,
            Err(reason) => return Ok(Outcome::Ran(vec![reason.into_finding(&self.id)])),
        };
        let patterns = Patterns::new()?;
        let files = population(cx.home(), &repo.tracked()?, &patterns);
        if files.is_empty() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "no bash of ours is tracked in the clear",
            )));
        }

        let mut findings = lint(&self.id, cx, &files, self.budget)?;
        findings.extend(format(&self.id, cx, &files, self.budget)?);
        findings.extend(ratchet(&self.id, cx, &files, &patterns)?);
        Ok(Outcome::Ran(findings))
    }
}

// --- Which files are ours ---------------------------------------------------

/// How a file has to be told what dialect it is.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dialect {
    /// It carries a shebang, which names its own interpreter.
    Declared,
    /// It is sourced, so nothing in the file says what it is, and `SC2148`
    /// would fire on every one of them. Naming bash is the answer — but only
    /// for these, because forcing it on a `#!/bin/sh` script would lint that
    /// script as something it is not.
    Sourced,
}

impl Dialect {
    /// What `shellcheck` has to be told.
    fn shellcheck(self) -> &'static [&'static str] {
        match self {
            Self::Declared => &[],
            Self::Sourced => &["-s", "bash"],
        }
    }

    /// What `shfmt` has to be told. A different spelling of the same fact.
    fn shfmt(self) -> &'static [&'static str] {
        match self {
            Self::Declared => &[],
            Self::Sourced => &["-ln", "bash"],
        }
    }
}

/// One file in scope, and what it has to be told.
struct Owned {
    rel: Rel,
    dialect: Dialect,
}

/// The tracked files that are bash we own.
fn population(home: &Utf8Path, tracked: &[Rel], patterns: &Patterns) -> Vec<Owned> {
    let mut owned: Vec<Owned> = tracked
        .iter()
        .filter_map(|rel| {
            let dialect = classify(home, rel, patterns)?;
            Some(Owned {
                rel: rel.clone(),
                dialect,
            })
        })
        .collect();
    // Stable, so two runs over one tree issue the same command line and report
    // in the same order.
    owned.sort_by(|left, right| left.rel.cmp(&right.rel));
    owned
}

/// Whether a tracked path is ours to check, and what it has to be told.
fn classify(home: &Utf8Path, rel: &Rel, patterns: &Patterns) -> Option<Dialect> {
    let path = rel.absolute(home);
    // A symlink would be checked twice, once through each name.
    let meta = fs_err::symlink_metadata(path.as_std_path()).ok()?;
    if !meta.is_file() {
        return None;
    }
    if rel
        .path()
        .components()
        .any(|part| part.as_str() == FIXTURES)
    {
        return None;
    }
    let name = rel.path().file_name()?;
    if FOREIGN_NAMES.contains(&name) || FOREIGN_SUFFIXES.iter().any(|end| name.ends_with(end)) {
        return None;
    }

    let first = first_line(&path)?;
    let dialect = if first.starts_with("#!") {
        Dialect::Declared
    } else {
        Dialect::Sourced
    };
    if OURS_NAMES.contains(&name) || OURS_SUFFIXES.iter().any(|end| name.ends_with(end)) {
        return Some(dialect);
    }
    // Nothing about the name says bash, so the shebang has to.
    (dialect == Dialect::Declared && patterns.shebang.is_match(&first)).then_some(dialect)
}

/// A file's first line, or nothing when it is not text.
fn first_line(path: &Utf8Path) -> Option<String> {
    use std::io::BufRead as _;
    let file = fs_err::File::open(path.as_std_path()).ok()?;
    let mut line = String::new();
    std::io::BufReader::new(file).read_line(&mut line).ok()?;
    Some(line.trim_end().to_owned())
}

// --- The three gates --------------------------------------------------------

/// What `shellcheck -f json1` says.
#[derive(Deserialize)]
struct Comments {
    comments: Vec<Comment>,
}

/// One lint, with its position as a field rather than as text to be split.
#[derive(Deserialize)]
struct Comment {
    file: String,
    line: usize,
    column: usize,
    level: String,
    code: u32,
    message: String,
}

fn lint(id: &StationId, cx: &Context, files: &[Owned], budget: Duration) -> Result<Vec<Finding>> {
    let Some(shellcheck) = resolve(cx, "shellcheck") else {
        return Ok(vec![
            id.soft(Summary::lossy(&format!(
                "shellcheck is not on PATH, so {} file(s) went unlinted",
                files.len()
            )))
            .fixed_by(FixHint::lossy("brew install shellcheck")),
        ]);
    };

    let mut comments: Vec<Comment> = Vec::new();
    for dialect in [Dialect::Declared, Dialect::Sourced] {
        let batch = batch(files, dialect);
        if batch.is_empty() {
            continue;
        }
        let mut command = shellcheck.in_dir(cx.home());
        command
            .args(dialect.shellcheck())
            .args(["-S", SEVERITY, "-e", UNFOLLOWABLE, "-f", "json1"])
            .args(&batch);
        // Exiting non-zero is how shellcheck says it found something, so the
        // status is data and never a failure.
        let exit = shellcheck
            .run_within(&mut command, budget)
            .context("running shellcheck")?;
        match serde_json::from_str::<Comments>(&exit.stdout) {
            Ok(answer) => comments.extend(answer.comments),
            Err(error) => {
                return Ok(vec![
                    id.broken(Summary::lossy(&format!(
                        "shellcheck's answer could not be read, so nothing is linting bash: {error}"
                    )))
                    .detailed_with(Detail::new(exit.stderr.trim().to_owned()))
                    .fixed_by(FixHint::lossy(
                        "check `shellcheck --version` supports -f json1",
                    )),
                ]);
            }
        }
    }

    let mut by_file: BTreeMap<String, Vec<Comment>> = BTreeMap::new();
    for comment in comments {
        by_file
            .entry(comment.file.clone())
            .or_default()
            .push(comment);
    }
    Ok(by_file
        .into_iter()
        .map(|(file, mut found)| {
            found.sort_by_key(|c| (c.line, c.column, c.code));
            let detail = found
                .iter()
                .map(|c| {
                    format!(
                        "{}:{} {} SC{}: {}",
                        c.line, c.column, c.level, c.code, c.message
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            id.broken(Summary::lossy(&format!(
                "{file} has {} shellcheck finding(s)",
                found.len()
            )))
            .detailed_with(Detail::new(detail))
            .at(Rel::new(&file).location())
            .fixed_by(FixHint::lossy(
                "fix it, or accept it at the site with an inline `# shellcheck disable=` and its reason",
            ))
        })
        .collect())
}

fn format(id: &StationId, cx: &Context, files: &[Owned], budget: Duration) -> Result<Vec<Finding>> {
    let Some(shfmt) = resolve(cx, "shfmt") else {
        return Ok(vec![
            id.soft(Summary::lossy(&format!(
                "shfmt is not on PATH, so {} file(s) went unformatted",
                files.len()
            )))
            .fixed_by(FixHint::lossy("brew install shfmt")),
        ]);
    };

    let known: std::collections::BTreeSet<&str> =
        files.iter().map(|owned| owned.rel.as_str()).collect();
    let mut findings = Vec::new();
    for dialect in [Dialect::Declared, Dialect::Sourced] {
        let batch = batch(files, dialect);
        if batch.is_empty() {
            continue;
        }
        let mut command = shfmt.in_dir(cx.home());
        command
            .args(dialect.shfmt())
            .args(STYLE)
            .arg("-l")
            .args(&batch);
        let exit = shfmt
            .run_within(&mut command, budget)
            .context("running shfmt")?;

        for line in exit.stdout.lines().filter(|line| !line.is_empty()) {
            if known.contains(line) {
                findings.push(
                    id.broken(Summary::lossy(&format!("{line} is not in the house style")))
                        .at(Rel::new(line).location())
                        .fixed_by(FixHint::lossy(&format!(
                            "shfmt {} -w {line}",
                            STYLE.join(" ")
                        ))),
                );
            } else {
                // shfmt has no `-0`, so a path holding a newline would arrive
                // as two lines that name nothing. Saying so beats guessing.
                findings.push(id.note(Summary::lossy(
                    "shfmt named something that is not a file it was given",
                )));
            }
        }

        // The retired script sent this to /dev/null, so a file neither tool
        // could parse passed both gates in silence.
        for line in exit.stderr.lines().filter(|line| !line.trim().is_empty()) {
            findings.push(
                id.broken(Summary::lossy(&format!("shfmt could not parse: {line}")))
                    .fixed_by(FixHint::lossy("fix the syntax error")),
            );
        }
    }
    Ok(findings)
}

/// The committed count of inline suppressions, per file.
fn ratchet(
    id: &StationId,
    cx: &Context,
    files: &[Owned],
    patterns: &Patterns,
) -> Result<Vec<Finding>> {
    let baseline = cx.at(BASELINE);
    let Ok(text) = fs_err::read_to_string(baseline.as_std_path()) else {
        return Ok(vec![
            id.soft(Summary::lossy(&format!(
                "there is no suppression baseline at {BASELINE}, so silencing a lint is invisible"
            )))
            .fixed_by(FixHint::lossy("write one from the tree's current counts")),
        ]);
    };
    let want: BTreeMap<String, u64> = toml::from_str(&text)
        .with_context(|| format!("reading {BASELINE} — the suppression baseline is unusable"))?;

    let mut have: BTreeMap<String, u64> = BTreeMap::new();
    for owned in files {
        let count = suppressions(&owned.rel.absolute(cx.home()), patterns);
        if count > 0 {
            have.insert(owned.rel.as_str().to_owned(), count);
        }
    }

    let mut findings = Vec::new();
    for path in want
        .keys()
        .chain(have.keys())
        .collect::<std::collections::BTreeSet<_>>()
    {
        let (wanted, held) = (want.get(path).copied(), have.get(path).copied());
        if wanted == held {
            continue;
        }
        let summary = match (wanted, held) {
            (Some(wanted), Some(held)) => {
                format!("{path} carries {held} suppression(s) and the baseline records {wanted}")
            }
            (None, Some(held)) => {
                format!("{path} carries {held} suppression(s) and the baseline does not list it")
            }
            (Some(wanted), None) => format!(
                "the baseline records {wanted} suppression(s) for {path}, and the tree has none"
            ),
            // Neither side has it, which is not a difference.
            (None, None) => continue,
        };
        findings.push(
            id.broken(Summary::lossy(&summary))
                .at(Rel::new(path).location())
                .fixed_by(FixHint::lossy(&format!(
                    "fix the lint, or edit {BASELINE} in the same commit"
                ))),
        );
    }
    Ok(findings)
}

/// Inline suppressions in one file.
fn suppressions(path: &Utf8Path, patterns: &Patterns) -> u64 {
    let Ok(text) = fs_err::read_to_string(path.as_std_path()) else {
        return 0;
    };
    patterns
        .suppression
        .find_iter(&text)
        .count()
        .try_into()
        .unwrap_or(u64::MAX)
}

// --- Plumbing ---------------------------------------------------------------

/// The paths to hand one invocation, relative to the home it runs in — so the
/// tools answer in the same spelling every lane uses, with no absolute path in
/// the output.
fn batch(files: &[Owned], dialect: Dialect) -> Vec<&str> {
    files
        .iter()
        .filter(|owned| owned.dialect == dialect)
        .map(|owned| owned.rel.as_str())
        .collect()
}

/// A program on the injected search path.
fn resolve(cx: &Context, name: &str) -> Option<Tool> {
    crate::probe::resolve(name, cx.path())
        .map(|program| Tool::at_path(name, program.into_std_path_buf()))
}

#[cfg(test)]
mod tests {
    use relic_core::finding::Severity;

    use super::*;
    use crate::repo::testing::Machine;

    /// Shims for the two tools, so a test pins this station's logic rather than
    /// the version of `shellcheck` the machine happens to carry. Their contract
    /// — the `json1` shape, the `-l` list, the stream a parse error goes to — is
    /// pinned separately, against output captured from the real ones.
    impl Machine {
        /// A `shellcheck` that reports `message` against `about`, and exits
        /// non-zero the way the real one does when it has something to say.
        /// Nothing may be discarded for that: a status is an answer here.
        fn shellcheck_finds(&self, about: &str, code: u32, message: &str) {
            self.executable(
                "shellcheck",
                &format!(
                    "#!/bin/sh\nfor a in \"$@\"; do\n  if [ \"$a\" = '{about}' ]; then\n    printf '%s' '{{\"comments\":[{{\"file\":\"{about}\",\"line\":3,\"endLine\":3,\"column\":5,\"endColumn\":9,\"level\":\"warning\",\"code\":{code},\"message\":\"{message}\",\"fix\":null}}]}}'\n    exit 1\n  fi\ndone\nprintf '%s' '{{\"comments\":[]}}'\n"
                ),
            );
        }

        fn shellcheck_clean(&self) {
            self.executable("shellcheck", "#!/bin/sh\nprintf '%s' '{\"comments\":[]}'\n");
        }

        fn shellcheck_babbles(&self) {
            self.executable("shellcheck", "#!/bin/sh\necho 'not json at all'\nexit 4\n");
        }

        /// An `shfmt` that lists `about` as unformatted when it is asked about
        /// it, the way `-l` does.
        fn shfmt_lists(&self, about: &str) {
            self.executable(
                "shfmt",
                &format!(
                    "#!/bin/sh\nfor a in \"$@\"; do [ \"$a\" = '{about}' ] && echo '{about}'; done\nexit 0\n"
                ),
            );
        }

        fn shfmt_clean(&self) {
            self.executable("shfmt", "#!/bin/sh\nexit 0\n");
        }

        /// An `shfmt` that cannot parse a file. The real one says so on stderr
        /// and keeps going, which is why the retired script's `2>/dev/null`
        /// let an unparseable file pass every gate.
        fn shfmt_chokes(&self, about: &str) {
            self.executable(
                "shfmt",
                &format!(
                    "#!/bin/sh\nfor a in \"$@\"; do [ \"$a\" = '{about}' ] && echo '{about}:1:8: `then` must be followed by a statement list' >&2; done\nexit 1\n"
                ),
            );
        }

        fn baseline(&self, body: &str) {
            self.write(BASELINE, body);
        }

        /// A tracked file, which is the only kind this station looks at.
        fn shell(&self, rel: &str, body: &str) -> &Self {
            self.write(rel, body);
            self.track(rel);
            self
        }
    }

    fn station() -> ShellLint {
        ShellLint::default()
    }

    fn findings(machine: &Machine) -> Vec<Finding> {
        machine.findings_of(&station())
    }

    fn about(machine: &Machine, needle: &str) -> Vec<Finding> {
        machine.about_of(&station(), needle)
    }

    /// A machine with both tools present and quiet, and a baseline that records
    /// nothing — so any finding a test sees is the one it arranged.
    fn quiet() -> Machine {
        let machine = Machine::new();
        machine.shellcheck_clean();
        machine.shfmt_clean();
        machine.baseline("# nothing suppressed\n");
        machine.shell(".config/bin/thing.sh", "#!/usr/bin/env bash\necho ok\n");
        machine
    }

    fn patterns() -> Patterns {
        Patterns::new().expect("the patterns are literals")
    }

    // --- Which files are ours ----------------------------------------------

    #[test]
    fn the_patterns_are_literals_that_compile() {
        assert!(Patterns::new().is_ok());
    }

    #[test]
    fn a_shebang_naming_bash_or_sh_is_ours_and_one_naming_anything_else_is_not() {
        let patterns = patterns();
        for yes in [
            "#!/bin/bash",
            "#!/usr/bin/env bash",
            "#!/bin/sh",
            "#!/usr/bin/env sh",
            "#!/bin/bash -eu",
        ] {
            assert!(patterns.shebang.is_match(yes), "{yes} is bash we own");
        }
        for no in [
            "#!/bin/zsh",
            "#!/usr/bin/env fish",
            "#!/usr/bin/env python3",
            "#!/usr/bin/env node",
            "# not a shebang at all",
        ] {
            assert!(
                !patterns.shebang.is_match(no),
                "{no} is not ours — zsh and fish spell those letters without naming them"
            );
        }
    }

    #[test]
    fn a_name_that_says_bash_needs_no_shebang_and_is_told_it_is_bash() {
        let machine = Machine::new();
        machine.write(".config/shell/env.d/040-env.sh", "export X=1\n");
        let rel = Rel::new(".config/shell/env.d/040-env.sh");
        assert_eq!(
            classify(&machine.home, &rel, &patterns()),
            Some(Dialect::Sourced),
            "a sourced file names no interpreter, so both tools have to be told"
        );
    }

    #[test]
    fn a_shebang_is_left_to_speak_for_itself() {
        let machine = Machine::new();
        machine.write(".config/bin/up", "#!/usr/bin/env bash\necho ok\n");
        assert_eq!(
            classify(&machine.home, &Rel::new(".config/bin/up"), &patterns()),
            Some(Dialect::Declared),
        );
        assert!(Dialect::Declared.shellcheck().is_empty());
        assert!(Dialect::Declared.shfmt().is_empty());
        assert_eq!(Dialect::Sourced.shellcheck(), &["-s", "bash"]);
        assert_eq!(Dialect::Sourced.shfmt(), &["-ln", "bash"]);
    }

    #[test]
    fn shell_that_is_not_bash_is_out_of_scope_because_neither_tool_can_read_it() {
        let machine = Machine::new();
        for rel in [
            ".config/shell/env.d/040-env.fish",
            ".config/shell/interactive.d/050-prompt.zsh",
            ".config/zsh/.zshrc",
            ".zshenv",
            ".config/zsh/.zprofile",
        ] {
            machine.write(rel, "# whatever\n");
            assert_eq!(
                classify(&machine.home, &Rel::new(rel), &patterns()),
                None,
                "{rel} is shell, but not bash"
            );
        }
    }

    #[test]
    fn a_fixture_is_test_data_and_a_lint_written_into_one_is_its_point() {
        let machine = Machine::new();
        machine.write(
            ".config/relics/relic/tests/fixtures/broken.sh",
            "#!/bin/bash\necho $unquoted\n",
        );
        assert_eq!(
            classify(
                &machine.home,
                &Rel::new(".config/relics/relic/tests/fixtures/broken.sh"),
                &patterns()
            ),
            None
        );
    }

    #[test]
    fn a_symlink_is_skipped_so_its_target_is_not_checked_twice() {
        let machine = Machine::new();
        machine.write(".config/bin/yadm-wrapper", "#!/usr/bin/env bash\necho ok\n");
        std::os::unix::fs::symlink(
            machine.home.join(".config/bin/yadm-wrapper"),
            machine.home.join(".config/bin/yadm-link"),
        )
        .expect("a symlink");
        assert_eq!(
            classify(
                &machine.home,
                &Rel::new(".config/bin/yadm-link"),
                &patterns()
            ),
            None
        );
    }

    #[test]
    fn the_population_is_what_is_tracked_and_never_what_is_merely_present() {
        let machine = quiet();
        machine.write(".config/bin/untracked.sh", "#!/bin/bash\necho ok\n");
        let tracked = Repo::discover(&machine.context())
            .expect("a repository")
            .tracked()
            .expect("a tracked set");
        let population = population(&machine.home, &tracked, &patterns());
        let named: Vec<&str> = population.iter().map(|owned| owned.rel.as_str()).collect();
        assert_eq!(named, vec![".config/bin/thing.sh"]);
    }

    #[test]
    fn a_machine_with_no_bash_of_ours_is_skipped_rather_than_passed() {
        let machine = Machine::new();
        machine.shellcheck_clean();
        machine.shfmt_clean();
        let Outcome::Skipped(reason) = machine.outcome_of(&station()) else {
            panic!("nothing is in scope, so there is nothing to report");
        };
        assert!(reason.as_str().contains("no bash of ours"));
    }

    // --- Reaching the repository -------------------------------------------

    #[test]
    fn no_yadm_on_the_path_is_broken_and_never_a_silent_pass() {
        let machine = Machine::new();
        fs_err::remove_file(machine.bin.join("yadm")).expect("removed");
        let findings = findings(&machine);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Broken);
    }

    // --- Lint ---------------------------------------------------------------

    #[test]
    fn a_clean_tree_says_nothing() {
        assert!(findings(&quiet()).is_empty());
    }

    #[test]
    fn a_lint_is_broken_and_carries_its_code_and_position() {
        let machine = quiet();
        machine.shellcheck_finds(
            ".config/bin/thing.sh",
            2086,
            "Double quote to prevent globbing",
        );
        let found = about(&machine, "shellcheck finding");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Broken);
        assert!(found[0].summary.as_str().contains(".config/bin/thing.sh"));
        let detail = found[0].detail.as_ref().expect("the lints themselves");
        assert!(detail.as_str().contains("SC2086"), "{detail}");
        assert!(detail.as_str().contains("3:5"), "{detail}");
    }

    #[test]
    fn a_linter_that_exits_non_zero_is_answering_rather_than_failing() {
        // shellcheck exits 1 *because* it found something. A caller reading
        // that as a failure would discard every report that had anything in it.
        let machine = quiet();
        machine.shellcheck_finds(".config/bin/thing.sh", 2154, "referenced but not assigned");
        assert_eq!(about(&machine, "shellcheck finding").len(), 1);
    }

    #[test]
    fn an_unreadable_answer_is_broken_rather_than_a_clean_machine() {
        let machine = quiet();
        machine.shellcheck_babbles();
        let found = about(&machine, "could not be read");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Broken);
    }

    #[test]
    fn a_missing_linter_is_soft_and_says_how_much_went_unchecked() {
        let machine = quiet();
        fs_err::remove_file(machine.bin.join("shellcheck")).expect("removed");
        let found = about(&machine, "shellcheck is not on PATH");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Soft);
        assert!(found[0].summary.as_str().contains("1 file(s)"));
    }

    // --- Format -------------------------------------------------------------

    #[test]
    fn an_unformatted_file_is_broken_and_names_the_command_that_fixes_it() {
        let machine = quiet();
        machine.shfmt_lists(".config/bin/thing.sh");
        let found = about(&machine, "house style");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Broken);
        let fix = found[0].fix.as_ref().expect("a fix");
        assert!(fix.as_str().contains("-i 4 -ci -w"), "{fix}");
    }

    #[test]
    fn a_file_the_formatter_cannot_parse_is_reported_rather_than_silenced() {
        let machine = quiet();
        machine.shfmt_chokes(".config/bin/thing.sh");
        let found = about(&machine, "could not parse");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Broken);
    }

    #[test]
    fn a_formatter_naming_something_it_was_not_given_says_so() {
        let machine = quiet();
        machine.executable("shfmt", "#!/bin/sh\necho 'a line that is not a file'\n");
        let found = about(&machine, "not a file it was given");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Note);
    }

    #[test]
    fn a_missing_formatter_is_soft() {
        let machine = quiet();
        fs_err::remove_file(machine.bin.join("shfmt")).expect("removed");
        let found = about(&machine, "shfmt is not on PATH");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Soft);
    }

    // --- The ratchet --------------------------------------------------------

    #[test]
    fn a_count_that_matches_the_baseline_says_nothing() {
        let machine = quiet();
        machine.shell(
            ".config/bin/quiet.sh",
            "#!/bin/bash\n# shellcheck disable=SC2086\necho ok\n",
        );
        machine.baseline("\".config/bin/quiet.sh\" = 1\n");
        assert!(findings(&machine).is_empty());
    }

    #[test]
    fn a_suppression_the_baseline_does_not_record_is_broken() {
        let machine = quiet();
        machine.shell(
            ".config/bin/sneaky.sh",
            "#!/bin/bash\n# shellcheck disable=SC2086\necho ok\n",
        );
        let found = about(&machine, "does not list it");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Broken);
    }

    #[test]
    fn removing_a_suppression_without_lowering_the_number_is_broken_too() {
        // The equality is what makes the control work in both directions: an
        // inequality lets slack accumulate, and slack is suppressions that can
        // be added back unseen.
        let machine = quiet();
        machine.baseline("\".config/bin/thing.sh\" = 2\n");
        let found = about(&machine, "the tree has none");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Broken);
    }

    #[test]
    fn a_changed_count_reports_both_numbers() {
        let machine = quiet();
        machine.shell(
            ".config/bin/two.sh",
            "#!/bin/bash\n# shellcheck disable=SC2086\n# shellcheck disable=SC2154\necho ok\n",
        );
        machine.baseline("\".config/bin/two.sh\" = 1\n");
        let found = about(&machine, "the baseline records");
        assert_eq!(found.len(), 1);
        assert!(
            found[0].summary.as_str().contains("carries 2"),
            "{}",
            found[0].summary
        );
        assert!(
            found[0].summary.as_str().contains("records 1"),
            "{}",
            found[0].summary
        );
    }

    #[test]
    fn a_directive_mentioned_mid_line_is_not_a_suppression() {
        // Prose about the mechanism, and the grep that counts it, both name the
        // directive without being one. The linter's own rule is that a
        // directive is a comment of its own.
        let machine = quiet();
        machine.shell(
            ".config/bin/prose.sh",
            "#!/bin/bash\necho 'add a # shellcheck disable= line'  # not one\ngrep -c '# shellcheck disable=' x\n",
        );
        assert!(findings(&machine).is_empty());
    }

    #[test]
    fn a_missing_baseline_is_soft_because_silencing_a_lint_is_then_invisible() {
        let machine = quiet();
        fs_err::remove_file(machine.home.join(BASELINE)).expect("removed");
        let found = about(&machine, "no suppression baseline");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].severity, Severity::Soft);
    }

    #[test]
    fn an_unusable_baseline_stops_the_station_rather_than_grading_a_clean_machine() {
        let machine = quiet();
        machine.baseline("this is not toml at all [[[\n");
        let Err(error) = ShellLint::default().check(&machine.context()) else {
            panic!("an unreadable baseline is not a machine that passed");
        };
        assert!(error.to_string().contains("suppression baseline"));
    }

    // --- The tools' own contract -------------------------------------------

    #[test]
    fn the_json1_shape_is_the_one_shellcheck_emits() {
        // Captured verbatim from `shellcheck -f json1`. Reading fields beats
        // splitting a `-f gcc` line, which is what the retired script did: the
        // second of these carries a nested `fix` object a line reader never
        // sees, and a message holding the tool's own separators.
        let captured = r#"{"comments":[{"file":"t.sh","line":2,"endLine":2,"column":1,"endColumn":4,"level":"warning","code":2034,"message":"foo appears unused. Verify use (or export if used externally).","fix":null},{"file":"m.sh","line":2,"endLine":2,"column":1,"endColumn":8,"level":"warning","code":2164,"message":"Use 'cd ... || exit' or 'cd ... || return' in case cd fails.","fix":{"replacements":[{"column":8,"endColumn":8,"endLine":2,"insertionPoint":"beforeStart","line":2,"precedence":5,"replacement":" || exit"}]}}]}"#;
        let answer: Comments = serde_json::from_str(captured).expect("the json1 shape");
        assert_eq!(answer.comments.len(), 2);
        assert_eq!(answer.comments[0].code, 2034);
        assert_eq!(answer.comments[0].line, 2);
        assert_eq!(answer.comments[0].level, "warning");
        assert_eq!(answer.comments[1].file, "m.sh");
        assert!(answer.comments[1].message.contains("||"));

        let clean: Comments = serde_json::from_str(r#"{"comments":[]}"#).expect("an empty answer");
        assert!(clean.comments.is_empty());
    }

    #[test]
    fn the_house_style_is_the_one_the_baseline_was_measured_under() {
        assert_eq!(STYLE, &["-i", "4", "-ci"]);
        assert_eq!(SEVERITY, "warning");
        assert_eq!(UNFOLLOWABLE, "SC1090,SC1091");
    }
}
