//! The measurement.
//!
//! Everything here is windowed and bucketed rather than assuming a cadence,
//! because the cadence is a human's and will not hold.
//!
//! **Over the derived drills, never over what one writer believed.** The
//! corpus reads each drill's occasion, gap and lateness off merged order at
//! replay, so two machines that drilled the same engram without having seen
//! each other cannot each claim a full interval for one real gap.
//!
//! **Per engram, not per lineage.** A rotation is a different secret and a
//! different memory, so pooling the two into one retention figure is a number
//! about nothing.
//!
//! `rote stats` doubles as a canary for neurological change, which is why
//! latency is reported at all: it rises before the first fail.

use jiff::civil::Date;

use crate::corpus::Corpus;
use crate::corpus::drill::Drill;
use crate::corpus::record::EngramId;
use crate::ladder::{self, Ladder, Occasion};
use crate::slug::Slug;

/// Records older than this are out of the headline figures.
pub const WINDOW_DAYS: u32 = 90;

/// A pass rate, carrying its own sample size so a percentage is never read
/// without one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Retention {
    /// Cold passes.
    pub passes: u32,
    /// Drills.
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

/// One band of interval.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bucket {
    /// How the band is spelled, derived from what it covers.
    pub label: String,
    /// Pass rate within it.
    pub retention: Retention,
}

/// The bands, in order. The tail is what irregular invocation populates, and
/// with the ladder reaching a month the cap itself lands in it.
///
/// Ranges only, and [`band_label`] is the only place a band is spelled: a
/// hand-written label beside a range is a second statement of the same fact,
/// and the first band covers a gap of zero days — a drill taken the same day
/// reported as a one-day interval is the one reading that undercuts the
/// guide's claim about the effective interval being the honest one.
const BANDS: [(u32, u32); 5] = [(0, 1), (2, 4), (5, 8), (9, 30), (31, u32::MAX)];

/// How a band is written, derived from what it covers so the two cannot part.
fn band_label(low: u32, high: u32) -> String {
    if high == u32::MAX {
        format!("{low}d+")
    } else if low == high {
        format!("{low}d")
    } else {
        format!("{low}-{high}d")
    }
}

/// What the record says about one engram.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngramStats {
    /// The engram.
    pub engram: EngramId,
    /// How it is spelled to a person.
    pub label: String,
    /// Its lineage.
    pub slug: Slug,
    /// Drills in the window.
    pub drills: u32,
    /// Cold pass rate in the window.
    pub retention: Retention,
    /// Consecutive cold passes, most recent first, whose gap since the
    /// previous exposure reached the cap interval.
    ///
    /// The one number behind *do not change a lock until the new key has
    /// survived spacing*. It is reported and never judged: what counts as
    /// enough is a person's call, taken with this in front of them.
    pub streak: u32,
    /// Median milliseconds from prompt to the cold capture's first keystroke,
    /// in the window.
    pub ttfk_ms: Option<u64>,
}

/// A cold fail.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fail {
    /// How the engram is spelled to a person.
    pub label: String,
    /// The drill day.
    pub day: Date,
    /// What the ladder had asked for.
    pub scheduled: u32,
    /// What it actually got: the gap since the last exposure, across the
    /// merged corpus.
    pub effective: u32,
    /// Whether a follow-up passed.
    pub recovered: bool,
    /// Whether the answer was looked up.
    pub aided: bool,
}

impl Fail {
    /// Whether the fail came after a longer gap than the schedule asked for.
    /// A fail beyond schedule is expected decay; one at or below it is not.
    pub fn beyond_schedule(&self) -> bool {
        self.effective > self.scheduled
    }
}

/// The whole reading.
#[derive(Clone, Debug)]
pub struct Stats {
    /// The window the headline figures cover.
    pub window_days: u32,
    /// Drills in the window.
    pub drills: u32,
    /// Cold pass rate over every drill in the window.
    pub retention: Retention,
    /// Retention by interval.
    pub buckets: Vec<Bucket>,
    /// Median days a review ran past its due day, over reviews in the window.
    /// `None` when there was no review.
    pub median_lateness_days: Option<u64>,
    /// Every cold fail in the window, most recent first.
    pub fails: Vec<Fail>,
    /// Per engram.
    pub engrams: Vec<EngramStats>,
}

impl Stats {
    /// Read the corpus.
    pub fn gather(corpus: &Corpus, today: Date, window: u32, ladder: &Ladder) -> Self {
        let mut stats = Self {
            window_days: window,
            drills: 0,
            retention: Retention::default(),
            buckets: BANDS
                .iter()
                .map(|(low, high)| Bucket {
                    label: band_label(*low, *high),
                    retention: Retention::default(),
                })
                .collect(),
            median_lateness_days: None,
            fails: Vec::new(),
            engrams: Vec::new(),
        };

        let mut lateness: Vec<u64> = Vec::new();
        for drill in corpus
            .drills()
            .iter()
            .filter(|drill| in_window(drill.day, today, window))
        {
            stats.drills = stats.drills.saturating_add(1);
            stats.retention.record(drill.passed());
            if drill.occasion == Occasion::Review {
                lateness.push(u64::from(
                    drill.since_anchor.saturating_sub(drill.scheduled_days),
                ));
            }
            if !drill.passed() {
                stats.fails.push(Fail {
                    label: corpus.label(&drill.engram),
                    day: drill.day,
                    scheduled: drill.scheduled_days,
                    effective: drill.gap,
                    recovered: drill.recovered,
                    aided: drill.aided,
                });
            }
            if let Some(bucket) = band_of(&mut stats.buckets, drill.gap) {
                bucket.retention.record(drill.passed());
            }
        }
        stats.median_lateness_days = median(&mut lateness);
        stats.fails.reverse();
        stats.engrams = per_engram(corpus, today, window, ladder);
        stats
    }
}

fn in_window(day: Date, today: Date, window: u32) -> bool {
    ladder::days_between(day, today) <= window
}

fn band_of(buckets: &mut [Bucket], days: u32) -> Option<&mut Bucket> {
    let index = BANDS
        .iter()
        .position(|(low, high)| days >= *low && days <= *high)?;
    buckets.get_mut(index)
}

fn per_engram(corpus: &Corpus, today: Date, window: u32, ladder: &Ladder) -> Vec<EngramStats> {
    let mut order: Vec<EngramId> = corpus.drills().iter().map(|drill| drill.engram).collect();
    order.sort();
    order.dedup();

    let mut out: Vec<EngramStats> = order
        .into_iter()
        .filter_map(|engram| {
            let mine: Vec<&Drill> = corpus
                .drills()
                .iter()
                .filter(|drill| drill.engram == engram)
                .collect();
            let recent: Vec<&Drill> = mine
                .iter()
                .copied()
                .filter(|drill| in_window(drill.day, today, window))
                .collect();

            let mut retention = Retention::default();
            for drill in &recent {
                retention.record(drill.passed());
            }
            let mut ttfk: Vec<u64> = recent.iter().filter_map(|drill| drill.ttfk_ms).collect();
            let slug = corpus.owner(&engram)?.slug.clone();

            Some(EngramStats {
                engram,
                label: corpus.label(&engram),
                slug,
                drills: u32::try_from(recent.len()).unwrap_or(u32::MAX),
                retention,
                streak: streak(&mine, ladder),
                ttfk_ms: median(&mut ttfk),
            })
        })
        .collect();
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// Consecutive cold passes at the cap interval or longer, counted back from
/// the most recent drill.
///
/// A pass below the cap is not evidence in either direction and is stepped
/// over; a fail ends the run, recovered or not. The occasion is not consulted:
/// a cold pass a month after the last exposure is the evidence the cutover
/// gate wants whether or not the calendar asked for it. Computed here rather
/// than carried in the projection, because a threshold belongs where the
/// thresholds are — and because a counter maintained across a merge would
/// count one real interval twice.
fn streak(mine: &[&Drill], ladder: &Ladder) -> u32 {
    let cap = ladder.cap_days();
    let mut count = 0u32;
    for drill in mine.iter().rev() {
        if !drill.passed() {
            break;
        }
        if drill.gap >= cap {
            count = count.saturating_add(1);
        }
    }
    count
}

/// The lower median. Chosen over a mean so that one enormous reading moves
/// nothing: the prompt voids what it saw nobody in front of, and this is what
/// covers the walk-away it could not see.
pub fn median(values: &mut [u64]) -> Option<u64> {
    if values.is_empty() {
        return None;
    }
    values.sort_unstable();
    values.get(values.len().saturating_sub(1) / 2).copied()
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Stats, median};
    use crate::corpus::Corpus;
    use crate::corpus::record::{
        Attached, Digest, Drilled, EngramId, Enrolled, Event, Outcome, Record, SCHEMA,
    };
    use crate::ladder::Ladder;

    struct Build {
        records: Vec<Record>,
        engram: EngramId,
    }

    impl Build {
        fn new(day: Date) -> Self {
            let engram = EngramId::mint().unwrap();
            let mut build = Self {
                records: Vec::new(),
                engram,
            };
            build.push(
                day,
                Event::Enroll(Enrolled {
                    slug: "a".parse().unwrap(),
                    engram,
                    critical: false,
                }),
            );
            build
        }

        fn push(&mut self, day: Date, event: Event) {
            self.records.push(Record {
                v: SCHEMA,
                at: day.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp(),
                day,
                host: "Mac".to_owned(),
                prev: Digest::GENESIS,
                event,
            });
        }

        fn drill(&mut self, day: Date, outcome: Outcome) {
            self.drilled(day, outcome, false, false);
        }

        fn drilled(&mut self, day: Date, outcome: Outcome, recovered: bool, aided: bool) {
            let engram = self.engram;
            self.push(
                day,
                Event::Drill(Drilled {
                    engram,
                    outcome,
                    ttfk_ms: Some(1_000),
                    follow_ups: u16::from(recovered),
                    recovered,
                    aided,
                }),
            );
        }

        fn gather(&self, today: Date) -> Stats {
            let ladder = Ladder::default();
            let corpus = Corpus::replay(self.records.iter(), &ladder);
            Stats::gather(&corpus, today, 90, &ladder)
        }
    }

    #[test]
    fn the_gap_is_measured_across_the_corpus() {
        let mut build = Build::new(date(2026, 9, 1));
        build.drill(date(2026, 9, 2), Outcome::Pass);
        build.drill(date(2026, 9, 3), Outcome::Pass);
        let stats = build.gather(date(2026, 9, 3));

        assert_eq!(stats.drills, 2);
        let engram = stats.engrams.first().unwrap();
        assert_eq!(engram.drills, 2);
        assert_eq!(
            engram.streak, 0,
            "a one-day gap is not evidence of thirty-day recall"
        );
        assert_eq!(engram.ttfk_ms, Some(1_000));
        let one_day = stats
            .buckets
            .iter()
            .find(|bucket| bucket.label == "0-1d")
            .unwrap();
        assert_eq!(one_day.retention.total, 2, "both land in the first band");
    }

    #[test]
    fn a_streak_counts_back_from_the_last_drill_and_a_fail_ends_it() {
        let mut build = Build::new(date(2026, 6, 1));
        // Climb to the cap first: the ladder has to be at thirty days for a
        // thirty-day gap to be what the schedule asks.
        for day in [
            date(2026, 6, 2),
            date(2026, 6, 3),
            date(2026, 6, 5),
            date(2026, 6, 9),
            date(2026, 6, 16),
            date(2026, 6, 30),
        ] {
            build.drill(day, Outcome::Pass);
        }
        let mut day = date(2026, 7, 30);
        for _ in 0..2 {
            build.drill(day, Outcome::Pass);
            day = day.checked_add(jiff::Span::new().days(30)).unwrap();
        }
        let streak = build.gather(day).engrams.first().unwrap().streak;
        assert_eq!(streak, 2, "two cold passes at the cap");

        build.drilled(day, Outcome::Fail, true, false);
        let after = build.gather(day).engrams.first().unwrap().streak;
        assert_eq!(after, 0, "a fail ends the run, recovered or not");
    }

    #[test]
    fn a_pass_below_the_cap_neither_adds_to_a_streak_nor_clears_it() {
        let mut build = Build::new(date(2026, 8, 1));
        build.drill(date(2026, 8, 31), Outcome::Pass);
        build.drill(date(2026, 9, 1), Outcome::Pass);
        let stats = build.gather(date(2026, 9, 1));
        assert_eq!(stats.engrams.first().unwrap().streak, 1);
    }

    #[test]
    fn an_attachment_restarts_the_interval_without_ending_a_streak() {
        let mut build = Build::new(date(2026, 8, 1));
        build.drill(date(2026, 8, 31), Outcome::Pass);
        let engram = build.engram;
        build.push(date(2026, 9, 5), Event::Attach(Attached { engram }));
        build.drill(date(2026, 9, 6), Outcome::Pass);
        let stats = build.gather(date(2026, 9, 6));
        assert_eq!(
            stats.engrams.first().unwrap().streak,
            1,
            "the drill after an attachment is one day cold, so it adds nothing"
        );
    }

    #[test]
    fn a_fail_is_reported_with_the_gap_that_preceded_it_and_what_followed() {
        let mut build = Build::new(date(2026, 8, 1));
        build.drilled(date(2026, 8, 20), Outcome::Fail, true, true);
        let stats = build.gather(date(2026, 8, 20));
        assert_eq!(stats.retention.total, 1);
        assert_eq!(stats.retention.passes, 0);
        let fail = stats.fails.first().unwrap();
        assert_eq!(fail.effective, 19);
        assert_eq!(fail.scheduled, 1, "a fresh engram is asked for daily");
        assert!(fail.beyond_schedule());
        assert!(fail.recovered);
        assert!(fail.aided);
        assert_eq!(fail.label, "a@1");
    }

    #[test]
    fn a_recovered_fail_is_still_a_fail_in_every_figure() {
        let mut build = Build::new(date(2026, 9, 1));
        build.drilled(date(2026, 9, 2), Outcome::Fail, true, false);
        let stats = build.gather(date(2026, 9, 2));
        assert_eq!(stats.retention.passes, 0);
        assert_eq!(stats.fails.len(), 1);
        assert_eq!(stats.engrams.first().unwrap().retention.passes, 0);
    }

    #[test]
    fn lateness_is_measured_over_reviews_only() {
        let mut build = Build::new(date(2026, 9, 1));
        let stats = build.gather(date(2026, 9, 1));
        assert_eq!(stats.median_lateness_days, None, "no review, no figure");

        // Due on the second, taken on the fourth: two days late.
        build.drill(date(2026, 9, 4), Outcome::Pass);
        // Rung 1 asks for a day; the same afternoon is a practice.
        build.drill(date(2026, 9, 4), Outcome::Pass);
        let stats = build.gather(date(2026, 9, 4));
        assert_eq!(stats.drills, 2);
        assert_eq!(stats.median_lateness_days, Some(2));
    }

    #[test]
    fn a_rotation_splits_the_measurement_in_two() {
        let mut build = Build::new(date(2026, 9, 1));
        build.drill(date(2026, 9, 2), Outcome::Pass);
        let second = EngramId::mint().unwrap();
        build.push(
            date(2026, 9, 3),
            Event::Rotate(crate::corpus::record::Rotated {
                slug: "a".parse().unwrap(),
                to: second,
            }),
        );
        build.engram = second;
        build.drill(date(2026, 9, 4), Outcome::Fail);

        let stats = build.gather(date(2026, 9, 4));
        assert_eq!(stats.engrams.len(), 2, "one row per engram, never pooled");
        let labels: Vec<&str> = stats
            .engrams
            .iter()
            .map(|engram| engram.label.as_str())
            .collect();
        assert!(labels.contains(&"a@1") && labels.contains(&"a@2"));
    }

    #[test]
    fn the_window_keeps_old_drills_out_of_the_headline_and_in_the_streak() {
        let mut build = Build::new(date(2025, 1, 1));
        for day in [
            date(2025, 1, 2),
            date(2025, 1, 3),
            date(2025, 1, 5),
            date(2025, 1, 9),
            date(2025, 1, 16),
            date(2025, 1, 30),
            date(2025, 3, 1),
        ] {
            build.drill(day, Outcome::Pass);
        }
        let stats = build.gather(date(2026, 9, 1));
        assert_eq!(stats.drills, 0);
        let engram = stats.engrams.first().unwrap();
        assert_eq!(engram.drills, 0);
        assert_eq!(engram.ttfk_ms, None);
        assert_eq!(engram.streak, 1, "the streak reaches past the window");
    }

    #[test]
    fn the_median_is_the_lower_one_and_an_empty_series_has_none() {
        assert_eq!(median(&mut []), None);
        assert_eq!(median(&mut [3, 1, 2]), Some(2));
        assert_eq!(median(&mut [4, 1, 2, 3]), Some(2));
    }

    #[test]
    fn every_band_is_labelled_with_what_it_actually_covers() {
        let labels: Vec<String> = super::BANDS
            .iter()
            .map(|(low, high)| super::band_label(*low, *high))
            .collect();
        assert_eq!(labels, ["0-1d", "2-4d", "5-8d", "9-30d", "31d+"]);
        // A same-day drill is the case the guide's claim about effective
        // intervals turns on, so the first band may never read as a point.
        assert_eq!(super::band_label(0, 1), "0-1d");
    }
}
