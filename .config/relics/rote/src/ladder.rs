//! The expanding-interval schedule, and the two readings taken off it.
//!
//! Shape from Bonneau and Schechter (USENIX Security 2014), who held 56-bit
//! secrets at roughly 88% unaided recall on a ladder of this kind.
//!
//! **The ladder proposes; it never decides.** It classifies an invocation and
//! says when the next one is wanted. It refuses nothing, permits nothing, and
//! renders no verdict — `rote` records what a person actually did and turns it
//! into a measurement. Every threshold that once produced a verdict now
//! produces a number, in `stats`, for a person to read.
//!
//! The cap is an **evidence-staleness** parameter rather than a memory one: it
//! sets the longest you can be wrong about yourself without finding out. Held
//! at thirty days, the log accumulates thirty-day retention data as a
//! by-product of ordinary use.

use jiff::Span;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

/// Days between reviews, by rung, when the config says nothing.
pub const DEFAULT_LADDER: [u32; 7] = [1, 1, 2, 4, 7, 14, 30];

/// The longest interval a ladder may declare.
pub const MAX_INTERVAL_DAYS: u32 = 3650;

/// The most rungs a ladder may hold. Bounded so a rung always fits a `u8`.
pub const MAX_RUNGS: usize = 64;

/// A position on the ladder.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Rung(u8);

impl Rung {
    /// Where a new engram starts, and where a lapse returns it to.
    pub const FIRST: Self = Self(0);

    /// The position as a number, for rendering and for the log.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// Why a list of days is not a ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum BadLadder {
    /// No intervals at all.
    #[error("a ladder needs at least one interval")]
    Empty,
    /// More rungs than a `u8` position can address.
    #[error("a ladder holds at most {MAX_RUNGS} rungs, and this one holds {0}")]
    TooManyRungs(usize),
    /// A zero-day interval, which would make an engram perpetually due.
    #[error("an interval of zero days would leave an engram always due")]
    Zero,
    /// An interval shorter than the one before it.
    #[error("a ladder never shortens, and {1} days follows {0}")]
    Shortens(u32, u32),
    /// Longer than [`MAX_INTERVAL_DAYS`].
    #[error("an interval of {0} days is longer than the {MAX_INTERVAL_DAYS} this tool schedules")]
    TooLong(u32),
}

/// The schedule: one interval per rung, the last of them held forever.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Ladder(Vec<u32>);

impl Default for Ladder {
    fn default() -> Self {
        Self(DEFAULT_LADDER.to_vec())
    }
}

impl Ladder {
    /// Build a ladder from its intervals.
    ///
    /// # Errors
    ///
    /// When the list is empty, too long, holds a zero, shortens, or declares an
    /// interval past [`MAX_INTERVAL_DAYS`].
    pub fn new(days: Vec<u32>) -> Result<Self, BadLadder> {
        if days.is_empty() {
            return Err(BadLadder::Empty);
        }
        if days.len() > MAX_RUNGS {
            return Err(BadLadder::TooManyRungs(days.len()));
        }
        let mut previous = 0;
        for &day in &days {
            if day == 0 {
                return Err(BadLadder::Zero);
            }
            if day > MAX_INTERVAL_DAYS {
                return Err(BadLadder::TooLong(day));
            }
            if day < previous {
                return Err(BadLadder::Shortens(previous, day));
            }
            previous = day;
        }
        Ok(Self(days))
    }

    /// The intervals, in rung order.
    pub fn days(&self) -> &[u32] {
        &self.0
    }

    /// The interval held at the top.
    pub fn cap_days(&self) -> u32 {
        self.0.last().copied().unwrap_or(1)
    }

    /// The top of the ladder.
    pub fn cap(&self) -> Rung {
        let last = self.0.len().saturating_sub(1);
        Rung(u8::try_from(last).unwrap_or(u8::MAX))
    }

    /// Days until the next review from a position.
    pub fn interval(&self, rung: Rung) -> u32 {
        self.0
            .get(usize::from(rung.get()))
            .copied()
            .unwrap_or_else(|| self.cap_days())
    }

    /// The next position after a first-attempt unaided pass.
    pub fn advanced(&self, rung: Rung) -> Rung {
        Rung(rung.get().saturating_add(1).min(self.cap().get()))
    }

    /// Whether a position is the top.
    pub fn at_cap(&self, rung: Rung) -> bool {
        rung.get() >= self.cap().get()
    }

    /// A position read out of a record, clamped into this ladder.
    ///
    /// A shortened ladder must not leave a record addressing a rung that is no
    /// longer there.
    pub fn recorded(&self, rung: u8) -> Rung {
        Rung(rung.min(self.cap().get()))
    }

    /// The day an engram next falls due.
    ///
    /// Saturates at `anchor` if the arithmetic overflows, which puts the engram
    /// due now — the safe direction for a drill.
    pub fn due_day(&self, anchor: Date, rung: Rung) -> Date {
        let days = i64::from(self.interval(rung));
        anchor.checked_add(Span::new().days(days)).unwrap_or(anchor)
    }

    /// Where an engram stands against its own schedule today.
    pub fn standing(&self, today: Date, anchor: Date, rung: Rung) -> Standing {
        let due = self.due_day(anchor, rung);
        if today >= due {
            Standing::Due
        } else {
            Standing::Waiting { until: due }
        }
    }
}

/// What an invocation counted as for one engram.
///
/// Orthogonal to whether the answer was consulted, which rides beside it as a
/// plain `aided` flag. **The occasion decides whether the schedule was served;
/// aided decides whether the memory was measured.**
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Occasion {
    /// The schedule asked for it.
    Review,
    /// Nobody asked for it.
    Practice,
}

impl Occasion {
    /// The one spelling of this occasion. Every render site reads it here.
    pub fn word(self) -> &'static str {
        match self {
            Self::Review => "review",
            Self::Practice => "practice",
        }
    }

    /// Whether the schedule asked, and so whether the anchor moves.
    pub fn serves_the_schedule(self) -> bool {
        match self {
            Self::Review => true,
            Self::Practice => false,
        }
    }
}

/// Where an engram stands against its own schedule today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// The schedule is asking now.
    Due,
    /// Not yet.
    Waiting {
        /// When it next falls due.
        until: Date,
    },
}

/// Whole days between two drill days, clamped at zero.
pub fn days_between(earlier: Date, later: Date) -> u32 {
    u32::try_from((later - earlier).get_days()).unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::{BadLadder, DEFAULT_LADDER, Ladder, MAX_RUNGS, Occasion, Rung, Standing};

    #[test]
    fn the_ladder_climbs_and_then_holds() {
        let ladder = Ladder::default();
        let mut rung = Rung::FIRST;
        let mut seen = vec![ladder.interval(rung)];
        for _ in 0..8 {
            rung = ladder.advanced(rung);
            seen.push(ladder.interval(rung));
        }
        assert_eq!(seen, vec![1, 1, 2, 4, 7, 14, 30, 30, 30]);
        assert!(ladder.at_cap(rung));
        assert_eq!(ladder.cap_days(), 30);
    }

    #[test]
    fn climbing_to_the_cap_is_the_sum_of_everything_below_it() {
        let climb: u32 = DEFAULT_LADDER.iter().take(DEFAULT_LADDER.len() - 1).sum();
        assert_eq!(climb, 29, "a month of drilling reaches the cap");
    }

    #[test]
    fn a_ladder_refuses_every_shape_that_is_not_one() {
        assert_eq!(Ladder::new(Vec::new()), Err(BadLadder::Empty));
        assert_eq!(Ladder::new(vec![1, 0]), Err(BadLadder::Zero));
        assert_eq!(Ladder::new(vec![7, 2]), Err(BadLadder::Shortens(7, 2)));
        assert_eq!(Ladder::new(vec![1, 9_000]), Err(BadLadder::TooLong(9_000)));
        assert_eq!(
            Ladder::new(vec![1; MAX_RUNGS + 1]),
            Err(BadLadder::TooManyRungs(MAX_RUNGS + 1))
        );
        assert!(Ladder::new(vec![1, 1, 2]).is_ok());
        assert!(Ladder::new(vec![3, 3, 3]).is_ok(), "flat is not shortening");
    }

    #[test]
    fn a_recorded_rung_past_the_ladder_clamps_rather_than_addressing_nothing() {
        let ladder = Ladder::default();
        assert_eq!(ladder.recorded(200), ladder.cap());
        assert_eq!(ladder.recorded(1).get(), 1);

        let short = Ladder::new(vec![1, 2]).unwrap();
        assert_eq!(short.recorded(6), short.cap(), "a shortened ladder clamps");
    }

    #[test]
    fn a_new_engram_falls_due_the_day_after_enrolment() {
        let ladder = Ladder::default();
        let minted = date(2026, 9, 10);
        assert_eq!(ladder.due_day(minted, Rung::FIRST), date(2026, 9, 11));
        assert_eq!(
            ladder.standing(minted, minted, Rung::FIRST),
            Standing::Waiting {
                until: date(2026, 9, 11)
            }
        );
        assert_eq!(
            ladder.standing(date(2026, 9, 11), minted, Rung::FIRST),
            Standing::Due
        );
    }

    #[test]
    fn a_long_absence_leaves_the_engram_due_rather_than_confused() {
        let ladder = Ladder::default();
        assert_eq!(
            ladder.standing(date(2026, 12, 1), date(2026, 9, 1), ladder.cap()),
            Standing::Due
        );
    }

    #[test]
    fn an_occasion_is_spelled_in_exactly_one_place() {
        assert_eq!(Occasion::Review.word(), "review");
        assert_eq!(Occasion::Practice.word(), "practice");
        assert!(Occasion::Review.serves_the_schedule());
        assert!(!Occasion::Practice.serves_the_schedule());
    }

    #[test]
    fn days_between_clamps_a_backwards_pair_rather_than_wrapping() {
        assert_eq!(super::days_between(date(2026, 9, 1), date(2026, 9, 8)), 7);
        assert_eq!(super::days_between(date(2026, 9, 8), date(2026, 9, 1)), 0);
    }
}
