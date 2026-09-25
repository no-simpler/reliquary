//! The one record that lets `doctor` say something the card cannot: when each
//! outstanding notice was first seen.

use std::collections::BTreeMap;

use anyhow::{Context, Result};
use camino::Utf8Path;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::notice::Notice;

/// How long a notice may stand before `doctor` suggests it may be furniture.
pub const HORIZON_DAYS: i64 = 14;

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

#[cfg(test)]
mod tests {
    use super::{FirstSeen, HORIZON_DAYS};
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
}
