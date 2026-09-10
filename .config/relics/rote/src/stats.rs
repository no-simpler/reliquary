//! The measurement.
//!
//! Everything here is windowed and bucketed rather than assuming a cadence,
//! because the cadence is a human's and will not hold. The load-bearing field
//! is `effective_interval_days` — days since the previous entry of **any**
//! kind — which is what keeps both directions of irregularity honest: extra
//! practice cannot dress a one-day recall up as a seven-day one, and a fortnight
//! away arrives as long-horizon evidence rather than as noise.
//!
//! `rote stats` doubles as a canary for neurological change, which is why
//! latency is reported at all: it rises before the first failure.

use jiff::civil::Date;

use crate::ladder::{self, Class};
use crate::log::{Attempted, Event, Line, Outcome};
use crate::slug::Slug;

/// Entries older than this are out of the headline figures.
pub const WINDOW_DAYS: u32 = 90;

/// How many entries each half of the latency comparison wants before it will
/// say anything at all.
pub const TREND_WINDOW: usize = 10;

/// The fewest entries in a half that still supports a reading.
pub const TREND_FLOOR: usize = 6;

/// A pass rate, carrying its own sample size so a percentage is never read
/// without one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Retention {
    /// First-attempt passes.
    pub passes: u32,
    /// First attempts.
    pub total: u32,
}

impl Retention {
    fn record(&mut self, passed: bool) {
        self.total = self.total.saturating_add(1);
        if passed {
            self.passes = self.passes.saturating_add(1);
        }
    }

    /// The rate as a percentage, when there is anything to divide.
    pub fn percent(self) -> Option<f64> {
        (self.total > 0).then(|| f64::from(self.passes) * 100.0 / f64::from(self.total))
    }
}

/// One band of effective interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Bucket {
    /// How the band is spelled.
    pub label: &'static str,
    /// Pass rate within it.
    pub retention: Retention,
}

/// The bands, in order. The tail is what irregular invocation populates, and it
/// is the only source of decay data at this artifact's horizon.
const BANDS: [(&str, u32, u32); 5] = [
    ("1d", 0, 1),
    ("2-4d", 2, 4),
    ("5-8d", 5, 8),
    ("9-30d", 9, 30),
    ("31d+", 31, u32::MAX),
];

/// Which way latency is moving for a slug.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trend {
    /// Not enough entries on one side to say. Reported as such, never as
    /// "steady".
    Unknown,
    /// Within noise.
    Steady,
    /// Slower than it was, by this many percent.
    Rising(u32),
    /// Faster than it was, by this many percent.
    Falling(u32),
}

/// A change smaller than this reads as noise rather than as a signal.
pub const TREND_NOISE_PERCENT: u32 = 20;

/// What the log says about one slug's timings.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlugStats {
    /// The slug.
    pub slug: Slug,
    /// Pass rate over scheduled reviews.
    pub retention: Retention,
    /// Median milliseconds from prompt to first keystroke.
    pub ttfk_ms: Option<u64>,
    /// Median milliseconds from first keystroke to submission.
    pub total_ms: Option<u64>,
    /// Which way time-to-first-keystroke is moving.
    pub trend: Trend,
    /// Recent time-to-first-keystroke, oldest first, for a sparkline.
    pub recent_ttfk: Vec<u64>,
    /// First-attempt aided entries in the window. Not recall, so it sits beside
    /// the retention figure rather than inside it.
    pub aided: u32,
}

/// A first-attempt failure on an entry that scored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lapse {
    /// The slug.
    pub slug: Slug,
    /// The drill day.
    pub day: Date,
    /// What the ladder had asked for.
    pub scheduled: u32,
    /// What it actually got.
    pub effective: Option<u32>,
}

impl Lapse {
    /// Whether the failure came after a longer gap than the schedule asked for.
    /// A lapse beyond schedule is expected decay; one at or below it is not.
    pub fn beyond_schedule(&self) -> bool {
        self.effective.is_some_and(|days| days > self.scheduled)
    }
}

/// How close to due reviews were actually taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Punctuality {
    /// Reviews taken within a day of falling due.
    pub on_time: u32,
    /// Reviews taken.
    pub total: u32,
    /// Median days late, over the same set.
    pub median_lateness: Option<u64>,
}

/// The whole reading.
#[derive(Clone, Debug)]
pub struct Stats {
    /// The window the headline figures cover.
    pub window_days: u32,
    /// Pass rate over scheduled reviews. Anki calls this true retention, and
    /// keeping practice out of it is the point.
    pub retention: Retention,
    /// Pass rate over practice, reported separately so it can be compared
    /// rather than confused.
    pub practice: Retention,
    /// Aided entries, and how many of them the verifier accepted. A refusal here
    /// is not a memory reading at all: it means what the vault holds and what
    /// the verifier expects have diverged.
    pub aided: Retention,
    /// Retention by effective interval.
    pub buckets: Vec<Bucket>,
    /// Per slug.
    pub slugs: Vec<SlugStats>,
    /// Every first-attempt failure that scored, most recent first.
    pub lapses: Vec<Lapse>,
    /// How late reviews ran.
    pub punctuality: Punctuality,
}

impl Stats {
    /// Read the log.
    pub fn gather<'a>(lines: impl IntoIterator<Item = &'a Line>, today: Date, window: u32) -> Self {
        let entries: Vec<(Date, &Attempted)> = lines
            .into_iter()
            .filter_map(Line::record)
            .filter_map(|record| match &record.event {
                Event::Attempt(entry) => Some((record.day, entry)),
                Event::Add(_) | Event::Rekey(_) | Event::Retire(_) | Event::Probe(_) => None,
            })
            .collect();

        let in_window: Vec<(Date, &Attempted)> = entries
            .iter()
            .copied()
            .filter(|(day, _)| ladder::days_between(*day, today) <= window)
            .collect();

        let mut stats = Self {
            window_days: window,
            retention: Retention::default(),
            practice: Retention::default(),
            aided: Retention::default(),
            buckets: BANDS
                .iter()
                .map(|(label, _, _)| Bucket {
                    label,
                    retention: Retention::default(),
                })
                .collect(),
            slugs: Vec::new(),
            lapses: Vec::new(),
            punctuality: Punctuality::default(),
        };

        let mut lateness: Vec<u64> = Vec::new();
        for (day, entry) in &in_window {
            // An aided entry is not a first attempt at anything — it follows a
            // miss — so it never reaches the first-attempt filter below.
            if !entry.class.unaided() {
                if let Some(accepted) = aided_verdict(entry) {
                    stats.aided.record(accepted);
                }
                continue;
            }
            let Some(passed) = scored(entry) else {
                continue;
            };
            match entry.class {
                Class::Review => {
                    stats.retention.record(passed);
                    if let Some(actual) = entry.actual_interval_days {
                        let late = actual.saturating_sub(entry.scheduled_interval_days);
                        lateness.push(u64::from(late));
                        stats.punctuality.total = stats.punctuality.total.saturating_add(1);
                        if late <= 1 {
                            stats.punctuality.on_time = stats.punctuality.on_time.saturating_add(1);
                        }
                    }
                }
                Class::Practice => stats.practice.record(passed),
                Class::Probe | Class::Aided => {}
            }
            // A probe is a cold reading at a chosen horizon, so it belongs in
            // the bands: it is the only source of decay data out at that
            // distance, which is the whole reason for taking one.
            if (entry.class.scores() || entry.class == Class::Probe)
                && let Some(bucket) = stats.bucket_for(entry.effective_interval_days)
            {
                bucket.retention.record(passed);
            }
            // It is not a lapse, though. A lapse is the ladder going back to the
            // foot, and a probe moves the ladder in neither direction — the
            // horizon was asked for, so failing it is a reading rather than a
            // slip in the schedule.
            if entry.class.scores() && !passed {
                stats.lapses.push(Lapse {
                    slug: entry.slug.clone(),
                    day: *day,
                    scheduled: entry.scheduled_interval_days,
                    effective: entry.effective_interval_days,
                });
            }
        }
        stats.punctuality.median_lateness = median(&mut lateness);
        stats.lapses.reverse();
        stats.slugs = per_slug(&entries, &in_window);
        stats
    }

    fn bucket_for(&mut self, effective: Option<u32>) -> Option<&mut Bucket> {
        let days = effective?;
        let index = BANDS
            .iter()
            .position(|(_, lo, hi)| days >= *lo && days <= *hi)?;
        self.buckets.get_mut(index)
    }
}

/// What an aided entry says about the vault and the verifier: accepted, refused,
/// or nothing. A blank offered no answer, so it is evidence of no disagreement
/// and is not counted as one.
fn aided_verdict(entry: &Attempted) -> Option<bool> {
    match entry.outcome {
        Outcome::Pass => Some(true),
        Outcome::Fail => Some(false),
        Outcome::Blank | Outcome::Skip | Outcome::Abort => None,
    }
}

/// Whether an entry counts toward a rate, and whether it passed.
///
/// Only the first try scores: a second entry in the same sitting is primed by
/// the first and measures transcription rather than recall. A skip or an abort
/// put nothing in front of anyone. A blank did: conceding is a failure of
/// recall, not an absence of one.
///
/// This says nothing about class. An aided entry has an outcome like any other,
/// and every caller that claims to measure recall filters it out first.
fn scored(entry: &Attempted) -> Option<bool> {
    if entry.attempt != 1 {
        return None;
    }
    match entry.outcome {
        Outcome::Pass => Some(true),
        Outcome::Fail | Outcome::Blank => Some(false),
        Outcome::Skip | Outcome::Abort => None,
    }
}

fn per_slug(all: &[(Date, &Attempted)], in_window: &[(Date, &Attempted)]) -> Vec<SlugStats> {
    let mut slugs: Vec<Slug> = all.iter().map(|(_, entry)| entry.slug.clone()).collect();
    slugs.sort();
    slugs.dedup();

    slugs
        .into_iter()
        .map(|slug| {
            let mut retention = Retention::default();
            for (_, entry) in in_window.iter().filter(|(_, e)| e.slug == slug) {
                if entry.class == Class::Review
                    && let Some(passed) = scored(entry)
                {
                    retention.record(passed);
                }
            }

            // Latency is a claim about recall, so an aided entry — which
            // measures transcription — never enters the series.
            let firsts: Vec<&Attempted> = all
                .iter()
                .filter(|(_, entry)| {
                    entry.slug == slug && entry.class.unaided() && scored(entry).is_some()
                })
                .map(|(_, entry)| *entry)
                .collect();

            let aided = u32::try_from(
                in_window
                    .iter()
                    .filter(|(_, entry)| {
                        entry.slug == slug
                            && !entry.class.unaided()
                            && aided_verdict(entry).is_some()
                    })
                    .count(),
            )
            .unwrap_or(u32::MAX);
            let mut ttfk: Vec<u64> = firsts.iter().filter_map(|e| e.ttfk_ms).collect();
            let mut total: Vec<u64> = firsts.iter().filter_map(|e| e.total_ms).collect();
            let recent: Vec<u64> = ttfk
                .iter()
                .rev()
                .take(TREND_WINDOW)
                .rev()
                .copied()
                .collect();

            SlugStats {
                slug,
                retention,
                trend: trend(&ttfk),
                ttfk_ms: median(&mut ttfk),
                total_ms: median(&mut total),
                recent_ttfk: recent,
                aided,
            }
        })
        .collect()
}

/// Compare the last window of timings against the one before it.
///
/// Fewer than [`TREND_FLOOR`] entries either side is [`Trend::Unknown`] rather
/// than "steady": not knowing is not the same as knowing nothing changed.
fn trend(ttfk: &[u64]) -> Trend {
    let count = ttfk.len();
    if count < TREND_FLOOR.saturating_mul(2) {
        return Trend::Unknown;
    }
    let split = count.saturating_sub(TREND_WINDOW.min(count / 2));
    let (before, after) = ttfk.split_at(split.max(1));
    if before.len() < TREND_FLOOR || after.len() < TREND_FLOOR {
        return Trend::Unknown;
    }
    let mut before = before.to_vec();
    let mut after = after.to_vec();
    let (Some(was), Some(now)) = (median(&mut before), median(&mut after)) else {
        return Trend::Unknown;
    };
    if was == 0 {
        return Trend::Unknown;
    }
    let change = (i128::from(now) - i128::from(was)) * 100 / i128::from(was);
    let magnitude = u32::try_from(change.abs()).unwrap_or(u32::MAX);
    if magnitude < TREND_NOISE_PERCENT {
        Trend::Steady
    } else if change > 0 {
        Trend::Rising(magnitude)
    } else {
        Trend::Falling(magnitude)
    }
}

/// The lower median. Chosen over a mean so that walking away mid-prompt needs no
/// special case: one enormous entry moves a mean and does not move this.
pub fn median(values: &mut [u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    values.get(values.len().saturating_sub(1) / 2).copied()
}

/// Eight levels, oldest first. Empty when there is nothing to draw.
pub fn sparkline(values: &[u64]) -> String {
    const LEVELS: [char; 8] = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let (Some(low), Some(high)) = (values.iter().min(), values.iter().max()) else {
        return String::new();
    };
    let span = high.saturating_sub(*low);
    values
        .iter()
        .map(|value| {
            let level = value
                .saturating_sub(*low)
                .saturating_mul(7)
                .checked_div(span)
                .and_then(|level| usize::try_from(level).ok())
                .unwrap_or(0);
            LEVELS.get(level.min(7)).copied().unwrap_or('▁')
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Retention, Stats, TREND_FLOOR, Trend, median, sparkline};
    use crate::ladder::Class;
    use crate::log::{Attempted, Digest, Event, Line, Outcome, Record, SCHEMA, SessionId};

    struct Entry {
        day: Date,
        slug: &'static str,
        class: Class,
        outcome: Outcome,
        attempt: u8,
        scheduled: u32,
        actual: Option<u32>,
        effective: Option<u32>,
        ttfk: Option<u64>,
    }

    impl Default for Entry {
        fn default() -> Self {
            Self {
                day: date(2026, 9, 10),
                slug: "a",
                class: Class::Review,
                outcome: Outcome::Pass,
                attempt: 1,
                scheduled: 7,
                actual: Some(7),
                effective: Some(7),
                ttfk: Some(1_000),
            }
        }
    }

    fn line(entry: &Entry) -> Line {
        Line::Parsed(Box::new(Record {
            v: SCHEMA,
            at: entry
                .day
                .to_zoned(jiff::tz::TimeZone::UTC)
                .unwrap()
                .timestamp(),
            day: entry.day,
            host: "Mac".to_owned(),
            prev: Digest::GENESIS,
            event: Event::Attempt(Attempted {
                slug: entry.slug.parse().unwrap(),
                session: SessionId::from_bytes([0; 8]),
                version: 1,
                class: entry.class,
                attempt: entry.attempt,
                outcome: entry.outcome,
                ttfk_ms: entry.ttfk,
                total_ms: Some(3_000),
                corrections: 0,
                paste_refused: 0,
                scheduled_interval_days: entry.scheduled,
                actual_interval_days: entry.actual,
                effective_interval_days: entry.effective,
                stretch: false,
                step_before: 4,
                step_after: 4,
            }),
        }))
    }

    fn gather(lines: &[Line]) -> Stats {
        Stats::gather(lines, date(2026, 9, 10), super::WINDOW_DAYS)
    }

    #[test]
    fn an_aided_entry_is_kept_out_of_retention_and_out_of_the_latency_series() {
        let stats = gather(&[
            line(&Entry::default()),
            line(&Entry {
                class: Class::Aided,
                ttfk: Some(50),
                ..Entry::default()
            }),
        ]);
        assert_eq!(
            stats.retention,
            Retention {
                passes: 1,
                total: 1
            }
        );
        assert_eq!(
            stats.aided,
            Retention {
                passes: 1,
                total: 1
            }
        );
        let slug = stats.slugs.first().unwrap();
        assert_eq!(slug.aided, 1);
        assert_eq!(
            slug.ttfk_ms,
            Some(1_000),
            "transcription speed is not recall speed"
        );
        assert!(
            stats
                .buckets
                .iter()
                .all(|bucket| bucket.retention.total <= 1),
            "an aided entry never lands in an interval band"
        );
    }

    #[test]
    fn an_aided_entry_is_counted_though_it_is_never_the_first_try() {
        let stats = gather(&[
            line(&Entry {
                outcome: Outcome::Fail,
                ..Entry::default()
            }),
            line(&Entry {
                class: Class::Aided,
                attempt: 2,
                ..Entry::default()
            }),
        ]);
        assert_eq!(
            stats.aided,
            Retention {
                passes: 1,
                total: 1
            }
        );
        assert_eq!(stats.slugs.first().unwrap().aided, 1);
    }

    #[test]
    fn a_refused_aided_entry_reads_as_a_disagreement_rather_than_a_lapse() {
        let stats = gather(&[line(&Entry {
            class: Class::Aided,
            outcome: Outcome::Fail,
            ..Entry::default()
        })]);
        assert_eq!(
            stats.aided,
            Retention {
                passes: 0,
                total: 1
            }
        );
        assert!(stats.lapses.is_empty());
    }

    #[test]
    fn a_blank_aided_entry_is_neither_accepted_nor_refused() {
        let stats = gather(&[line(&Entry {
            class: Class::Aided,
            outcome: Outcome::Blank,
            attempt: 2,
            ..Entry::default()
        })]);
        assert_eq!(stats.aided, Retention::default());
        assert_eq!(stats.slugs.first().unwrap().aided, 0);
    }

    #[test]
    fn a_blank_counts_against_retention() {
        let stats = gather(&[line(&Entry {
            outcome: Outcome::Blank,
            ttfk: None,
            ..Entry::default()
        })]);
        assert_eq!(
            stats.retention,
            Retention {
                passes: 0,
                total: 1
            }
        );
        assert_eq!(stats.lapses.len(), 1);
    }

    #[test]
    fn practice_is_partitioned_out_of_true_retention() {
        let stats = gather(&[
            line(&Entry::default()),
            line(&Entry {
                outcome: Outcome::Fail,
                class: Class::Practice,
                ..Entry::default()
            }),
        ]);
        assert_eq!(
            stats.retention,
            Retention {
                passes: 1,
                total: 1
            }
        );
        assert_eq!(
            stats.practice,
            Retention {
                passes: 0,
                total: 1
            }
        );
    }

    #[test]
    fn only_the_first_try_counts() {
        let stats = gather(&[
            line(&Entry {
                outcome: Outcome::Fail,
                ..Entry::default()
            }),
            line(&Entry {
                outcome: Outcome::Pass,
                attempt: 2,
                ..Entry::default()
            }),
        ]);
        assert_eq!(stats.retention.total, 1);
        assert_eq!(stats.retention.passes, 0);
    }

    #[test]
    fn a_skip_counts_as_nothing_at_all() {
        let stats = gather(&[line(&Entry {
            outcome: Outcome::Skip,
            ..Entry::default()
        })]);
        assert_eq!(stats.retention.total, 0);
        assert_eq!(stats.retention.percent(), None);
    }

    #[test]
    fn retention_is_banded_by_the_interval_that_actually_elapsed() {
        let stats = gather(&[
            line(&Entry {
                effective: Some(1),
                ..Entry::default()
            }),
            line(&Entry {
                effective: Some(45),
                outcome: Outcome::Fail,
                ..Entry::default()
            }),
        ]);
        let by = |label: &str| {
            stats
                .buckets
                .iter()
                .find(|b| b.label == label)
                .copied()
                .unwrap()
                .retention
        };
        assert_eq!(by("1d").passes, 1);
        assert_eq!(
            by("31d+"),
            Retention {
                passes: 0,
                total: 1
            }
        );
        assert_eq!(by("5-8d").total, 0);
    }

    #[test]
    fn a_probe_lands_in_the_bands_without_touching_true_retention() {
        let stats = gather(&[line(&Entry {
            class: Class::Probe,
            effective: Some(45),
            ..Entry::default()
        })]);
        assert_eq!(stats.retention.total, 0);
        assert!(stats.lapses.is_empty());
        assert_eq!(
            stats
                .buckets
                .iter()
                .find(|b| b.label == "31d+")
                .unwrap()
                .retention
                .total,
            1
        );
    }

    #[test]
    fn a_failed_probe_is_a_reading_rather_than_a_lapse() {
        // rote guide probes: the horizon was chosen, so failing it is not a
        // slip in the schedule. It is still decay data, and still banded.
        let stats = gather(&[line(&Entry {
            class: Class::Probe,
            outcome: Outcome::Fail,
            effective: Some(60),
            ..Entry::default()
        })]);
        assert!(stats.lapses.is_empty(), "{:?}", stats.lapses);
        assert_eq!(
            stats
                .buckets
                .iter()
                .find(|bucket| bucket.label == "31d+")
                .unwrap()
                .retention
                .total,
            1,
            "a failed probe is still the only decay data at that horizon"
        );
    }

    #[test]
    fn a_failed_practice_entry_is_not_a_lapse_either() {
        let stats = gather(&[line(&Entry {
            class: Class::Practice,
            outcome: Outcome::Fail,
            ..Entry::default()
        })]);
        assert!(stats.lapses.is_empty(), "{:?}", stats.lapses);
    }

    #[test]
    fn a_lapse_says_whether_it_came_after_a_longer_gap_than_asked_for() {
        let stats = gather(&[
            line(&Entry {
                outcome: Outcome::Fail,
                effective: Some(20),
                ..Entry::default()
            }),
            line(&Entry {
                outcome: Outcome::Fail,
                effective: Some(3),
                ..Entry::default()
            }),
        ]);
        assert_eq!(stats.lapses.len(), 2);
        assert!(!stats.lapses.first().unwrap().beyond_schedule());
        assert!(stats.lapses.get(1).unwrap().beyond_schedule());
    }

    #[test]
    fn punctuality_reads_off_how_late_a_review_actually_ran() {
        let stats = gather(&[
            line(&Entry {
                actual: Some(7),
                ..Entry::default()
            }),
            line(&Entry {
                actual: Some(8),
                ..Entry::default()
            }),
            line(&Entry {
                actual: Some(21),
                ..Entry::default()
            }),
        ]);
        assert_eq!(stats.punctuality.total, 3);
        assert_eq!(stats.punctuality.on_time, 2);
        assert_eq!(stats.punctuality.median_lateness, Some(1));
    }

    #[test]
    fn entries_outside_the_window_leave_the_headline_alone() {
        let stats = Stats::gather(
            &[line(&Entry {
                day: date(2026, 1, 1),
                ..Entry::default()
            })],
            date(2026, 9, 10),
            90,
        );
        assert_eq!(stats.retention.total, 0);
        assert_eq!(stats.slugs.len(), 1, "but the slug still has timings");
    }

    #[test]
    fn a_short_history_reports_the_trend_as_unknown_rather_than_steady() {
        let lines: Vec<Line> = (0..TREND_FLOOR).map(|_| line(&Entry::default())).collect();
        let stats = gather(&lines);
        assert_eq!(stats.slugs.first().unwrap().trend, Trend::Unknown);
    }

    #[test]
    fn a_slowing_hand_reads_as_rising() {
        let mut lines: Vec<Line> = (0..TREND_FLOOR)
            .map(|_| {
                line(&Entry {
                    ttfk: Some(1_000),
                    ..Entry::default()
                })
            })
            .collect();
        lines.extend((0..TREND_FLOOR).map(|_| {
            line(&Entry {
                ttfk: Some(2_000),
                ..Entry::default()
            })
        }));
        assert_eq!(
            gather(&lines).slugs.first().unwrap().trend,
            Trend::Rising(100)
        );
    }

    #[test]
    fn a_steady_hand_reads_as_steady() {
        let lines: Vec<Line> = (0..TREND_FLOOR * 2)
            .map(|i| {
                line(&Entry {
                    ttfk: Some(1_000 + u64::try_from(i).unwrap() % 3),
                    ..Entry::default()
                })
            })
            .collect();
        assert_eq!(gather(&lines).slugs.first().unwrap().trend, Trend::Steady);
    }

    #[test]
    fn the_median_ignores_the_prompt_someone_walked_away_from() {
        assert_eq!(median(&mut [1, 2, 3, 4, 900_000]), Some(3));
        assert_eq!(median(&mut []), None);
        assert_eq!(median(&mut [5]), Some(5));
        assert_eq!(median(&mut [4, 2]), Some(2));
    }

    #[test]
    fn a_sparkline_spans_the_values_it_is_given() {
        assert_eq!(sparkline(&[]), "");
        assert_eq!(sparkline(&[5, 5, 5]), "▁▁▁");
        assert_eq!(sparkline(&[0, 100]), "▁█");
        assert_eq!(sparkline(&[0, 50, 100]).chars().count(), 3);
    }

    #[test]
    fn every_slug_gets_its_own_row_in_name_order() {
        let stats = gather(&[
            line(&Entry {
                slug: "zeta",
                ..Entry::default()
            }),
            line(&Entry {
                slug: "alpha",
                ..Entry::default()
            }),
            line(&Entry {
                slug: "alpha",
                ..Entry::default()
            }),
        ]);
        let names: Vec<&str> = stats.slugs.iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(names, vec!["alpha", "zeta"]);
        assert_eq!(stats.slugs.first().unwrap().retention.total, 2);
    }
}
