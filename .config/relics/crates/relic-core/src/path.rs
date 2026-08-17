//! One meaning for a path, so two relics asked about the same place answer with
//! the same key.
//!
//! [`project_key`] is the shared key. It is deliberately the whole composition
//! and not its two halves: a relic that assembled the halves itself is a relic
//! that can assemble them differently, which is how the two implementations this
//! module replaces came to disagree.

use std::path::{Component, Path, PathBuf};

use crate::git;

/// The main checkout root of the containing repository, or the working
/// directory when there is none. Linked worktrees fold into their main
/// checkout, so a worktree and its origin are one project; a submodule reports
/// its own root, which is what a per-aspect repository layout needs.
///
/// Resolved on both exits, so the key never depends on how the path was spelled.
pub fn project_key(path: &Path) -> PathBuf {
    let resolved = resolve_lenient(path);
    match git::detect().and_then(|git| git.main_worktree(&resolved)) {
        Some(main) => resolve_lenient(&main),
        None => resolved,
    }
}

/// Absolute and symlink-free as far as the path exists, lexical the rest of the
/// way, so a target that has not been created yet keys the same as it will once
/// it is.
pub fn resolve_lenient(path: &Path) -> PathBuf {
    let expanded = expand_tilde(path);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir().unwrap_or_default().join(expanded)
    };

    let mut real = PathBuf::new();
    let mut tail = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => continue,
            Component::ParentDir => {
                if tail.as_os_str().is_empty() {
                    real.pop();
                } else {
                    // Nothing in the tail exists, so there is no symlink here
                    // for a lexical pop to be wrong about — which is the only
                    // reason the existing prefix is canonicalised instead.
                    // Emptying the tail resumes canonicalisation.
                    tail.pop();
                }
            }
            other => {
                if tail.as_os_str().is_empty() {
                    let candidate = real.join(other);
                    match candidate.canonicalize() {
                        Ok(resolved) => real = resolved,
                        Err(_) => tail.push(other),
                    }
                } else {
                    tail.push(other);
                }
            }
        }
    }
    // Joining an empty tail would append a separator, and a trailing separator
    // makes a different key for the same directory.
    if tail.as_os_str().is_empty() {
        real
    } else {
        real.join(tail)
    }
}

fn expand_tilde(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let Some(rest) = text.strip_prefix('~') else {
        return path.to_owned();
    };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return path.to_owned();
    }
    let Some(home) = std::env::var_os("HOME") else {
        return path.to_owned();
    };
    PathBuf::from(home).join(rest.trim_start_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lenient_resolution_keeps_the_missing_tail() {
        let resolved = resolve_lenient(Path::new("/tmp/definitely-absent-xyz/deeper"));
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("definitely-absent-xyz/deeper"));
    }

    #[test]
    fn resolution_is_idempotent() {
        let once = resolve_lenient(Path::new("/tmp/definitely-absent-xyz/deeper"));
        assert_eq!(resolve_lenient(&once), once);
    }

    /// A trailing separator, a `.` and a `..` all name one directory, and one
    /// directory is one key.
    #[test]
    fn spellings_of_one_directory_agree() {
        let plain = resolve_lenient(Path::new("/tmp"));
        assert_eq!(resolve_lenient(Path::new("/tmp/")), plain);
        assert_eq!(resolve_lenient(Path::new("/tmp/./")), plain);
        assert_eq!(resolve_lenient(Path::new("/tmp/absent/..")), plain);
        assert_eq!(resolve_lenient(Path::new("/tmp/a/b/../..")), plain);
    }

    #[test]
    fn a_bare_tilde_and_a_tilde_path_expand() {
        let Some(home) = std::env::var_os("HOME") else {
            return;
        };
        let home = resolve_lenient(Path::new(&home));
        assert_eq!(resolve_lenient(Path::new("~")), home);
        assert_eq!(resolve_lenient(Path::new("~/")), home);
    }

    /// `~user` is another user's home, which is not ours to expand.
    #[test]
    fn a_named_tilde_is_left_alone() {
        assert!(resolve_lenient(Path::new("~someone/else")).ends_with("~someone/else"));
    }
}
