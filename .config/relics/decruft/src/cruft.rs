//! What counts as cruft, and what deliberately does not.
//!
//! Two families qualify, and they share one property: the tool that wrote the
//! file rebuilds it on demand, so deleting it costs nothing but the rebuild.

/// Names that are always safe to delete.
const CRUFT: &[&str] = &[
    // OS and filesystem metadata.
    ".DS_Store",
    ".DS_Store?",
    ".AppleDouble",
    ".LSOverride",
    ".apdisk",
    ".Spotlight-V100",
    ".Trashes",
    ".fseventsd",
    ".DocumentRevisions-V100",
    ".TemporaryItems",
    ".VolumeIcon.icns",
    ".com.apple.timemachine.donotpresent",
    "Thumbs.db",
    "Thumbs.db:encryptable",
    "ehthumbs.db",
    "Desktop.ini",
    ".directory",
    // Interpreter caches — the same set the encrypt patterns exclude from the
    // archive, for the same reason.
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
];

/// A name that looks like an `AppleDouble` fork but is not.
///
/// zinit writes its metadata under this name. The `._*` rule would otherwise
/// eat a plugin manager's own state.
const NOT_A_FORK: &[&str] = &["._zinit"];

/// Directories never descended into.
///
/// Cost, not safety: every one of these is inert, and every one is expensive to
/// rebuild. They are also where the walking time goes.
///
/// The interpreter caches are deliberately absent. They are cruft, so they are
/// *removed* — and a directory that is pruned for descent is never yielded, so
/// pruning one would be the same as never finding it.
pub const PRUNED: &[&str] = &[
    "node_modules",
    "target",
    "target-native",
    "vendor",
    "Pods",
    ".venv",
    "venv",
    ".build",
    ".next",
    ".output",
    "dist",
    "build",
    ".gradle",
    ".m2",
    ".cargo",
    ".rustup",
    ".npm",
    ".cache",
    ".pnpm-store",
];

/// Whether a file or directory name is cruft.
///
/// Editor swap, backup and lock files (`*~`, `*.swp`, `#*#`, `.#*`) are
/// deliberately absent. The central git excludes ignore them, which keeps them
/// out of commits — but a live vim swap or emacs lock is crash-recovery state,
/// and this may run while an editor is open. Ignoring is not deleting.
#[must_use]
pub fn is_cruft(name: &str) -> bool {
    if NOT_A_FORK.contains(&name) {
        return false;
    }
    if name.starts_with("._") {
        return true;
    }
    CRUFT.contains(&name)
}

/// Whether a directory name is one this never descends into.
#[must_use]
pub fn is_pruned(name: &str) -> bool {
    PRUNED.contains(&name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_families_are_cruft() {
        assert!(is_cruft(".DS_Store"));
        assert!(is_cruft("__pycache__"));
        assert!(is_cruft("._resource-fork"));
    }

    /// A plugin manager's metadata is not an `AppleDouble` fork, however much the
    /// name looks like one.
    #[test]
    fn a_name_that_only_looks_like_a_fork_is_kept() {
        assert!(!is_cruft("._zinit"));
    }

    /// Ignoring these keeps them out of commits. Deleting one throws away
    /// crash-recovery state, which is a different decision.
    #[test]
    fn editor_state_is_not_cruft() {
        for name in ["notes.txt~", ".notes.txt.swp", "#notes.txt#", ".#notes.txt"] {
            assert!(!is_cruft(name), "{name}");
        }
    }

    #[test]
    fn ordinary_files_are_not_cruft() {
        for name in ["README.md", ".gitignore", "src", ".env"] {
            assert!(!is_cruft(name), "{name}");
        }
    }

    /// A pruned directory is never yielded, so a name cannot be both: pruning
    /// a cruft name would be the same as never finding it.
    #[test]
    fn nothing_is_both_pruned_and_cruft() {
        for name in PRUNED {
            assert!(
                !is_cruft(name),
                "{name} is pruned, so it could never be removed"
            );
        }
        assert!(is_cruft("__pycache__"));
        assert!(is_pruned("node_modules"));
        assert!(!is_cruft("node_modules"));
    }
}
