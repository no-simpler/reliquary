//! One meaning for a path, so two relics asked about the same place answer with
//! the same key.
//!
//! Paths here are [`Utf8Path`], because a key is *program data*: it is compared,
//! stored and printed. `to_string_lossy` maps two different directories onto one
//! key, and serde's `PathBuf` refuses a path it cannot spell — deep inside a save
//! rather than at the edge. So UTF-8 is the parse. [`utf8`], [`cwd`] and [`home`]
//! are the only places a filesystem path becomes one, and everything past them is
//! a string by construction.
//!
//! [`project_key`] is the shared key. It is deliberately the whole composition
//! and not its two halves: a relic that assembled the halves itself is a relic
//! that can assemble them differently, which is how the two implementations this
//! module replaces came to disagree.

use std::io;
use std::path::PathBuf;

use camino::{Utf8Component, Utf8Path, Utf8PathBuf};

use crate::git;

/// Why a place could not be named.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The filesystem holds a path this program cannot spell.
    #[error("{} is not valid UTF-8", .0.display())]
    NotUtf8(PathBuf),
    /// The working directory could not be read — it was removed, or is not
    /// reachable from here.
    #[error("reading the working directory")]
    Cwd(#[source] io::Error),
}

/// The parse, at the edge: a filesystem path becomes one this program can name,
/// or says plainly that it cannot.
///
/// # Errors
///
/// [`Error::NotUtf8`].
pub fn utf8(path: PathBuf) -> Result<Utf8PathBuf, Error> {
    Utf8PathBuf::from_path_buf(path).map_err(Error::NotUtf8)
}

/// The working directory, named.
///
/// # Errors
///
/// [`Error::Cwd`] when it cannot be read, [`Error::NotUtf8`] when it cannot be
/// spelled.
pub fn cwd() -> Result<Utf8PathBuf, Error> {
    utf8(std::env::current_dir().map_err(Error::Cwd)?)
}

/// The home directory, when the environment names one this program can spell.
///
/// `None` rather than an error: every caller has somewhere else to go — a depot
/// root falls back to a flag, and a `~`-shortened cell falls back to the whole
/// path.
#[must_use]
pub fn home() -> Option<Utf8PathBuf> {
    utf8(PathBuf::from(std::env::var_os("HOME")?)).ok()
}

/// The main checkout root of the containing repository, or the working
/// directory when there is none. Linked worktrees fold into their main
/// checkout, so a worktree and its origin are one project; a submodule reports
/// its own root, which is what a per-aspect repository layout needs.
///
/// Resolved on both exits, so the key never depends on how the path was spelled.
///
/// # Errors
///
/// Whatever [`resolve_lenient`] reports.
pub fn project_key(path: &Utf8Path) -> Result<Utf8PathBuf, Error> {
    let resolved = resolve_lenient(path)?;
    match git::detect().and_then(|git| git.main_worktree(&resolved)) {
        Some(main) => resolve_lenient(&main),
        None => Ok(resolved),
    }
}

/// Absolute and symlink-free as far as the path exists, lexical the rest of the
/// way, so a target that has not been created yet keys the same as it will once
/// it is.
///
/// # Errors
///
/// [`Error::Cwd`] when a relative path has no working directory to anchor to,
/// [`Error::NotUtf8`] when a symlink resolves to somewhere unnameable — which is
/// reported rather than skipped, because skipping it would return a *different*
/// key rather than no key.
pub fn resolve_lenient(path: &Utf8Path) -> Result<Utf8PathBuf, Error> {
    let expanded = expand_tilde(path);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        cwd()?.join(expanded)
    };

    let mut real = Utf8PathBuf::new();
    let mut tail = Utf8PathBuf::new();
    for component in absolute.components() {
        match component {
            Utf8Component::CurDir => continue,
            Utf8Component::ParentDir => {
                if tail.as_str().is_empty() {
                    real.pop();
                } else {
                    // Nothing in the tail exists, so there is no symlink here
                    // for a lexical pop to be wrong about — which is the only
                    // reason the existing prefix is canonicalised instead.
                    // Emptying the tail resumes canonicalisation.
                    tail.pop();
                }
            }
            Utf8Component::Prefix(_) | Utf8Component::RootDir | Utf8Component::Normal(_) => {
                if tail.as_str().is_empty() {
                    let candidate = real.join(component);
                    match candidate.canonicalize_utf8() {
                        Ok(resolved) => real = resolved,
                        Err(e) if e.kind() == io::ErrorKind::InvalidData => {
                            return Err(Error::NotUtf8(candidate.into_std_path_buf()));
                        }
                        Err(_) => tail.push(component),
                    }
                } else {
                    tail.push(component);
                }
            }
        }
    }
    // Joining an empty tail would append a separator, and a trailing separator
    // makes a different key for the same directory.
    Ok(if tail.as_str().is_empty() {
        real
    } else {
        real.join(tail)
    })
}

fn expand_tilde(path: &Utf8Path) -> Utf8PathBuf {
    let Some(rest) = path.as_str().strip_prefix('~') else {
        return path.to_owned();
    };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return path.to_owned();
    }
    match home() {
        Some(home) => home.join(rest.trim_start_matches('/')),
        None => path.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolve(path: &str) -> Utf8PathBuf {
        resolve_lenient(Utf8Path::new(path)).expect("resolvable")
    }

    #[test]
    fn lenient_resolution_keeps_the_missing_tail() {
        let resolved = resolve("/tmp/definitely-absent-xyz/deeper");
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("definitely-absent-xyz/deeper"));
    }

    #[test]
    fn resolution_is_idempotent() {
        let once = resolve("/tmp/definitely-absent-xyz/deeper");
        assert_eq!(resolve(once.as_str()), once);
    }

    /// A trailing separator, a `.` and a `..` all name one directory, and one
    /// directory is one key.
    #[test]
    fn spellings_of_one_directory_agree() {
        let plain = resolve("/tmp");
        assert_eq!(resolve("/tmp/"), plain);
        assert_eq!(resolve("/tmp/./"), plain);
        assert_eq!(resolve("/tmp/absent/.."), plain);
        assert_eq!(resolve("/tmp/a/b/../.."), plain);
    }

    #[test]
    fn a_bare_tilde_and_a_tilde_path_expand() {
        let Some(home) = home() else {
            return;
        };
        let home = resolve(home.as_str());
        assert_eq!(resolve("~"), home);
        assert_eq!(resolve("~/"), home);
    }

    /// `~user` is another user's home, which is not ours to expand.
    #[test]
    fn a_named_tilde_is_left_alone() {
        assert!(resolve("~someone/else").ends_with("~someone/else"));
    }

    #[test]
    fn a_path_that_cannot_be_spelled_is_refused_at_the_edge() {
        use std::os::unix::ffi::OsStringExt;
        let raw = PathBuf::from(std::ffi::OsString::from_vec(vec![0xff, 0xfe]));
        assert!(matches!(utf8(raw), Err(Error::NotUtf8(_))));
    }
}
