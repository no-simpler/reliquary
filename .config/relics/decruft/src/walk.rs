//! Finding the repositories, and the one lane that has none.

use camino::{Utf8Path, Utf8PathBuf};
use ignore::WalkBuilder;

use crate::cruft;

/// Directories skipped by absolute path rather than by name: large trees, a
/// virtual machine's own filesystem, and tool-managed trees whose internals are
/// the tool's business rather than the user's leavings.
///
/// The last is the distinction that matters. A cache belonging to *your* code
/// is cruft; the bytecode cache of a vendored interpreter belongs to whatever
/// installed the interpreter, and removing it makes that tool slower for no
/// reclaimed space anyone asked for.
fn is_skipped(path: &Utf8Path, home: &Utf8Path, data: &Utf8Path) -> bool {
    [
        home.join("Library"),
        home.join(".Trash"),
        home.join("OrbStack"),
        home.join(".orbstack"),
        data.join("nexus"),
        data.join("zinit"),
        data.join("uv"),
    ]
    .iter()
    .any(|skipped| path == skipped)
}

/// A configured walker. Hidden entries are visited — the cruft is nearly all
/// hidden — and no ignore file is consulted: one walk is looking for
/// repositories rather than obeying them, and the other is inside a subtree a
/// repository has already condemned whole.
///
/// `prune_cruft` is what separates them. Repository discovery skips a cruft
/// directory, because there is no repository inside one. The by-name sweep must
/// not, because skipping a directory means never yielding it, and yielding it
/// is the point.
fn walker(root: &Utf8Path, home: &Utf8Path, data: &Utf8Path, prune_cruft: bool) -> ignore::Walk {
    let home = home.to_owned();
    let data = data.to_owned();
    let mut builder = WalkBuilder::new(root);
    builder
        .hidden(false)
        .parents(false)
        .ignore(false)
        .git_ignore(false)
        .git_global(false)
        .git_exclude(false)
        .follow_links(false)
        .filter_entry(move |entry| {
            let Some(name) = entry.file_name().to_str() else {
                return false;
            };
            if cruft::is_pruned(name) || (prune_cruft && cruft::is_cruft(name)) {
                return false;
            }
            match Utf8Path::from_path(entry.path()) {
                Some(path) => !is_skipped(path, &home, &data),
                // A path this program cannot spell is one it will not delete
                // from. Skipping is the safe direction.
                None => false,
            }
        });
    builder.build()
}

/// Every git repository under `root`, deepest-first-free: a repository found
/// inside another is reported too, because a submodule answers for itself.
///
/// The `.git` entry ends the descent for the walker's purposes only in that
/// nothing under it is cruft; the repository lane asks git about the whole
/// tree, so there is nothing more to find by walking further.
#[must_use]
pub fn repositories(root: &Utf8Path, home: &Utf8Path, data: &Utf8Path) -> Vec<Utf8PathBuf> {
    let mut found = Vec::new();
    for entry in walker(root, home, data, true).flatten() {
        if entry.file_name() != ".git" {
            continue;
        }
        let Some(path) = Utf8Path::from_path(entry.path()) else {
            continue;
        };
        if let Some(repo) = path.parent() {
            found.push(repo.to_owned());
        }
    }
    found.sort();
    found.dedup();
    found
}

/// Cruft under `root` by name alone, skipping anything inside one of `repos` —
/// those are the repository lane's, and a repository's own answer outranks a
/// name.
#[must_use]
pub fn cruft_by_name(
    root: &Utf8Path,
    home: &Utf8Path,
    data: &Utf8Path,
    repos: &[Utf8PathBuf],
) -> Vec<Utf8PathBuf> {
    let mut found = Vec::new();
    for entry in walker(root, home, data, false).flatten() {
        let Some(path) = Utf8Path::from_path(entry.path()) else {
            continue;
        };
        let Some(name) = path.file_name() else {
            continue;
        };
        if !cruft::is_cruft(name) {
            continue;
        }
        if repos.iter().any(|repo| path.starts_with(repo)) {
            continue;
        }
        found.push(path.to_owned());
    }
    found.sort();
    found.dedup();
    found
}
