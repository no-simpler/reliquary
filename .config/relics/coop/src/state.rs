//! The two small records that let `doctor` say something the card cannot.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::Utf8Path;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::notice::Notice;

/// How long a notice may stand before it stops being a nag and starts being
/// furniture.
pub const HORIZON_DAYS: i64 = 14;
/// What one tick is allowed to cost before it is a finding.
pub const TICK_BUDGET_MICROS: u64 = 15_000;
/// How often the tick measurement is rewritten when it is inside budget.
const TIMING_REFRESH_SECONDS: i64 = 3600;

/// When each outstanding notice was first seen.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FirstSeen {
    /// Identity to instant.
    #[serde(default)]
    pub at: BTreeMap<String, Timestamp>,
}

impl FirstSeen {
    /// Read it, treating anything wrong with it as empty.
    pub fn load(path: &Utf8Path) -> Self {
        fs_err::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Stamp what is outstanding now, and forget what no longer is.
    ///
    /// Pruning is what keeps this from becoming a log. Once a notice is gone its
    /// first sighting is history, and coop is not the place history lives.
    ///
    /// # Errors
    ///
    /// When the record cannot be written.
    pub fn update(path: &Utf8Path, notices: &[Notice], now: Timestamp) -> Result<Self> {
        let mut record = Self::load(path);
        let mut next = BTreeMap::new();
        for notice in notices {
            let id = notice.identity();
            let at = record.at.remove(&id).unwrap_or(now);
            next.insert(id, at);
        }
        record.at = next;
        if let Some(parent) = path.parent() {
            fs_err::create_dir_all(parent)?;
        }
        let text = serde_json::to_string(&record).context("serialising first-seen")?;
        relic_core::fs::write_atomic(path, &text).with_context(|| format!("writing {path}"))?;
        Ok(record)
    }

    /// Notices that have outstayed the horizon.
    pub fn overstayed(&self, notices: &[Notice], now: Timestamp) -> Vec<(String, i64)> {
        notices
            .iter()
            .filter_map(|notice| {
                let at = self.at.get(&notice.identity())?;
                let days = relic_core::fmt::age_days(*at, now);
                (days > HORIZON_DAYS).then(|| (notice.source.clone(), days))
            })
            .collect()
    }
}

/// The last measured tick.
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Timing {
    /// How long it took.
    pub micros: u64,
    /// When it was measured.
    pub at: Timestamp,
}

impl Timing {
    /// Read it, treating anything wrong with it as absent.
    pub fn load(path: &Utf8Path) -> Option<Self> {
        let text = fs_err::read_to_string(path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Record a measurement, if this one is worth recording.
    ///
    /// A write on every prompt would be the very cost this measures. An
    /// over-budget tick is always written, because that is the one worth seeing.
    pub fn record(path: &Utf8Path, micros: u64, now: Timestamp) {
        let previous = Self::load(path);
        let due = previous.is_none_or(|previous| {
            micros > TICK_BUDGET_MICROS
                || now.as_second().saturating_sub(previous.at.as_second()) > TIMING_REFRESH_SECONDS
        });
        if !due {
            return;
        }
        let record = Self { micros, at: now };
        let Ok(text) = serde_json::to_string(&record) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = fs_err::create_dir_all(parent);
        }
        let _ = relic_core::fs::write_atomic(path, &text);
    }
}

#[cfg(test)]
mod tests {
    use super::{FirstSeen, HORIZON_DAYS, Timing};
    use crate::notice::Notice;
    use camino::Utf8PathBuf;
    use jiff::Timestamp;
    use relic_core::finding::{Severity, StationId, Summary};

    fn notice(source: &str, summary: &str) -> Notice {
        Notice {
            source: source.to_owned(),
            finding: StationId::from_static("t").finds(Severity::Soft, Summary::lossy(summary)),
        }
    }

    fn at(second: i64) -> Timestamp {
        Timestamp::from_second(second).unwrap()
    }

    fn temp() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("first-seen.json")).unwrap();
        (dir, path)
    }

    #[test]
    fn a_notice_keeps_the_instant_it_first_arrived() {
        let (_dir, path) = temp();
        let one = vec![notice("up", "3d")];
        FirstSeen::update(&path, &one, at(1000)).unwrap();
        let record = FirstSeen::update(&path, &one, at(9000)).unwrap();
        assert_eq!(record.at.values().next(), Some(&at(1000)));
    }

    #[test]
    fn a_notice_that_went_away_is_forgotten_rather_than_logged() {
        let (_dir, path) = temp();
        FirstSeen::update(&path, &[notice("up", "3d")], at(1000)).unwrap();
        let record = FirstSeen::update(&path, &[], at(2000)).unwrap();
        assert!(record.at.is_empty());
    }

    #[test]
    fn reworded_is_new_so_its_clock_starts_again() {
        let (_dir, path) = temp();
        FirstSeen::update(&path, &[notice("up", "3d")], at(1000)).unwrap();
        let record = FirstSeen::update(&path, &[notice("up", "4d")], at(90_000)).unwrap();
        assert_eq!(record.at.values().next(), Some(&at(90_000)));
    }

    #[test]
    fn a_notice_past_the_horizon_is_furniture() {
        let (_dir, path) = temp();
        let one = vec![notice("up", "3d")];
        let start = at(1_000_000);
        FirstSeen::update(&path, &one, start).unwrap();
        let record = FirstSeen::load(&path);
        let later = at(start.as_second() + (HORIZON_DAYS + 1) * 86_400);
        assert_eq!(record.overstayed(&one, later).len(), 1);
        assert!(record.overstayed(&one, start).is_empty());
    }

    #[test]
    fn an_in_budget_tick_is_not_rewritten_every_prompt() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("timing.json")).unwrap();
        Timing::record(&path, 900, at(1000));
        Timing::record(&path, 1200, at(1001));
        assert_eq!(Timing::load(&path).unwrap().micros, 900);
    }

    #[test]
    fn an_over_budget_tick_is_always_written() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("timing.json")).unwrap();
        Timing::record(&path, 900, at(1000));
        Timing::record(&path, 90_000, at(1001));
        assert_eq!(Timing::load(&path).unwrap().micros, 90_000);
    }
}
