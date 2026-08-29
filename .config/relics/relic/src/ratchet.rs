//! The two committed baselines every compiled relic passes.
//!
//! **A ratchet is a deterministic control, not a directive.** A committed
//! baseline file, plus a check comparing today's measurement against it that
//! exits non-zero when it is worse. No judgement, no model in the loop — a
//! program comparing two numbers.
//!
//! The governing property is not "the number may never worsen". It is: *you can
//! always move the number, you just cannot move it silently.* A legitimate
//! regression is an edit to the baseline in the same commit, which puts it in a
//! diff someone reads.
//!
//! Both of these are **equalities, not ceilings**. A count that falls fails too,
//! because an inequality lets slack accumulate and slack is suppressions that
//! can be added back unseen.

use std::collections::BTreeMap;

use camino::Utf8Path;

/// A `package = number` baseline file.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Baseline {
    values: BTreeMap<String, u64>,
}

impl Baseline {
    /// Read one, or nothing when there is none.
    ///
    /// An absent baseline is `None` rather than an empty one: a ratchet with
    /// nothing in it and a ratchet that is not there read identically at the
    /// call site, and only one of them should gate.
    #[must_use]
    pub fn load(path: &Utf8Path) -> Option<Self> {
        fs_err::read_to_string(path.as_std_path())
            .ok()
            .map(|body| Self::parse(&body))
    }

    /// Parse its text: `#` comments and blanks are not entries.
    #[must_use]
    pub fn parse(body: &str) -> Self {
        let values = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| {
                let (key, value) = line.split_once('=')?;
                let key = key.trim().trim_matches('"').to_owned();
                let number: String = value.chars().filter(char::is_ascii_digit).collect();
                Some((key, number.parse().ok()?))
            })
            .collect();
        Self { values }
    }

    /// What a package's baseline says.
    #[must_use]
    pub fn get(&self, package: &str) -> Option<u64> {
        self.values.get(package).copied()
    }

    /// Every package it names, in name order.
    pub fn packages(&self) -> impl Iterator<Item = (&str, u64)> {
        self.values.iter().map(|(k, v)| (k.as_str(), *v))
    }
}

/// How one package's suppression count compares to its baseline.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// It is exactly the baseline.
    Holds,
    /// There is no baseline for it at all.
    Unbaselined,
    /// It rose: a suppression was added without accounting for it.
    Rose {
        /// What it is now.
        have: u64,
        /// What the baseline says.
        want: u64,
    },
    /// It fell: a suppression went away and the baseline was not lowered.
    ///
    /// A failure, deliberately. Slack in a ratchet is suppressions that can be
    /// added back unseen.
    Fell {
        /// What it is now.
        have: u64,
        /// What the baseline says.
        want: u64,
    },
}

impl Verdict {
    /// Compare a measurement to a baseline.
    #[must_use]
    pub fn of(have: u64, want: Option<u64>) -> Self {
        match want {
            None => Self::Unbaselined,
            Some(want) if have == want => Self::Holds,
            Some(want) if have > want => Self::Rose { have, want },
            Some(want) => Self::Fell { have, want },
        }
    }

    /// Whether it fails the gate.
    #[must_use]
    pub fn fails(&self) -> bool {
        match self {
            Self::Holds => false,
            Self::Unbaselined | Self::Rose { .. } | Self::Fell { .. } => true,
        }
    }

    /// What to say about it.
    #[must_use]
    pub fn report(&self, package: &str, baseline: &Utf8Path) -> Option<String> {
        match self {
            Self::Holds => None,
            Self::Unbaselined => Some(format!(
                "lint ratchet: {package} has no baseline in {baseline}"
            )),
            Self::Rose { have, want } => Some(format!(
                "lint ratchet: {package} has {have} suppressions, baseline {want}\n  \
                 fix the lint, or raise the baseline in the same commit as the suppression"
            )),
            Self::Fell { have, want } => Some(format!(
                "lint ratchet: {package} is down to {have} suppressions (baseline {want}) \
                 — lower it in {baseline}"
            )),
        }
    }
}

/// Count the suppression attributes in one package's Rust sources.
///
/// `#[allow]`, `#![allow]` and `#[expect]` alike. `#[expect]` is the preferable
/// form — it fails once the lint it silences stops firing — but a count that
/// could not see it would be a count an agent could walk around.
///
/// A `fixtures/` directory is excluded: a fixture is test data, and lints
/// written into one are its point.
#[must_use]
pub fn suppressions(package_dir: &Utf8Path) -> u64 {
    let mut total = 0;
    let walker = ignore::WalkBuilder::new(package_dir.as_std_path())
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .filter_entry(|entry| {
            let name = entry.file_name().to_string_lossy();
            name != "target" && name != "fixtures"
        })
        .build();
    for entry in walker.flatten() {
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        if entry.path().extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let Ok(body) = fs_err::read_to_string(entry.path()) else {
            continue;
        };
        total += count_in(&body);
    }
    total
}

/// How many suppression attributes one file's text opens.
///
/// The grammar rather than a list of four concatenations: `#`, an optional `!`,
/// `[`, and a lint-silencing keyword. Spelled this way for a reason particular
/// to this crate — the four literals it would otherwise carry are four openers
/// in its **own** sources, and this is the file that counts them, so its
/// baseline would be four suppressions it does not have.
///
/// A `#` inside a comment or a string still opens one, and that is honest: a
/// count that reasoned about either would be a parser, and a parser is
/// something an agent can find the edge of.
#[must_use]
pub fn count_in(body: &str) -> u64 {
    /// The attributes that silence a lint. `expect` is the preferable form —
    /// it fails once the lint it silences stops firing — and a count that
    /// could not see it would be a count anyone could walk around.
    const SILENCERS: [&str; 2] = ["allow", "expect"];

    let mut count = 0;
    for (index, _) in body.match_indices('#') {
        let rest = body.get(index + 1..).unwrap_or_default();
        let rest = rest.strip_prefix('!').unwrap_or(rest);
        let Some(rest) = rest.strip_prefix('[') else {
            continue;
        };
        if SILENCERS
            .iter()
            .any(|word| rest.strip_prefix(word).is_some_and(|t| t.starts_with('(')))
        {
            count += 1;
        }
    }
    count
}

#[cfg(test)]
mod tests {
    use super::{Baseline, Verdict, count_in};
    use camino::Utf8Path;

    /// Fixture text, read from `tests/fixtures/` rather than written inline.
    ///
    /// This is the one crate whose *subject* is suppression attributes, so
    /// spelling them in its own source would put eighteen of them in its own
    /// baseline and stop that number meaning anything. The `fixtures/`
    /// exclusion exists for exactly this.
    const SUPPRESSIONS: &str = include_str!("../tests/fixtures/suppressions.rs");
    const ONE_ALLOW: &str = include_str!("../tests/fixtures/one-allow.rs");
    const ONE_EXPECT: &str = include_str!("../tests/fixtures/one-expect.rs");
    const THREE_ALLOWS: &str = include_str!("../tests/fixtures/three-allows.rs");

    #[test]
    fn a_baseline_ignores_comments_and_quotes() {
        let baseline = Baseline::parse("# a note\n\ndocket = 5\n\"midden\" = 4\nernest=17\n");
        assert_eq!(baseline.get("docket"), Some(5));
        assert_eq!(baseline.get("midden"), Some(4));
        assert_eq!(baseline.get("ernest"), Some(17));
        assert_eq!(baseline.get("nothing"), None);
    }

    #[test]
    fn an_absent_baseline_is_not_an_empty_one() {
        assert!(Baseline::load(Utf8Path::new("/nowhere/at/all")).is_none());
    }

    #[test]
    fn the_ratchet_is_an_equality_and_fails_in_both_directions() {
        assert_eq!(Verdict::of(5, Some(5)), Verdict::Holds);
        assert!(!Verdict::of(5, Some(5)).fails());
        assert!(Verdict::of(6, Some(5)).fails());
        assert!(
            Verdict::of(4, Some(5)).fails(),
            "slack is suppressions that can be added back unseen"
        );
        assert!(Verdict::of(0, None).fails());
    }

    #[test]
    fn every_failing_verdict_says_what_to_do() {
        let path = Utf8Path::new("/r/allows.toml");
        assert!(Verdict::Holds.report("x", path).is_none());
        for verdict in [
            Verdict::Unbaselined,
            Verdict::Rose { have: 2, want: 1 },
            Verdict::Fell { have: 0, want: 1 },
        ] {
            let said = verdict.report("x", path).unwrap_or_default();
            assert!(said.contains('x'), "{said}");
        }
    }

    #[test]
    fn every_suppression_form_is_counted_including_the_preferable_one() {
        assert_eq!(count_in(SUPPRESSIONS), 5);
    }

    #[test]
    fn a_file_with_no_suppressions_counts_none() {
        assert_eq!(count_in("fn main() {}\n#[derive(Clone)]\nstruct X;\n"), 0);
    }

    #[test]
    fn the_walk_counts_the_package_and_skips_what_is_not_its_code() {
        let guard = tempfile::tempdir().expect("a scratch dir");
        let root = camino::Utf8PathBuf::from_path_buf(guard.path().to_path_buf())
            .expect("utf8 scratch path");
        let write = |rest: &str, body: &str| {
            let path = root.join(rest);
            if let Some(parent) = path.parent() {
                fs_err::create_dir_all(parent.as_std_path()).expect("a dir");
            }
            fs_err::write(path.as_std_path(), body).expect("a file");
        };
        write("src/lib.rs", ONE_ALLOW);
        write("src/deep/mod.rs", ONE_EXPECT);
        // Not Rust, so not counted.
        write("src/notes.md", THREE_ALLOWS);
        // A build tree is not the package's code.
        write("target/debug/build.rs", THREE_ALLOWS);
        // A fixture is test data, and lints written into one are its point.
        write("tests/fixtures/bad.rs", THREE_ALLOWS);

        assert_eq!(super::suppressions(&root), 2);
    }
}
