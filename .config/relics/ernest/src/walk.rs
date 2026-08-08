//! Finding the files worth reading.

use std::path::{Path, PathBuf};

use ignore::WalkBuilder;

use crate::analyze::profiles::Profile;
use crate::detect::profile_for;

/// Directory names never worth walking into. They hold dependencies and build
/// output — text nobody wrote and nobody loads into context on purpose.
/// `.gitignore` covers most of them in a repository; this list is what keeps
/// the answer sane outside one.
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

pub struct Candidate {
    pub path: PathBuf,
    pub profile: &'static Profile,
}

/// Every supported file under `roots`, plus a tally of the files skipped for
/// having no profile — so coverage is visible rather than assumed.
pub fn collect(roots: &[PathBuf], respect_ignore: bool, lang: Option<&str>) -> (Vec<Candidate>, u64) {
    let mut builder = WalkBuilder::new(&roots[0]);
    for root in &roots[1..] {
        builder.add(root);
    }
    builder
        // Dotfiles are the point on this machine, so hidden entries are walked;
        // the exclude list below is what still keeps `.git` out.
        .hidden(false)
        .parents(respect_ignore)
        .git_ignore(respect_ignore)
        .git_global(respect_ignore)
        .git_exclude(respect_ignore)
        .ignore(respect_ignore);

    if respect_ignore {
        builder.filter_entry(|entry| {
            entry
                .file_name()
                .to_str()
                .is_none_or(|name| !DEFAULT_EXCLUDES.contains(&name))
        });
    }

    let mut candidates = Vec::new();
    let mut skipped = 0u64;

    for entry in builder.build() {
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.into_path();
        match profile_for(&path) {
            Some(profile) if lang.is_none_or(|l| l == profile.language) => {
                candidates.push(Candidate { path, profile })
            }
            Some(_) => {}
            None => skipped += 1,
        }
    }

    candidates.sort_by(|a, b| a.path.cmp(&b.path));
    (candidates, skipped)
}

/// A root the user named explicitly is read whether or not it is a directory,
/// so `ernest one-file.php` behaves.
pub fn is_readable_root(path: &Path) -> bool {
    path.exists()
}
