//! Finding the files worth reading.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};

use crate::analyze::profiles::Profile;
use crate::detect::profile_for;

/// Directory names never worth walking into. They hold dependencies and build
/// output — text nobody wrote and nobody loads into context on purpose. They
/// are excluded at every scope, so widening the scope cannot drag them in.
const DEFAULT_EXCLUDES: &[&str] = &[
    ".git",
    ".hg",
    ".svn",
    "node_modules",
    "vendor",
    "target",
    "__pycache__",
    ".venv",
    "venv",
];

/// How far the walk reaches. The levels are nested, and the line between them
/// is the one git already draws: `.gitignore` is committed and holds the noise,
/// `.git/info/exclude` is local and holds what you keep but do not share.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// What a fresh clone would see.
    Shared,
    /// Plus files excluded only on this machine.
    Local,
    /// Plus gitignored files.
    All,
}

/// Which side of the share a file sits on. Where both rule sets match a path,
/// `.gitignore` wins and the file never reaches the walk at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Provenance {
    Tracked,
    Local,
}

impl Provenance {
    pub fn label(self) -> &'static str {
        match self {
            Provenance::Tracked => "tracked",
            Provenance::Local => "local",
        }
    }
}

pub struct Candidate {
    pub path: PathBuf,
    pub profile: &'static Profile,
    pub provenance: Provenance,
}

/// Every supported file under `roots`, plus a tally of the files skipped for
/// having no profile — so coverage is visible rather than assumed.
pub fn collect(roots: &[PathBuf], scope: Scope, lang: Option<&str>) -> (Vec<Candidate>, u64) {
    let (found, skipped) = files(roots, scope, lang);
    let excludes = LocalExcludes::for_roots(roots);

    // At the widest scope a file may be hidden by `.gitignore` instead, which
    // no single matcher answers; the narrower walk is what names those.
    let reference: Option<HashSet<PathBuf>> = (scope == Scope::All).then(|| {
        files(roots, Scope::Local, lang)
            .0
            .into_iter()
            .map(|(path, _)| path)
            .collect()
    });

    let mut candidates: Vec<Candidate> = found
        .into_iter()
        .map(|(path, profile)| {
            let local = excludes.matches(&path)
                || reference.as_ref().is_some_and(|seen| !seen.contains(&path));
            Candidate {
                path,
                profile,
                provenance: if local {
                    Provenance::Local
                } else {
                    Provenance::Tracked
                },
            }
        })
        // A named root is exempt from the walk's own ignore rules, so the scope
        // has to be re-asserted here or `ernest .claude --scope shared` would
        // report the very files a clone cannot see.
        .filter(|c| scope != Scope::Shared || c.provenance == Provenance::Tracked)
        .collect();

    candidates.sort_by(|a, b| a.path.cmp(&b.path));
    (candidates, skipped)
}

/// `.git/info/exclude` for the repositories the roots sit in.
///
/// The walk cannot answer provenance on its own: `ignore` exempts a walk root
/// from its own rules, so naming a locally-excluded path would otherwise read
/// as shared. This asks the rule set directly, whatever the walk started from.
struct LocalExcludes(Vec<(PathBuf, Gitignore)>);

impl LocalExcludes {
    fn for_roots(roots: &[PathBuf]) -> Self {
        let mut repos: Vec<PathBuf> = roots.iter().filter_map(|root| repo_of(root)).collect();
        repos.sort();
        repos.dedup();
        Self(
            repos
                .into_iter()
                .filter_map(|repo| matcher(&repo).map(|m| (repo, m)))
                .collect(),
        )
    }

    fn matches(&self, path: &Path) -> bool {
        let Ok(path) = std::path::absolute(path) else {
            return false;
        };
        self.0.iter().any(|(repo, matcher)| {
            // The matcher panics on a path outside its root, so the prefix test
            // is a guard rather than an optimisation.
            path.starts_with(repo)
                && matcher
                    .matched_path_or_any_parents(&path, false)
                    .is_ignore()
        })
    }
}

/// The nearest ancestor holding a `.git`, resolved lexically so the prefix
/// matches what `std::path::absolute` produces for a candidate.
fn repo_of(root: &Path) -> Option<PathBuf> {
    let mut at = std::path::absolute(root).ok()?;
    loop {
        if at.join(".git").exists() {
            return Some(at);
        }
        if !at.pop() {
            return None;
        }
    }
}

fn matcher(repo: &Path) -> Option<Gitignore> {
    let exclude = repo.join(".git/info/exclude");
    if !exclude.is_file() {
        return None;
    }
    let mut builder = GitignoreBuilder::new(repo);
    builder.add(exclude);
    builder.build().ok()
}

fn files(
    roots: &[PathBuf],
    scope: Scope,
    lang: Option<&str>,
) -> (Vec<(PathBuf, &'static Profile)>, u64) {
    let respect = scope != Scope::All;
    let mut builder = WalkBuilder::new(&roots[0]);
    for root in &roots[1..] {
        builder.add(root);
    }
    builder
        // Dotfiles are the point on this machine, so hidden entries are walked;
        // the exclude list below is what still keeps `.git` out.
        .hidden(false)
        .parents(respect)
        .git_ignore(respect)
        .git_global(respect)
        .git_exclude(scope == Scope::Shared)
        .ignore(respect)
        .filter_entry(|entry| {
            entry
                .file_name()
                .to_str()
                .is_none_or(|name| !DEFAULT_EXCLUDES.contains(&name))
        });

    let mut found = Vec::new();
    let mut skipped = 0u64;

    for entry in builder.build() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.into_path();
        match profile_for(&path) {
            Some(profile) if lang.is_none_or(|l| l == profile.language) => {
                found.push((path, profile))
            }
            Some(_) => {}
            None => skipped += 1,
        }
    }

    (found, skipped)
}

/// A root the user named explicitly is read whether or not it is a directory,
/// so `ernest one-file.php` behaves.
pub fn is_readable_root(path: &Path) -> bool {
    path.exists()
}
