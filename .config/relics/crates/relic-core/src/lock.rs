//! Advisory file locking, with the one rule that matters made unrepresentable.
//!
//! **Bound the wait, never the hold.** A relic runs from session-start hooks and
//! from `up`; a caller that waits forever for a lock another session holds is a
//! hung terminal, not a queue. So [`Wait`] has no "forever" variant — it cannot
//! be asked for. The *hold* is never bounded in the other direction either:
//! releasing a lock part-way through a write is worse than any wait.
//!
//! The lock is released when the guard drops, which is why every caller binds it
//! (`let _guard = …`) rather than discarding it. `let _ = …` would take the lock
//! and release it on the same line.

use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// First gap between attempts. Doubles up to [`MAX_BACKOFF`].
const FIRST_BACKOFF: Duration = Duration::from_millis(10);

/// Longest gap between attempts, so a long wait stays responsive to the lock
/// being released rather than sleeping through it.
const MAX_BACKOFF: Duration = Duration::from_millis(100);

/// How long a caller will wait for a lock somebody else holds.
///
/// Deliberately not an `Option<Duration>` and deliberately without a `Forever`:
/// a type that cannot express an unbounded wait is a codebase that cannot
/// accidentally take one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Wait {
    /// Try once and give up. What anything on a hook path takes.
    None,
    /// Retry until the deadline, then fail and say who is holding it.
    Until(Duration),
}

impl Wait {
    /// The default for a command a person is waiting on at a prompt: long
    /// enough to outlast another session's write, short enough to be a pause
    /// rather than a hang.
    pub const INTERACTIVE: Self = Self::Until(Duration::from_secs(5));
}

/// Why a lock could not be taken.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The lock file itself could not be created or opened.
    #[error("opening the lock file {}", .path.display())]
    Open {
        /// The lock file.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: io::Error,
    },
    /// Somebody else holds it, and the wait ran out.
    #[error("{} is held by another process (waited {waited:?})", .path.display())]
    Busy {
        /// The lock file.
        path: PathBuf,
        /// How long this caller waited before giving up.
        waited: Duration,
    },
    /// The lock operation failed for a reason that is not contention.
    #[error("locking {}", .path.display())]
    Io {
        /// The lock file.
        path: PathBuf,
        /// The underlying failure.
        #[source]
        source: io::Error,
    },
}

/// A held lock. Releases when dropped.
#[derive(Debug)]
pub struct Lock {
    // Held for its side effect: closing the file releases the advisory lock.
    _file: File,
}

impl Lock {
    /// Take the lock, waiting no longer than `wait` says.
    ///
    /// # Errors
    ///
    /// [`Error::Busy`] when the wait ran out, [`Error::Open`] or [`Error::Io`]
    /// when the filesystem refused.
    pub fn acquire(path: &Path, wait: Wait) -> Result<Self, Error> {
        match Self::poll(path, wait)? {
            Some(lock) => Ok(lock),
            None => Err(Error::Busy {
                path: path.to_path_buf(),
                waited: match wait {
                    Wait::None => Duration::ZERO,
                    Wait::Until(d) => d,
                },
            }),
        }
    }

    /// Take the lock if it is free this instant, and report plainly when it is
    /// not.
    ///
    /// Distinct from `acquire(path, Wait::None).ok()`, which would fold a real
    /// filesystem error into "somebody else has it" — the silent-failure shape
    /// this crate exists to avoid.
    ///
    /// # Errors
    ///
    /// [`Error::Open`] or [`Error::Io`]. Contention is `Ok(None)`, not an error.
    pub fn try_acquire(path: &Path) -> Result<Option<Self>, Error> {
        Self::poll(path, Wait::None)
    }

    fn poll(path: &Path, wait: Wait) -> Result<Option<Self>, Error> {
        let file = File::create(path).map_err(|source| Error::Open {
            path: path.to_path_buf(),
            source,
        })?;

        let deadline = match wait {
            Wait::None => None,
            Wait::Until(d) => Some(Instant::now() + d),
        };
        let mut backoff = FIRST_BACKOFF;

        loop {
            match file.try_lock() {
                Ok(()) => return Ok(Some(Self { _file: file })),
                // Contention is not a failure; it is the answer to the question.
                Err(std::fs::TryLockError::WouldBlock) => {}
                Err(std::fs::TryLockError::Error(source)) => {
                    return Err(Error::Io {
                        path: path.to_path_buf(),
                        source,
                    });
                }
            }

            let Some(deadline) = deadline else {
                return Ok(None);
            };
            let now = Instant::now();
            if now >= deadline {
                return Ok(None);
            }
            // Never sleep past the deadline: the last attempt should happen at
            // the deadline, not after it.
            std::thread::sleep(backoff.min(deadline - now));
            backoff = (backoff * 2).min(MAX_BACKOFF);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("relic-core-lock-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("lock")
    }

    #[test]
    fn a_free_lock_is_taken() {
        let path = scratch("free");
        let lock = Lock::acquire(&path, Wait::None).unwrap();
        drop(lock);
    }

    #[test]
    fn a_held_lock_is_reported_rather_than_waited_on_forever() {
        let path = scratch("held");
        let _held = Lock::acquire(&path, Wait::None).unwrap();
        let err = Lock::acquire(&path, Wait::Until(Duration::from_millis(30))).unwrap_err();
        assert!(matches!(err, Error::Busy { .. }), "{err:?}");
    }

    #[test]
    fn try_acquire_separates_contention_from_failure() {
        let path = scratch("try");
        let _held = Lock::acquire(&path, Wait::None).unwrap();
        assert!(Lock::try_acquire(&path).unwrap().is_none());
    }

    #[test]
    fn the_lock_is_free_again_once_the_guard_drops() {
        let path = scratch("drop");
        let held = Lock::acquire(&path, Wait::None).unwrap();
        drop(held);
        assert!(Lock::try_acquire(&path).unwrap().is_some());
    }

    #[test]
    fn a_bounded_wait_does_not_outlast_its_bound() {
        let path = scratch("bound");
        let _held = Lock::acquire(&path, Wait::None).unwrap();
        let budget = Duration::from_millis(80);
        let started = Instant::now();
        let _ = Lock::acquire(&path, Wait::Until(budget));
        let waited = started.elapsed();
        assert!(waited >= budget, "gave up early: {waited:?}");
        assert!(waited < budget * 4, "overshot the bound: {waited:?}");
    }

    #[test]
    fn an_unopenable_path_is_not_reported_as_contention() {
        let path = scratch("bad")
            .join("no")
            .join("such")
            .join("dir")
            .join("lock");
        let err = Lock::try_acquire(&path).unwrap_err();
        assert!(matches!(err, Error::Open { .. }), "{err:?}");
    }
}
