//! The measurement.
//!
//! Everything here is windowed and bucketed rather than assuming a cadence,
//! because the cadence is a human's and will not hold.
//!
//! **Computed from merged truth, never from what one writer believed.** The
//! interval a record carries is what the machine that wrote it could see; two
//! machines that drilled the same engram without having seen each other would
//! each claim a full interval for one real gap. So the gap between one drill and
//! the exposure before it is recomputed here, from the merged corpus, and the
//! recorded field stays a witness for `doctor` to compare against.
//!
//! **Per engram, not per lineage.** A rotation is a different secret and a
//! different memory, so pooling the two into one retention figure is a number
//! about nothing. Only what survives a rotation rolls up.
//!
//! `rote stats` doubles as a canary for neurological change, which is why
//! latency is reported at all: it rises before the first failure.

use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::Date;

use crate::corpus::Corpus;
use crate::corpus::drill::{Drill, drills};
use crate::corpus::record::{EngramId, Event, Outcome, Record};
use crate::ladder::{self, Ladder, Occasion};
use crate::slug::Slug;

/// Records older than this are out of the headline figures.
pub const WINDOW_DAYS: u32 = 90;

/// How many readings each half of the latency comparison wants before it will
/// say anything at all.
pub const TREND_WINDOW: usize = 10;

/// The fewest readings in a half that still supports a reading.
pub const TREND_FLOOR: usize = 6;

/// A change smaller than this reads as noise rather than as a signal.
pub const TREND_NOISE_PERCENT: u32 = 20;

/// A pass rate, carrying its own sample size so a percentage is never read
/// without one.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Retention {
    /// First-sample passes.
    pub passes: u32,
    /// First samples.
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

/// Which way latency is moving.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trend {
    /// Not enough readings on one side to say. Reported as such, never as
    /// "steady".
    Unknown,
    /// Within noise.
    Steady,
    /// Slower than it was, by this many percent.
    Rising(u32),
    /// Faster than it was, by this many percent.
    Falling(u32),
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
    /// Aided drills in the window. Not recall, so it sits beside the retention
    /// figure rather than inside it.
    pub aided: u32,
    /// Consecutive unaided first-sample passes, most recent first, whose gap
    /// since the previous exposure reached the cap interval.
    ///
    /// The one number behind *do not change a lock until the new key has
    /// survived spacing*. It is reported and never judged: what counts as
    /// enough is a person's call, taken with this in front of them.
    pub streak: u32,
}

/// What survives a rotation, rolled up over a whole lineage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LineageStats {
    /// The lineage.
    pub slug: Slug,
    /// How many engrams it has held.
    pub engrams: usize,
    /// Drills taken across all of them.
    pub drills: u32,
    /// Aided drills across all of them.
    pub aided: u32,
    /// Lapses across all of them.
    pub lapses: u32,
    /// How late its reviews ran.
    pub punctuality: Punctuality,
}

/// A first-sample failure on a drill the schedule asked for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lapse {
    /// How the engram is spelled to a person.
    pub label: String,
    /// The drill day.
    pub day: Date,
    /// What the ladder had asked for.
    pub scheduled: u32,
    /// What it actually got, measured across the merged corpus.
    pub effective: u32,
}

impl Lapse {
    /// Whether the failure came after a longer gap than the schedule asked for.
    /// A lapse beyond schedule is expected decay; one at or below it is not.
    pub fn beyond_schedule(&self) -> bool {
        self.effective > self.scheduled
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

/// One drill, with the intervals the merged corpus says preceded it.
struct Measured<'a> {
    drill: &'a Drill<'a>,
    /// Days since the engram was last in front of a person by any route. The
    /// effective interval.
    gap: u32,
    /// Days since the schedule was last served, or since the mint. The actual
    /// interval, and what punctuality is measured against.
    since_anchor: u32,
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
    /// Aided drills, and how many of them the verifier accepted. A refusal here
    /// is not a memory reading at all: it means what the vault holds and what
    /// the verifier expects have diverged.
    pub aided: Retention,
    /// Retention by interval.
    pub buckets: Vec<Bucket>,
    /// Per engram.
    pub engrams: Vec<EngramStats>,
    /// What rolls up per lineage.
    pub lineages: Vec<LineageStats>,
    /// Every first-sample failure that measured, most recent first.
    pub lapses: Vec<Lapse>,
    /// How late reviews ran.
    pub punctuality: Punctuality,
}

impl Stats {
    /// Read the corpus.
    pub fn gather(
        records: &[&Record],
        corpus: &Corpus,
        today: Date,
        window: u32,
        ladder: &Ladder,
    ) -> Self {
        let all = drills(records.iter().copied());
        let measured = with_gaps(records, &all);

        let mut stats = Self {
            window_days: window,
            retention: Retention::default(),
            practice: Retention::default(),
            aided: Retention::default(),
            buckets: BANDS
                .iter()
                .map(|(low, high)| Bucket {
                    label: band_label(*low, *high),
                    retention: Retention::default(),
                })
                .collect(),
            engrams: Vec::new(),
            lineages: Vec::new(),
            lapses: Vec::new(),
            punctuality: Punctuality::default(),
        };

        let mut lateness: Vec<u64> = Vec::new();
        for item in &measured {
            if !in_window(item.drill.day, today, window) {
                continue;
            }
            let Some(first) = item.drill.first() else {
                continue;
            };
            if item.drill.aided {
                if let Some(accepted) = aided_verdict(first.outcome) {
                    stats.aided.record(accepted);
                }
                continue;
            }
            let Some(passed) = scored(first.outcome) else {
                continue;
            };
            match item.drill.occasion {
                Occasion::Review => {
                    stats.retention.record(passed);
                    let late = item
                        .since_anchor
                        .saturating_sub(first.scheduled_interval_days);
                    lateness.push(u64::from(late));
                    stats.punctuality.total = stats.punctuality.total.saturating_add(1);
                    if late <= 1 {
                        stats.punctuality.on_time = stats.punctuality.on_time.saturating_add(1);
                    }
                    if !passed {
                        stats.lapses.push(Lapse {
                            label: corpus.label(&item.drill.engram),
                            day: item.drill.day,
                            scheduled: first.scheduled_interval_days,
                            effective: item.gap,
                        });
                    }
                }
                Occasion::Practice => stats.practice.record(passed),
            }
            if let Some(bucket) = band_of(&mut stats.buckets, item.gap) {
                bucket.retention.record(passed);
            }
        }
        stats.punctuality.median_lateness = median(&mut lateness);
        stats.lapses.reverse();
        stats.engrams = per_engram(&measured, corpus, today, window, ladder);
        stats.lineages = per_lineage(&measured, corpus, today, window);
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

/// Walk the merged corpus once, pairing each drill with the gap since that
/// engram was last in front of a person by any route, and with the days since
/// the schedule was last served.
///
/// Every date here only ever moves forward, the same rule the projection
/// holds: a second machine in another zone can hand the merge a day that
/// precedes its predecessor's.
fn with_gaps<'a>(records: &[&'a Record], all: &'a [Drill<'a>]) -> Vec<Measured<'a>> {
    type Key = (crate::corpus::record::SittingId, EngramId);
    let mut exposed: BTreeMap<EngramId, Date> = BTreeMap::new();
    let mut anchor: BTreeMap<EngramId, Date> = BTreeMap::new();
    let mut intervals: BTreeMap<Key, (u32, u32)> = BTreeMap::new();
    let mut open: BTreeSet<Key> = BTreeSet::new();
    let forward = |slot: &mut BTreeMap<EngramId, Date>, engram: EngramId, day: Date| {
        slot.entry(engram)
            .and_modify(|held| *held = (*held).max(day))
            .or_insert(day);
    };

    for record in records {
        match &record.event {
            Event::Enroll(event) => {
                forward(&mut exposed, event.engram, record.day);
                forward(&mut anchor, event.engram, record.day);
            }
            Event::Rotate(event) => {
                forward(&mut exposed, event.to, record.day);
                forward(&mut anchor, event.to, record.day);
            }
            Event::Attach(event) => {
                forward(&mut exposed, event.engram, record.day);
            }
            Event::Retire(_) => {}
            Event::Capture(event) => {
                if !event.outcome.exposed() {
                    continue;
                }
                let key = (event.sitting, event.engram);
                if open.insert(key) {
                    let since = |slot: &BTreeMap<EngramId, Date>| {
                        slot.get(&event.engram)
                            .map_or(0, |last| ladder::days_between(*last, record.day))
                    };
                    intervals.insert(key, (since(&exposed), since(&anchor)));
                }
                forward(&mut exposed, event.engram, record.day);
                if event.occasion.serves_the_schedule() {
                    forward(&mut anchor, event.engram, record.day);
                }
            }
        }
    }

    all.iter()
        .map(|drill| {
            let (gap, since_anchor) = intervals
                .get(&(drill.sitting, drill.engram))
                .copied()
                .unwrap_or((0, 0));
            Measured {
                drill,
                gap,
                since_anchor,
            }
        })
        .collect()
}

/// What an aided drill says about the vault and the verifier: accepted, refused,
/// or nothing. A blank offered no answer, so it is evidence of no disagreement.
fn aided_verdict(outcome: Outcome) -> Option<bool> {
    match outcome {
        Outcome::Pass => Some(true),
        Outcome::Fail => Some(false),
        Outcome::Blank | Outcome::Skip | Outcome::Abort => None,
    }
}

/// Whether a first sample counts toward a rate, and whether it passed.
///
/// A skip or an abort put nothing in front of anyone. A blank did: conceding is
/// a failure of recall, not an absence of one.
fn scored(outcome: Outcome) -> Option<bool> {
    match outcome {
        Outcome::Pass => Some(true),
        Outcome::Fail | Outcome::Blank => Some(false),
        Outcome::Skip | Outcome::Abort => None,
    }
}

fn per_engram(
    measured: &[Measured<'_>],
    corpus: &Corpus,
    today: Date,
    window: u32,
    ladder: &Ladder,
) -> Vec<EngramStats> {
    let mut order: Vec<EngramId> = measured.iter().map(|item| item.drill.engram).collect();
    order.sort();
    order.dedup();

    let mut out: Vec<EngramStats> = order
        .into_iter()
        .filter_map(|engram| {
            let mine: Vec<&Measured<'_>> = measured
                .iter()
                .filter(|item| item.drill.engram == engram)
                .collect();

            let mut retention = Retention::default();
            let mut aided = 0u32;
            for item in mine
                .iter()
                .filter(|item| in_window(item.drill.day, today, window))
            {
                let Some(first) = item.drill.first() else {
                    continue;
                };
                if item.drill.aided {
                    if aided_verdict(first.outcome).is_some() {
                        aided = aided.saturating_add(1);
                    }
                    continue;
                }
                if item.drill.occasion == Occasion::Review
                    && let Some(passed) = scored(first.outcome)
                {
                    retention.record(passed);
                }
            }

            // Latency is a claim about recall, so an aided drill — which
            // measures transcription — never enters the series. The series
            // reaches past the window on purpose: a trend wants history.
            let firsts: Vec<&crate::corpus::record::Captured> = mine
                .iter()
                .filter(|item| !item.drill.aided)
                .filter_map(|item| item.drill.first())
                .filter(|first| scored(first.outcome).is_some())
                .collect();

            let mut ttfk: Vec<u64> = firsts.iter().filter_map(|e| e.ttfk_ms).collect();
            let mut total: Vec<u64> = firsts.iter().filter_map(|e| e.total_ms).collect();
            let recent: Vec<u64> = ttfk
                .iter()
                .rev()
                .take(TREND_WINDOW)
                .rev()
                .copied()
                .collect();

            let slug = mine.first().map(|item| item.drill.slug.clone())?;

            Some(EngramStats {
                engram,
                label: corpus.label(&engram),
                slug,
                retention,
                trend: trend(&ttfk),
                ttfk_ms: median(&mut ttfk),
                total_ms: median(&mut total),
                recent_ttfk: recent,
                aided,
                streak: streak(&mine, ladder),
            })
        })
        .collect();
    out.sort_by(|a, b| a.label.cmp(&b.label));
    out
}

/// Consecutive unaided first-sample passes at the cap interval or longer,
/// counted back from the most recent drill.
///
/// A pass below the cap is not evidence in either direction and is stepped over;
/// a failure ends the run. The occasion is not consulted: a cold pass a month
/// after the last exposure is the evidence the cutover gate wants whether or
/// not the calendar asked for it, and a cold miss is a miss. Computed here
/// rather than carried in the projection, because a threshold belongs where the
/// thresholds are — and because a counter maintained across a merge would count
/// one real interval twice.
fn streak(mine: &[&Measured<'_>], ladder: &Ladder) -> u32 {
    let cap = ladder.cap_days();
    let mut count = 0u32;
    for item in mine.iter().rev() {
        if item.drill.aided {
            continue;
        }
        let Some(first) = item.drill.first() else {
            continue;
        };
        match scored(first.outcome) {
            Some(true) => {
                if item.gap >= cap {
                    count = count.saturating_add(1);
                }
            }
            Some(false) => break,
            None => {}
        }
    }
    count
}

fn per_lineage(
    measured: &[Measured<'_>],
    corpus: &Corpus,
    today: Date,
    window: u32,
) -> Vec<LineageStats> {
    corpus
        .lineages()
        .map(|lineage| {
            let mine: Vec<&Measured<'_>> = measured
                .iter()
                .filter(|item| *item.drill.slug == lineage.slug)
                .filter(|item| in_window(item.drill.day, today, window))
                .collect();

            let mut aided = 0u32;
            let mut lapses = 0u32;
            let mut punctuality = Punctuality::default();
            let mut lateness: Vec<u64> = Vec::new();
            for item in &mine {
                let Some(first) = item.drill.first() else {
                    continue;
                };
                if item.drill.aided {
                    aided = aided.saturating_add(1);
                    continue;
                }
                if item.drill.occasion == Occasion::Review
                    && let Some(passed) = scored(first.outcome)
                {
                    let late = item
                        .since_anchor
                        .saturating_sub(first.scheduled_interval_days);
                    lateness.push(u64::from(late));
                    punctuality.total = punctuality.total.saturating_add(1);
                    if late <= 1 {
                        punctuality.on_time = punctuality.on_time.saturating_add(1);
                    }
                    if !passed {
                        lapses = lapses.saturating_add(1);
                    }
                }
            }
            punctuality.median_lateness = median(&mut lateness);

            LineageStats {
                slug: lineage.slug.clone(),
                engrams: lineage.engrams.len(),
                drills: u32::try_from(mine.len()).unwrap_or(u32::MAX),
                aided,
                lapses,
                punctuality,
            }
        })
        .collect()
}

/// Compare the last window of timings against the one before it.
///
/// Fewer than [`TREND_FLOOR`] readings either side is [`Trend::Unknown`] rather
/// than "steady": not knowing is not the same as knowing nothing changed. The
/// earlier side is the window before the last, not everything before it: a
/// year of history would otherwise swamp a change that started last month.
fn trend(ttfk: &[u64]) -> Trend {
    let count = ttfk.len();
    if count < TREND_FLOOR.saturating_mul(2) {
        return Trend::Unknown;
    }
    let split = count.saturating_sub(TREND_WINDOW.min(count / 2));
    let (earlier, after) = ttfk.split_at(split.max(1));
    let before = earlier
        .get(earlier.len().saturating_sub(TREND_WINDOW)..)
        .unwrap_or(earlier);
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
/// special case: one enormous reading moves a mean and does not move this.
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

    use super::{Stats, Trend, median, sparkline};
    use crate::corpus::Corpus;
    use crate::corpus::record::{
        Attached, Captured, Digest, EngramId, Enrolled, Event, Outcome, Record, SCHEMA, SittingId,
    };
    use crate::ladder::{Ladder, Occasion};

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

        fn drill(&mut self, day: Date, occasion: Occasion, aided: bool, outcome: Outcome) {
            let engram = self.engram;
            self.push(
                day,
                Event::Capture(Captured {
                    slug: "a".parse().unwrap(),
                    engram,
                    sitting: SittingId::mint().unwrap(),
                    ordinal: 1,
                    occasion,
                    aided,
                    outcome,
                    ttfk_ms: Some(1_000),
                    total_ms: Some(3_000),
                    corrections: 0,
                    paste_accepted: 0,
                    paste_refused: 0,
                    scheduled_interval_days: 30,
                    // Deliberately a lie: the writer's belief is a witness, and
                    // stats must not read it.
                    actual_interval_days: 30,
                    effective_interval_days: 30,
                    rung_before: 6,
                    rung_after: 6,
                }),
            );
        }

        fn gather(&self, today: Date) -> Stats {
            let ladder = Ladder::default();
            let borrowed: Vec<&Record> = self.records.iter().collect();
            let corpus = Corpus::replay(borrowed.iter().copied(), &ladder);
            Stats::gather(&borrowed, &corpus, today, 90, &ladder)
        }
    }

    #[test]
    fn the_gap_is_measured_across_the_corpus_and_not_taken_from_the_record() {
        let mut build = Build::new(date(2026, 9, 1));
        // Two drills one day apart, each claiming a thirty-day interval.
        build.drill(date(2026, 9, 2), Occasion::Review, false, Outcome::Pass);
        build.drill(date(2026, 9, 3), Occasion::Review, false, Outcome::Pass);
        let stats = build.gather(date(2026, 9, 3));

        let engram = stats.engrams.first().unwrap();
        assert_eq!(
            engram.streak, 0,
            "a one-day gap is not evidence of thirty-day recall, whatever the record claims"
        );
        let one_day = stats
            .buckets
            .iter()
            .find(|bucket| bucket.label == "0-1d")
            .unwrap();
        assert_eq!(one_day.retention.total, 2, "both land in the first band");
    }

    #[test]
    fn a_streak_counts_back_from_the_last_drill_and_a_failure_ends_it() {
        let mut build = Build::new(date(2026, 7, 2));
        let mut day = date(2026, 8, 2);
        for _ in 0..2 {
            build.drill(day, Occasion::Review, false, Outcome::Pass);
            day = day.checked_add(jiff::Span::new().days(30)).unwrap();
        }
        let streak = build.gather(day).engrams.first().unwrap().streak;
        assert_eq!(streak, 2, "two cold passes at the cap");

        build.drill(day, Occasion::Review, false, Outcome::Fail);
        let after = build.gather(day).engrams.first().unwrap().streak;
        assert_eq!(after, 0, "a lapse ends the run");
    }

    #[test]
    fn a_pass_below_the_cap_neither_adds_to_a_streak_nor_clears_it() {
        let mut build = Build::new(date(2026, 8, 1));
        build.drill(date(2026, 8, 31), Occasion::Review, false, Outcome::Pass);
        build.drill(date(2026, 9, 1), Occasion::Review, false, Outcome::Pass);
        let stats = build.gather(date(2026, 9, 1));
        assert_eq!(stats.engrams.first().unwrap().streak, 1);
    }

    #[test]
    fn an_attachment_restarts_the_interval_without_ending_a_streak() {
        let mut build = Build::new(date(2026, 8, 1));
        build.drill(date(2026, 8, 31), Occasion::Review, false, Outcome::Pass);
        let engram = build.engram;
        build.push(
            date(2026, 9, 5),
            Event::Attach(Attached {
                slug: "a".parse().unwrap(),
                engram,
            }),
        );
        build.drill(date(2026, 9, 6), Occasion::Review, false, Outcome::Pass);
        let stats = build.gather(date(2026, 9, 6));
        assert_eq!(
            stats.engrams.first().unwrap().streak,
            1,
            "the drill after an attachment is one day cold, so it adds nothing"
        );
    }

    #[test]
    fn aided_drills_stay_out_of_every_figure_that_claims_to_measure_recall() {
        let mut build = Build::new(date(2026, 9, 1));
        build.drill(date(2026, 9, 2), Occasion::Review, true, Outcome::Pass);
        build.drill(date(2026, 9, 3), Occasion::Review, true, Outcome::Fail);
        let stats = build.gather(date(2026, 9, 3));
        assert_eq!(stats.retention.total, 0);
        assert_eq!(stats.aided.total, 2);
        assert_eq!(stats.aided.passes, 1);
        assert_eq!(stats.engrams.first().unwrap().aided, 2);
        assert!(stats.lapses.is_empty(), "an aided drill is never a lapse");
    }

    #[test]
    fn practice_is_reported_apart_from_what_the_schedule_asked_for() {
        let mut build = Build::new(date(2026, 9, 1));
        build.drill(date(2026, 9, 2), Occasion::Review, false, Outcome::Pass);
        build.drill(date(2026, 9, 3), Occasion::Practice, false, Outcome::Fail);
        let stats = build.gather(date(2026, 9, 3));
        assert_eq!(stats.retention.total, 1);
        assert_eq!(stats.practice.total, 1);
        assert_eq!(stats.practice.passes, 0);
        assert!(
            stats.lapses.is_empty(),
            "practice moves no schedule, so a miss there is not a lapse"
        );
    }

    #[test]
    fn a_lapse_is_reported_with_the_gap_that_actually_preceded_it() {
        let mut build = Build::new(date(2026, 8, 1));
        build.drill(date(2026, 8, 20), Occasion::Review, false, Outcome::Fail);
        let stats = build.gather(date(2026, 8, 20));
        let lapse = stats.lapses.first().unwrap();
        assert_eq!(lapse.effective, 19);
        assert_eq!(lapse.scheduled, 30);
        assert!(!lapse.beyond_schedule());
        assert_eq!(lapse.label, "a@1");
    }

    #[test]
    fn a_rotation_splits_the_measurement_in_two() {
        let mut build = Build::new(date(2026, 9, 1));
        build.drill(date(2026, 9, 2), Occasion::Review, false, Outcome::Pass);
        let first = build.engram;
        let second = EngramId::mint().unwrap();
        build.push(
            date(2026, 9, 3),
            Event::Rotate(crate::corpus::record::Rotated {
                slug: "a".parse().unwrap(),
                from: first,
                to: second,
                proved: true,
            }),
        );
        build.engram = second;
        build.drill(date(2026, 9, 4), Occasion::Review, false, Outcome::Fail);

        let stats = build.gather(date(2026, 9, 4));
        assert_eq!(stats.engrams.len(), 2, "one row per engram, never pooled");
        let labels: Vec<&str> = stats
            .engrams
            .iter()
            .map(|engram| engram.label.as_str())
            .collect();
        assert!(labels.contains(&"a@1") && labels.contains(&"a@2"));

        let rolled = stats.lineages.first().unwrap();
        assert_eq!(rolled.engrams, 2);
        assert_eq!(rolled.drills, 2);
        assert_eq!(rolled.lapses, 1);
    }

    #[test]
    fn a_trend_says_it_does_not_know_rather_than_saying_steady() {
        let mut build = Build::new(date(2026, 9, 1));
        build.drill(date(2026, 9, 2), Occasion::Review, false, Outcome::Pass);
        let stats = build.gather(date(2026, 9, 2));
        assert_eq!(stats.engrams.first().unwrap().trend, Trend::Unknown);
    }

    #[test]
    fn a_trend_compares_the_last_window_against_the_one_before_it_and_not_all_of_history() {
        // Ten fast readings, then ten slow, then ten fast again. Against the
        // window before it the last ten are faster; against all of history
        // they would read as steady, which would hide the recovery.
        let mut series = vec![1_000u64; 10];
        series.extend(vec![2_000u64; 10]);
        series.extend(vec![1_000u64; 10]);
        assert_eq!(super::trend(&series), Trend::Falling(50));
        let mut rising = vec![1_000u64; 20];
        rising.extend(vec![1_500u64; 10]);
        assert_eq!(super::trend(&rising), Trend::Rising(50));
    }

    #[test]
    fn punctuality_is_measured_across_the_corpus_and_not_taken_from_the_record() {
        // Every seeded record claims a thirty-day actual interval. The merged
        // corpus says the review came a day after the mint, at a scheduled
        // thirty, so it was not late at all.
        let mut build = Build::new(date(2026, 9, 1));
        build.drill(date(2026, 9, 2), Occasion::Review, false, Outcome::Pass);
        let stats = build.gather(date(2026, 9, 2));
        assert_eq!(stats.punctuality.total, 1);
        assert_eq!(stats.punctuality.on_time, 1);
        assert_eq!(stats.punctuality.median_lateness, Some(0));

        // Forty days after the last review, against a scheduled thirty: ten
        // late, whatever the witness says.
        build.drill(date(2026, 10, 12), Occasion::Review, false, Outcome::Pass);
        let stats = build.gather(date(2026, 10, 12));
        assert_eq!(stats.punctuality.on_time, 1);
        assert_eq!(stats.punctuality.total, 2);
        assert_eq!(stats.punctuality.median_lateness, Some(0));
        let rolled = stats.lineages.first().unwrap();
        assert_eq!(rolled.punctuality.total, 2);
        assert_eq!(rolled.punctuality.on_time, 1);
    }

    #[test]
    fn practice_does_not_serve_the_schedule_and_so_does_not_move_the_actual_interval() {
        let mut build = Build::new(date(2026, 8, 1));
        build.drill(date(2026, 8, 31), Occasion::Review, false, Outcome::Pass);
        build.drill(date(2026, 9, 15), Occasion::Practice, false, Outcome::Pass);
        // Thirty-one days after the review, sixteen after the practice.
        build.drill(date(2026, 10, 1), Occasion::Review, false, Outcome::Pass);
        let stats = build.gather(date(2026, 10, 1));
        assert_eq!(
            stats.punctuality.median_lateness,
            Some(0),
            "the review landed a day past its schedule, which is on time"
        );
    }

    #[test]
    fn a_practice_pass_a_month_cold_counts_toward_the_streak_and_a_practice_miss_ends_it() {
        let mut build = Build::new(date(2026, 6, 1));
        build.drill(date(2026, 7, 1), Occasion::Review, false, Outcome::Pass);
        build.drill(date(2026, 8, 1), Occasion::Practice, false, Outcome::Pass);
        assert_eq!(
            build
                .gather(date(2026, 8, 1))
                .engrams
                .first()
                .unwrap()
                .streak,
            2,
            "a cold pass at the cap is evidence whoever asked for it"
        );
        build.drill(date(2026, 8, 2), Occasion::Practice, false, Outcome::Fail);
        assert_eq!(
            build
                .gather(date(2026, 8, 2))
                .engrams
                .first()
                .unwrap()
                .streak,
            0,
            "and a cold miss is a miss"
        );
    }

    #[test]
    fn the_median_is_the_lower_one_and_an_empty_series_has_none() {
        assert_eq!(median(&mut []), None);
        assert_eq!(median(&mut [3, 1, 2]), Some(2));
        assert_eq!(median(&mut [4, 1, 2, 3]), Some(2));
        assert!(sparkline(&[]).is_empty());
        assert_eq!(sparkline(&[1, 2, 3]).chars().count(), 3);
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
