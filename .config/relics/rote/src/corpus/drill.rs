//! The drill: one engram met once in one sitting.
//!
//! Derived rather than stored. The record's grain is the **capture**, because a
//! capture is flushed the moment it is taken and a sitting cut short keeps every
//! reading it had already produced. The drill is the grouping over those,
//! `(sitting, engram)`, and it is what every measurement actually reads.
//!
//! It lives here rather than beside the interactive loop because `stats` needs
//! it and never opens a terminal.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;

use super::record::{Captured, EngramId, Event, Outcome, Record, SittingId};
use crate::ladder::Occasion;
use crate::slug::Slug;

/// One engram met once in one sitting.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drill<'a> {
    /// The lineage, as the record spells it.
    pub slug: &'a Slug,
    /// The engram that was asked for.
    pub engram: EngramId,
    /// The sitting it happened in.
    pub sitting: SittingId,
    /// The drill day.
    pub day: Date,
    /// When the first sample landed.
    pub at: Timestamp,
    /// Whether the schedule asked.
    pub occasion: Occasion,
    /// Whether the answer was consulted at any point. A lookup re-labels the
    /// whole encounter, not only the sample that followed it.
    pub aided: bool,
    /// Every sample, in the order they were typed.
    pub samples: Vec<&'a Captured>,
}

impl<'a> Drill<'a> {
    /// The sample that measures. A second is primed by the first.
    pub fn first(&self) -> Option<&'a Captured> {
        self.samples.first().copied()
    }

    /// How the first sample ended.
    pub fn outcome(&self) -> Option<Outcome> {
        self.first().map(|sample| sample.outcome)
    }

    /// Whether this drill moved the ladder.
    pub fn measured(&self) -> bool {
        self.first().is_some_and(Captured::scores)
    }

    /// Samples beyond the first.
    pub fn retries(&self) -> usize {
        self.samples.len().saturating_sub(1)
    }

    /// Where the engram ended up.
    pub fn landing(&self) -> Option<Landing> {
        self.first().and_then(Landing::of)
    }
}

/// Where one engram ended up in one sitting.
///
/// A separate vocabulary from [`Occasion`] on purpose: the occasion says why the
/// drill happened, the landing says how it went. Neither is spelled twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    /// Answered cold, first try.
    Pass,
    /// Missed, and the ladder went back to the foot.
    Lapse,
    /// Missed, and nothing moved: practice carries no schedule.
    Miss,
    /// Passed over.
    Skipped,
    /// The answer was looked up before anything was typed, so it measured
    /// nothing either way.
    Aided,
    /// A verifier was minted here. Nothing was judged.
    Attached,
}

impl Landing {
    /// Where a first sample leaves its engram, or `None` when the sitting was
    /// abandoned there and the engram got no reading at all.
    ///
    /// Taken apart from the record type so the sitting can read a landing off a
    /// sample it has not written yet, and both get the same answer.
    pub fn from_parts(aided: bool, outcome: Outcome, serves_schedule: bool) -> Option<Self> {
        if aided {
            return Some(Self::Aided);
        }
        Some(match outcome {
            Outcome::Pass => Self::Pass,
            Outcome::Fail | Outcome::Blank => {
                if serves_schedule {
                    Self::Lapse
                } else {
                    Self::Miss
                }
            }
            Outcome::Skip => Self::Skipped,
            Outcome::Abort => return None,
        })
    }

    fn of(sample: &Captured) -> Option<Self> {
        Self::from_parts(
            sample.aided,
            sample.outcome,
            sample.occasion.serves_the_schedule(),
        )
    }

    /// The one spelling of this landing. Every render site reads it here.
    pub fn word(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Lapse => "lapse",
            Self::Miss => "miss",
            Self::Skipped => "skipped",
            Self::Aided => "aided",
            Self::Attached => "attached",
        }
    }

    /// How it reads when it stands alone rather than in a tally.
    pub fn alone(self) -> &'static str {
        match self {
            Self::Pass => "passed",
            Self::Lapse => "lapsed",
            Self::Miss => "missed",
            Self::Skipped => "skipped",
            Self::Aided => "aided",
            Self::Attached => "attached",
        }
    }
}

/// Group every capture in a merged stream into drills, in encounter order.
pub fn drills<'a>(records: impl IntoIterator<Item = &'a Record>) -> Vec<Drill<'a>> {
    let mut out: Vec<Drill<'a>> = Vec::new();
    let mut seen: BTreeMap<(SittingId, EngramId), usize> = BTreeMap::new();
    for record in records {
        let Event::Capture(sample) = &record.event else {
            continue;
        };
        let key = (sample.sitting, sample.engram);
        if let Some(drill) = seen.get(&key).copied().and_then(|at| out.get_mut(at)) {
            drill.aided |= sample.aided;
            drill.samples.push(sample);
        } else {
            seen.insert(key, out.len());
            out.push(Drill {
                slug: &sample.slug,
                engram: sample.engram,
                sitting: sample.sitting,
                day: record.day,
                at: record.at,
                occasion: sample.occasion,
                aided: sample.aided,
                samples: vec![sample],
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Landing, drills};
    use crate::corpus::record::{
        Captured, Digest, EngramId, Event, Outcome, Record, SCHEMA, SittingId,
    };
    use crate::ladder::Occasion;
    use crate::machine::MachineId;

    fn sample(
        day: Date,
        engram: EngramId,
        sitting: SittingId,
        ordinal: u8,
        occasion: Occasion,
        aided: bool,
        outcome: Outcome,
    ) -> Record {
        Record {
            v: SCHEMA,
            at: day.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp(),
            day,
            machine: MachineId::of("test"),
            host: "Mac".to_owned(),
            seq: 0,
            prev: Digest::GENESIS,
            event: Event::Capture(Captured {
                slug: "a".parse().unwrap(),
                engram,
                sitting,
                ordinal,
                occasion,
                aided,
                outcome,
                ttfk_ms: None,
                total_ms: None,
                corrections: 0,
                paste_refused: 0,
                scheduled_interval_days: 7,
                actual_interval_days: 7,
                effective_interval_days: 7,
                rung_before: 0,
                rung_after: 0,
            }),
        }
    }

    #[test]
    fn samples_in_one_sitting_on_one_engram_are_one_drill() {
        let engram = EngramId::mint().unwrap();
        let sitting = SittingId::mint().unwrap();
        let day = date(2026, 9, 10);
        let records = [
            sample(
                day,
                engram,
                sitting,
                1,
                Occasion::Review,
                false,
                Outcome::Fail,
            ),
            sample(
                day,
                engram,
                sitting,
                2,
                Occasion::Review,
                false,
                Outcome::Pass,
            ),
        ];
        let found = drills(records.iter());
        assert_eq!(found.len(), 1);
        let drill = found.first().unwrap();
        assert_eq!(drill.samples.len(), 2);
        assert_eq!(drill.retries(), 1);
        assert_eq!(drill.outcome(), Some(Outcome::Fail), "the first measures");
        assert!(drill.measured());
    }

    #[test]
    fn a_second_sitting_the_same_day_is_a_second_drill() {
        let engram = EngramId::mint().unwrap();
        let day = date(2026, 9, 10);
        let records = [
            sample(
                day,
                engram,
                SittingId::mint().unwrap(),
                1,
                Occasion::Review,
                false,
                Outcome::Pass,
            ),
            sample(
                day,
                engram,
                SittingId::mint().unwrap(),
                1,
                Occasion::Practice,
                false,
                Outcome::Pass,
            ),
        ];
        assert_eq!(drills(records.iter()).len(), 2);
    }

    #[test]
    fn a_lookup_re_labels_the_whole_drill() {
        let engram = EngramId::mint().unwrap();
        let sitting = SittingId::mint().unwrap();
        let day = date(2026, 9, 10);
        let records = [
            sample(
                day,
                engram,
                sitting,
                1,
                Occasion::Review,
                false,
                Outcome::Fail,
            ),
            sample(
                day,
                engram,
                sitting,
                2,
                Occasion::Review,
                true,
                Outcome::Pass,
            ),
        ];
        let found = drills(records.iter());
        let drill = found.first().unwrap();
        assert!(
            drill.aided,
            "the encounter was aided, whatever the first try was"
        );
        assert_eq!(
            drill.landing(),
            Some(Landing::Lapse),
            "and the cold miss it opened with is still a lapse"
        );
    }

    #[test]
    fn a_landing_reads_off_the_first_sample_and_the_buckets_are_disjoint() {
        assert_eq!(
            Landing::from_parts(false, Outcome::Pass, true),
            Some(Landing::Pass)
        );
        assert_eq!(
            Landing::from_parts(false, Outcome::Fail, true),
            Some(Landing::Lapse)
        );
        assert_eq!(
            Landing::from_parts(false, Outcome::Fail, false),
            Some(Landing::Miss),
            "practice moves no schedule, so a miss there is not a lapse"
        );
        assert_eq!(
            Landing::from_parts(true, Outcome::Fail, true),
            Some(Landing::Aided)
        );
        assert_eq!(
            Landing::from_parts(false, Outcome::Skip, true),
            Some(Landing::Skipped)
        );
        assert_eq!(Landing::from_parts(false, Outcome::Abort, true), None);
    }

    #[test]
    fn every_landing_is_spelled_in_exactly_one_place() {
        for landing in [
            Landing::Pass,
            Landing::Lapse,
            Landing::Miss,
            Landing::Skipped,
            Landing::Aided,
            Landing::Attached,
        ] {
            assert!(!landing.word().is_empty());
            assert!(!landing.alone().is_empty());
        }
    }
}
