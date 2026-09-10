//! What an asked source last said, and the terms on which it may be reused.
//!
//! The prompt path reads this and never the producer. A cache that has gone off
//! is still rendered while a refresh runs behind it — stale-while-revalidate,
//! because a prompt that waits for a subprocess is a prompt that stutters, and
//! a notice arriving one prompt late costs nothing.

use anyhow::{Context, Result};
use camino::Utf8Path;
use jiff::{Timestamp, tz::TimeZone};
use relic_core::finding::Report;
use serde::{Deserialize, Serialize};

use crate::source::{Ask, Key};

/// Bumped when this record's shape changes, so an older binary's file reads as
/// absent rather than being misread.
const SCHEMA: u32 = 1;

/// One source's answer, with everything needed to decide whether to believe it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cached {
    /// Schema guard.
    pub v: u32,
    /// When the producer answered.
    pub fetched_at: Timestamp,
    /// The fingerprint of everything the answer depends on.
    pub keys: String,
    /// What it said.
    pub report: Report,
}

/// Why a cached answer may not be reused.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Freshness {
    /// Reuse it.
    Fresh,
    /// Reuse it, and refresh behind the prompt.
    Stale,
    /// There is nothing to reuse.
    Cold,
}

impl Cached {
    /// Read one, treating anything wrong with it as absent.
    pub fn load(path: &Utf8Path) -> Option<Self> {
        let text = fs_err::read_to_string(path).ok()?;
        let cached: Self = serde_json::from_str(&text).ok()?;
        (cached.v == SCHEMA).then_some(cached)
    }

    /// Write one.
    ///
    /// # Errors
    ///
    /// When the tree cannot be created or the file cannot be written.
    pub fn store(path: &Utf8Path, report: Report, keys: String, now: Timestamp) -> Result<()> {
        let record = Self {
            v: SCHEMA,
            fetched_at: now,
            keys,
            report,
        };
        if let Some(parent) = path.parent() {
            fs_err::create_dir_all(parent)?;
        }
        let text = serde_json::to_string(&record).context("serialising a cached report")?;
        relic_core::fs::write_atomic(path, &text).with_context(|| format!("writing {path}"))?;
        Ok(())
    }
}

/// Decide whether a cached answer may be reused.
pub fn freshness(cached: Option<&Cached>, ask: &Ask, now: Timestamp, keys: &str) -> Freshness {
    let Some(cached) = cached else {
        return Freshness::Cold;
    };
    if cached.keys != keys {
        return Freshness::Stale;
    }
    let elapsed = now
        .as_second()
        .saturating_sub(cached.fetched_at.as_second());
    // A record from the future is a clock that moved backwards, not an answer
    // good until the clock catches up.
    if elapsed < 0 || u64::try_from(elapsed).unwrap_or(u64::MAX) >= ask.refresh.as_secs() {
        return Freshness::Stale;
    }
    Freshness::Fresh
}

/// Fingerprint everything an answer depends on.
///
/// A path contributes its mtime and size; a missing one contributes its absence,
/// which is itself a fact that can change. A day contributes the calendar date
/// under the declared rollover, so a drill that comes due at four in the morning
/// invalidates then and not at midnight.
pub fn fingerprint(keys: &[Key], now: Timestamp) -> String {
    let mut parts: Vec<String> = Vec::new();
    for key in keys {
        parts.push(match key {
            Key::Path(path) => match fs_err::metadata(path) {
                Ok(meta) => {
                    let seconds = meta
                        .modified()
                        .ok()
                        .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                        .map_or(0, |since| since.as_secs());
                    format!("{path}={seconds}:{}", meta.len())
                }
                Err(_) => format!("{path}=absent"),
            },
            Key::Day { hour, minute } => format!("day={}", drill_day(*hour, *minute, now)),
        });
    }
    parts.join("|")
}

/// The calendar day under a rollover that need not be midnight.
fn drill_day(hour: i8, minute: i8, now: Timestamp) -> String {
    let zoned = now.to_zoned(TimeZone::system());
    let offset = i64::from(hour) * 3600 + i64::from(minute) * 60;
    let shifted = zoned.timestamp().as_second().saturating_sub(offset);
    Timestamp::from_second(shifted)
        .map(|at| at.to_zoned(TimeZone::system()).date().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{Cached, Freshness, fingerprint, freshness};
    use crate::source::{Ask, Key, Kind};
    use camino::Utf8PathBuf;
    use jiff::Timestamp;
    use relic_core::finding::{Report, StationId};
    use std::time::Duration;

    fn ask(refresh: Duration) -> Ask {
        Ask {
            run: vec!["true".to_owned()],
            kind: Kind::Text,
            refresh,
            keys: Vec::new(),
        }
    }

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).unwrap()
    }

    fn cached(fetched: i64, keys: &str) -> Cached {
        Cached {
            v: 1,
            fetched_at: at(fetched),
            keys: keys.to_owned(),
            report: Report::ran(StationId::from_static("x"), Vec::new()),
        }
    }

    #[test]
    fn nothing_cached_is_cold() {
        assert_eq!(
            freshness(None, &ask(Duration::from_secs(600)), at(0), ""),
            Freshness::Cold
        );
    }

    #[test]
    fn a_recent_answer_with_matching_keys_is_reused() {
        let record = cached(1000, "k");
        assert_eq!(
            freshness(Some(&record), &ask(Duration::from_secs(600)), at(1100), "k"),
            Freshness::Fresh
        );
    }

    #[test]
    fn the_clock_alone_can_stale_an_answer() {
        let record = cached(1000, "k");
        assert_eq!(
            freshness(Some(&record), &ask(Duration::from_secs(600)), at(2000), "k"),
            Freshness::Stale
        );
    }

    #[test]
    fn a_moved_fingerprint_stales_an_answer_the_clock_still_likes() {
        let record = cached(1000, "k");
        assert_eq!(
            freshness(
                Some(&record),
                &ask(Duration::from_secs(600)),
                at(1001),
                "other"
            ),
            Freshness::Stale
        );
    }

    #[test]
    fn an_answer_from_the_future_is_not_trusted_until_the_clock_agrees() {
        let record = cached(9000, "k");
        assert_eq!(
            freshness(Some(&record), &ask(Duration::from_secs(600)), at(1000), "k"),
            Freshness::Stale
        );
    }

    #[test]
    fn a_paths_fingerprint_moves_when_the_file_does() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("f")).unwrap();
        fs_err::write(&path, "one").unwrap();
        let keys = vec![Key::Path(path.clone())];
        let before = fingerprint(&keys, at(0));
        fs_err::write(&path, "one plus more").unwrap();
        assert_ne!(before, fingerprint(&keys, at(0)));
    }

    #[test]
    fn an_absent_path_fingerprints_as_absent_rather_than_as_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("gone")).unwrap();
        let keys = vec![Key::Path(path.clone())];
        let absent = fingerprint(&keys, at(0));
        assert!(absent.contains("absent"));
        fs_err::write(&path, "back").unwrap();
        assert_ne!(absent, fingerprint(&keys, at(0)));
    }

    #[test]
    fn a_day_key_holds_across_the_day_and_moves_across_the_rollover() {
        let keys = vec![Key::Day { hour: 4, minute: 0 }];
        let noon = at(1_789_041_600);
        assert_eq!(
            fingerprint(&keys, noon),
            fingerprint(&keys, at(noon.as_second() + 3600))
        );
        assert_ne!(
            fingerprint(&keys, noon),
            fingerprint(&keys, at(noon.as_second() + 86_400))
        );
    }
}
