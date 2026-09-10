//! The stat tier: conditions coop evaluates itself, on every prompt.
//!
//! This is what makes a notice retire itself. `up` does not announce that it
//! ran and does not retract anything — its notice *is* a predicate over the
//! timestamp it already writes, so running it is the dismissal.
//!
//! Nothing here starts a process. The whole tier is `stat(2)` and arithmetic,
//! which is what keeps it honest to run before every prompt.

use std::time::Duration;

use anyhow::{Result, bail};
use camino::Utf8PathBuf;
use jiff::Timestamp;

/// What a missing path means. There is no third answer, and no default that
/// hides the question: "the file `up` writes is absent" reads as "never run" for
/// one declaration and "not installed here" for the next.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Missing {
    /// Absent means outstanding.
    Fire,
    /// Absent means nothing to say.
    Quiet,
}

/// A condition over the filesystem and the clock.
#[derive(Clone, Debug)]
pub enum Predicate {
    /// The path has not been touched within the span.
    OlderThan {
        /// The path whose mtime is read.
        path: Utf8PathBuf,
        /// How much staleness is tolerated.
        age: Duration,
        /// What an absent path means.
        missing: Missing,
    },
    /// The path is there.
    Exists {
        /// The path to look for.
        path: Utf8PathBuf,
    },
    /// The clock has not reached the instant.
    Before(Timestamp),
    /// The clock has passed the instant.
    After(Timestamp),
    /// Every branch fires.
    AllOf(Vec<Predicate>),
    /// Some branch fires.
    AnyOf(Vec<Predicate>),
    /// The branch does not fire.
    Not(Box<Predicate>),
}

/// What a firing predicate learned on the way, for the summary to interpolate.
///
/// One variable today. It is a struct rather than a bare `Option<String>`
/// because the second one will not be an age.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Vars {
    /// How stale the path that fired actually is, humanised.
    pub age: Option<String>,
}

impl Vars {
    fn merge(mut self, other: Self) -> Self {
        self.age = self.age.or(other.age);
        self
    }

    /// Substitute into a declaration's summary.
    ///
    /// An unresolved placeholder is left standing rather than blanked: a visible
    /// `{age}` in the card is a bug report, and an empty gap is a mystery.
    pub fn apply(&self, template: &str) -> String {
        match &self.age {
            Some(age) => template.replace("{age}", age),
            None => template.to_owned(),
        }
    }
}

impl Predicate {
    /// Evaluate against the filesystem and an injected clock.
    ///
    /// `None` is "nothing outstanding". `Some(vars)` is "outstanding, and here
    /// is what to say about it".
    ///
    /// The clock is a parameter so staleness can be tested without setting an
    /// mtime: a file written now, read against a `now` two days ahead, is a
    /// two-day-old file by every definition this tier has.
    pub fn eval(&self, now: Timestamp) -> Option<Vars> {
        match self {
            Self::OlderThan { path, age, missing } => match mtime(path) {
                Some(at) => {
                    let elapsed = now.as_second().saturating_sub(at.as_second());
                    let over = u64::try_from(elapsed).unwrap_or(0) >= age.as_secs();
                    over.then(|| Vars {
                        age: Some(relic_core::fmt::age(at, now)),
                    })
                }
                None => match missing {
                    Missing::Fire => Some(Vars::default()),
                    Missing::Quiet => None,
                },
            },
            Self::Exists { path } => path.exists().then(Vars::default),
            Self::Before(instant) => (now < *instant).then(Vars::default),
            Self::After(instant) => (now > *instant).then(Vars::default),
            Self::AllOf(branches) => {
                let mut vars = Vars::default();
                for branch in branches {
                    vars = vars.merge(branch.eval(now)?);
                }
                Some(vars)
            }
            Self::AnyOf(branches) => branches.iter().find_map(|branch| branch.eval(now)),
            Self::Not(inner) => inner.eval(now).is_none().then(Vars::default),
        }
    }
}

fn mtime(path: &Utf8PathBuf) -> Option<Timestamp> {
    let modified = fs_err::metadata(path).ok()?.modified().ok()?;
    let since = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    Timestamp::from_second(i64::try_from(since.as_secs()).ok()?).ok()
}

/// An empty branch list is a declaration that says nothing, and a predicate that
/// says nothing is the shape that quietly stops guarding.
///
/// # Errors
///
/// When a combinator was given no branches.
pub fn non_empty(branches: &[Predicate], label: &str) -> Result<()> {
    if branches.is_empty() {
        bail!("{label} has no branches, so it can never say anything");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Missing, Predicate, Vars};
    use camino::Utf8PathBuf;
    use jiff::Timestamp;
    use std::time::Duration;

    const DAY: Duration = Duration::from_secs(86_400);

    fn now() -> Timestamp {
        Timestamp::now()
    }

    /// A clock `days` from the file's own timestamp. Staleness is tested by
    /// moving the clock rather than the mtime: setting an mtime is a syscall
    /// this tier does not otherwise need, and the arithmetic is the same.
    fn ahead(path: &Utf8PathBuf, days: i64) -> Timestamp {
        let modified = fs_err::metadata(path).unwrap().modified().unwrap();
        let since = modified.duration_since(std::time::UNIX_EPOCH).unwrap();
        Timestamp::from_second(i64::try_from(since.as_secs()).unwrap() + days * 86_400).unwrap()
    }

    fn temp() -> tempfile::TempDir {
        tempfile::tempdir().unwrap()
    }

    fn touched(dir: &tempfile::TempDir, name: &str) -> Utf8PathBuf {
        let path = Utf8PathBuf::from_path_buf(dir.path().join(name)).unwrap();
        fs_err::write(&path, "x").unwrap();
        path
    }

    fn older_than(path: Utf8PathBuf, missing: Missing) -> Predicate {
        Predicate::OlderThan {
            path,
            age: DAY,
            missing,
        }
    }

    #[test]
    fn a_fresh_path_says_nothing() {
        let dir = temp();
        let path = touched(&dir, "stamp");
        assert_eq!(older_than(path, Missing::Fire).eval(now()), None);
    }

    #[test]
    fn a_stale_path_fires_and_carries_its_age() {
        let dir = temp();
        let path = touched(&dir, "stamp");
        let vars = older_than(path.clone(), Missing::Fire)
            .eval(ahead(&path, 3))
            .unwrap();
        assert_eq!(vars.age.as_deref(), Some("3d"));
    }

    #[test]
    fn the_boundary_is_inclusive_so_a_daily_nag_arrives_on_the_day() {
        let dir = temp();
        let path = touched(&dir, "stamp");
        assert!(
            older_than(path.clone(), Missing::Fire)
                .eval(ahead(&path, 1))
                .is_some()
        );
    }

    #[test]
    fn a_missing_path_is_whichever_the_declaration_said() {
        let dir = temp();
        let absent = Utf8PathBuf::from_path_buf(dir.path().join("nope")).unwrap();
        assert!(
            older_than(absent.clone(), Missing::Fire)
                .eval(now())
                .is_some()
        );
        assert!(older_than(absent, Missing::Quiet).eval(now()).is_none());
    }

    #[test]
    fn a_missing_path_that_fires_has_no_age_to_report() {
        let dir = temp();
        let absent = Utf8PathBuf::from_path_buf(dir.path().join("nope")).unwrap();
        let vars = older_than(absent, Missing::Fire).eval(now()).unwrap();
        assert_eq!(vars.age, None);
    }

    #[test]
    fn exists_and_not_are_inverses() {
        let dir = temp();
        let path = touched(&dir, "here");
        let exists = Predicate::Exists { path };
        assert!(exists.eval(now()).is_some());
        assert!(Predicate::Not(Box::new(exists)).eval(now()).is_none());
    }

    #[test]
    fn a_time_window_is_two_halves_of_one_line() {
        let at = |second: i64| Timestamp::from_second(second).unwrap();
        let clock = at(1_789_000_000);
        assert!(Predicate::After(at(1_788_000_000)).eval(clock).is_some());
        assert!(Predicate::After(at(1_790_000_000)).eval(clock).is_none());
        assert!(Predicate::Before(at(1_790_000_000)).eval(clock).is_some());
        assert!(Predicate::Before(at(1_788_000_000)).eval(clock).is_none());
    }

    #[test]
    fn all_of_needs_every_branch_and_keeps_the_first_age() {
        let dir = temp();
        let stale = touched(&dir, "stamp");
        let present = touched(&dir, "here");
        let both = Predicate::AllOf(vec![
            older_than(stale.clone(), Missing::Fire),
            Predicate::Exists { path: present },
        ]);
        assert_eq!(
            both.eval(ahead(&stale, 2)).unwrap().age.as_deref(),
            Some("2d")
        );
        assert_eq!(both.eval(now()), None, "the stale half is not stale yet");
    }

    #[test]
    fn any_of_takes_the_first_branch_that_fires() {
        let dir = temp();
        let absent = Utf8PathBuf::from_path_buf(dir.path().join("nope")).unwrap();
        let present = touched(&dir, "here");
        let either = Predicate::AnyOf(vec![
            Predicate::Exists { path: absent },
            Predicate::Exists { path: present },
        ]);
        assert!(either.eval(now()).is_some());
    }

    #[test]
    fn an_unresolved_placeholder_stays_visible() {
        let vars = Vars::default();
        assert_eq!(vars.apply("{age} since a thing"), "{age} since a thing");
    }

    #[test]
    fn a_resolved_placeholder_reads_as_prose() {
        let vars = Vars {
            age: Some("3d".to_owned()),
        };
        assert_eq!(vars.apply("{age} since a thing"), "3d since a thing");
    }
}
