//! The test itself: what a file's contents say about whether it may be
//! committed. Knows nothing about where the definition came from, or about git.

use camino::Utf8PathBuf;

use crate::definition::Definition;

/// One reason a file may not be committed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Finding {
    /// A term from the definition appears in the file.
    Term {
        /// The file.
        path: Utf8PathBuf,
        /// The term, as the definition spells it.
        term: String,
    },
    /// The character class matched, with the first line it matched on — the
    /// line is the useful part, because the class says nothing about where.
    Characters {
        /// The file.
        path: Utf8PathBuf,
        /// The first line that matched, trimmed.
        line: String,
    },
    /// The file is not text, so no test can speak for it. Refused rather than
    /// skipped: the whole point is that unreviewable content does not pass.
    Unreadable {
        /// The file.
        path: Utf8PathBuf,
        /// Why it could not be tested.
        reason: String,
    },
}

impl std::fmt::Display for Finding {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Term { path, term } => write!(f, "{path}: carries the term {term:?}"),
            Self::Characters { path, line } => write!(f, "{path}: {line}"),
            Self::Unreadable { path, reason } => write!(f, "{path}: {reason}"),
        }
    }
}

/// What a whole run concluded.
#[derive(Debug, Default)]
pub struct Verdict {
    /// Every reason to refuse, in the order the files were given.
    pub findings: Vec<Finding>,
    /// How many files were actually tested, for a run that wants to say so.
    pub examined: usize,
}

impl Verdict {
    /// Whether the commit may proceed.
    #[must_use]
    pub fn clean(&self) -> bool {
        self.findings.is_empty()
    }
}

/// How much of a file is worth reading. Past this, a text file is generated or
/// vendored, and the guard is not a content scanner for those.
const MAX_BYTES: usize = 4 * 1024 * 1024;

/// The findings in one file's bytes.
///
/// Every term is reported, not just the first: a commit that carries three of
/// them should say three, so the author fixes the file rather than the line.
#[must_use]
pub fn file(path: &camino::Utf8Path, bytes: &[u8], definition: &Definition) -> Vec<Finding> {
    if bytes.is_empty() {
        return Vec::new();
    }
    if bytes.len() > MAX_BYTES {
        return vec![Finding::Unreadable {
            path: path.to_owned(),
            reason: format!("{} bytes is past what the guard reads", bytes.len()),
        }];
    }
    let Ok(text) = std::str::from_utf8(bytes) else {
        return vec![Finding::Unreadable {
            path: path.to_owned(),
            reason: "not text, so nothing can vouch for what is in it".to_owned(),
        }];
    };

    // The pre-filter earns its keep on the common case: a clean file is one
    // pass rather than one per term.
    if !definition.matches(text) {
        return Vec::new();
    }

    let mut findings = Vec::new();
    if let Some(class) = definition.class()
        && let Some(line) = text.lines().find(|line| class.is_match(line))
    {
        findings.push(Finding::Characters {
            path: path.to_owned(),
            line: line.trim().to_owned(),
        });
    }
    for (term, pattern) in definition.keywords() {
        if pattern.is_match(text) {
            findings.push(Finding::Term {
                path: path.to_owned(),
                term: term.clone(),
            });
        }
    }
    findings
}
