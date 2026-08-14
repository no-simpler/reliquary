//! Finding the files worth reading.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use ignore::WalkBuilder;
use ignore::gitignore::{Gitignore, GitignoreBuilder};
use serde::{Deserialize, Serialize};

use crate::analyze::profiles::Profile;
use crate::detect::profile_for;

/// Declares prose that is the product rather than prose about the code — a
/// fiction archive, a wiki, a test corpus. gitignore syntax, honored at every
/// scope, because it answers a question git does not: not "is this shared" but
/// "is this what ernest is for". The narrow case; most projects never write one.
pub const ERNESTIGNORE: &str = ".ernestignore";

/// The version control system's own store, which is not part of the work tree
/// at any scope. This is the one exclusion `.gitignore` cannot express: git
/// excludes `.git` structurally rather than by rule, so `git check-ignore .git`
/// says nothing and no committed pattern can stand in for this list. Nothing
/// else filters it either, because the walk sets `hidden(false)` deliberately —
/// dotfiles are the subject here.
///
/// Dependency and build directories are **not** listed. They are what
/// `.gitignore` is for, and honoring the project's own declaration is the whole
/// contract; a second, hidden list would answer a question the repository has
/// already answered, and would quietly contradict `--scope all`.
const VCS_DIRS: &[&str] = &[".git", ".hg", ".svn"];

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

impl Scope {
    pub fn label(self) -> &'static str {
        match self {
            Scope::Shared => "shared",
            Scope::Local => "local",
            Scope::All => "all",
        }
    }
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

/// What the walk found, and what it did not.
pub struct Survey {
    pub candidates: Vec<Candidate>,
    /// Files no profile claimed, tallied by extension. The headline sums every
    /// cohort, so a missing profile skews it — naming the extensions keeps that
    /// visible instead of letting it read as measured.
    pub unsupported: BTreeMap<String, u64>,
    /// The `.ernestignore` files that were in effect, so an excluded corpus is
    /// declared in the report rather than silently absent from it.
    pub ernestignore: Vec<PathBuf>,
    /// What the walk was asked for. Carried so the report — and the snapshot it
    /// serialises — can say what produced it: a figure that does not name its
    /// scope cannot be reasoned about, and two snapshots taken at different ones
    /// compare as though they were the same measurement.
    pub scope: Scope,
    pub lang: Option<String>,
    pub roots: Vec<PathBuf>,
}

/// Every supported file under `roots`, plus what was passed over — so coverage
/// is visible rather than assumed.
pub fn collect(roots: &[PathBuf], scope: Scope, lang: Option<&str>) -> Survey {
    let walked = files(roots, scope, lang);
    let excludes = LocalExcludes::for_roots(roots);

    // At the widest scope a file may be hidden by `.gitignore` instead, which
    // no single matcher answers; the narrower walk is what names those.
    let reference: Option<HashSet<PathBuf>> = (scope == Scope::All).then(|| {
        files(roots, Scope::Local, lang)
            .found
            .into_iter()
            .map(|(path, _)| path)
            .collect()
    });

    let mut candidates: Vec<Candidate> = walked
        .found
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
    Survey {
        candidates,
        unsupported: walked.unsupported,
        ernestignore: walked.ernestignore,
        scope,
        lang: lang.map(str::to_string),
        roots: roots.to_vec(),
    }
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

struct Walked {
    found: Vec<(PathBuf, &'static Profile)>,
    unsupported: BTreeMap<String, u64>,
    ernestignore: Vec<PathBuf>,
}

fn files(roots: &[PathBuf], scope: Scope, lang: Option<&str>) -> Walked {
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
        // Without this the crate applies no git-derived rule outside a `.git`,
        // and a `.gitignore` sitting on its own is inert. A yadm-managed tree is
        // exactly that shape — the work tree is `$HOME` and the git dir lives
        // elsewhere — so this repository's own `/target` rule would go unread,
        // and delegating build output to `.gitignore` would silently not work
        // in the tree ernest was written in.
        .require_git(false)
        // Unconditional: a corpus is not ernest's subject at any scope, which
        // is what separates this from the git-derived rules above.
        .add_custom_ignore_filename(ERNESTIGNORE)
        .filter_entry(|entry| {
            entry
                .file_name()
                .to_str()
                .is_none_or(|name| !VCS_DIRS.contains(&name))
        });

    let mut walked = Walked {
        found: Vec::new(),
        unsupported: BTreeMap::new(),
        ernestignore: Vec::new(),
    };

    for entry in builder.build() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.into_path();
        // Collected from the walk rather than guessed at the roots, so one
        // placed in a subdirectory is reported too.
        if path.file_name().is_some_and(|n| n == ERNESTIGNORE) {
            walked.ernestignore.push(path);
            continue;
        }
        match profile_for(&path) {
            Some(profile) if lang.is_none_or(|l| l == profile.language) => {
                walked.found.push((path, profile))
            }
            // Narrowed away by `--lang`, not unsupported: counting it would
            // read as a coverage gap that is not there.
            Some(_) => {}
            None => *walked.unsupported.entry(extension_of(&path)).or_insert(0) += 1,
        }
    }

    walked.ernestignore.sort();
    walked
}

/// The key an unsupported file is tallied under — its extension, which is what
/// names the profile that would cover it.
fn extension_of(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_else(|| "(no extension)".to_string())
}

/// A root the user named explicitly is read whether or not it is a directory,
/// so `ernest one-file.php` behaves.
pub fn is_readable_root(path: &Path) -> bool {
    path.exists()
}
