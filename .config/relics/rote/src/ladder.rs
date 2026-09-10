//! The expanding-interval schedule, and everything that reads off it.
//!
//! Shape from Bonneau and Schechter (USENIX Security 2014), who held 56-bit
//! secrets at roughly 88% unaided recall on a ladder of this shape.
//!
//! The ladder decides **classification**, never permission. `rote` records what
//! a person actually did and turns it into a measurement; it does not refuse an
//! entry to protect the shape of its own schedule. The one place it is strict is
//! [`gate`], which is a computed verdict rather than a gate on anything.

use jiff::Span;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

/// Days between reviews, by step. The last element is the cap, held forever.
pub const LADDER: [u32; 5] = [1, 1, 2, 4, 7];

/// First-attempt passes at the cap interval that the cutover gate wants.
pub const GATE_PASSES: u32 = 3;

/// An entry whose effective interval reaches this multiple of the scheduled one
/// is long-horizon evidence, however it came about — a deliberate probe or a
/// fortnight away from the machine.
pub const STRETCH_MULTIPLE: u32 = 2;

/// A position on the ladder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Step(u8);

impl Step {
    /// Where a new slug starts, and where a lapse returns it to.
    pub const FIRST: Self = Self(0);

    /// The top of the ladder.
    pub fn cap() -> Self {
        Self(u8::try_from(LADDER.len().saturating_sub(1)).unwrap_or(u8::MAX))
    }

    /// The step recorded in a log line, clamped into the ladder.
    pub fn from_recorded(step: u8) -> Self {
        Self(step.min(Self::cap().0))
    }

    /// Days until the next review from this position.
    pub fn interval(self) -> u32 {
        LADDER
            .get(usize::from(self.0))
            .copied()
            .unwrap_or(*LADDER.last().unwrap_or(&1))
    }

    /// The next position after a first-attempt pass.
    pub fn advanced(self) -> Self {
        Self(self.0.saturating_add(1).min(Self::cap().0))
    }

    /// Whether this position is the top of the ladder.
    pub fn at_cap(self) -> bool {
        self.0 >= Self::cap().0
    }

    /// The position as a number, for rendering and for the log.
    pub fn get(self) -> u8 {
        self.0
    }
}

/// The interval held at the top of the ladder.
pub fn cap_days() -> u32 {
    Step::cap().interval()
}

/// What an entry on a slug counts as.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Class {
    /// Due, and therefore scores: it advances or resets the step.
    Review,
    /// Not due. Recorded in full, and touches the step in neither direction.
    Practice,
    /// The far end of a deliberate stretch horizon. Long-horizon evidence, and
    /// it leaves the step alone: the horizon was chosen, so failing it is a
    /// reading rather than a lapse in the schedule.
    Probe,
}

impl Class {
    /// Whether an entry of this class moves the step.
    pub fn scores(self) -> bool {
        match self {
            Self::Review => true,
            Self::Practice | Self::Probe => false,
        }
    }
}

/// Where a slug stands against its own schedule today.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// Due now, as an ordinary review.
    Due,
    /// The far end of a stretch horizon has arrived.
    Probe,
    /// Held out of the reminder until a chosen date.
    Held {
        /// When the horizon ends.
        until: Date,
    },
    /// Not due yet.
    Waiting {
        /// When it next falls due.
        until: Date,
    },
}

/// Where a slug stands, given its anchor day, its step, and any stretch horizon.
///
/// `anchor` is the day of the last scheduled review, or of enrolment when there
/// has been none.
pub fn standing(today: Date, anchor: Date, step: Step, hold_until: Option<Date>) -> Standing {
    if let Some(until) = hold_until {
        return if today >= until {
            Standing::Probe
        } else {
            Standing::Held { until }
        };
    }
    let due = due_day(anchor, step);
    if today >= due {
        Standing::Due
    } else {
        Standing::Waiting { until: due }
    }
}

/// The day a slug next falls due.
///
/// Saturates at `anchor` if the date arithmetic overflows, which puts the slug
/// due now — the safe direction for a drill.
pub fn due_day(anchor: Date, step: Step) -> Date {
    let days = i64::from(step.interval());
    anchor.checked_add(Span::new().days(days)).unwrap_or(anchor)
}

/// Whole days between two drill days, clamped at zero.
pub fn days_between(earlier: Date, later: Date) -> u32 {
    u32::try_from((later - earlier).get_days()).unwrap_or(0)
}

/// Whether an interval is long enough to read as long-horizon evidence.
pub fn is_stretch(scheduled: u32, effective: u32) -> bool {
    effective >= scheduled.saturating_mul(STRETCH_MULTIPLE)
}

/// The cutover gate: *do not change a lock until the new key has survived
/// spacing.*
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Gate {
    /// At the cap with enough consecutive cold passes at the cap interval.
    Ready {
        /// How many, so the reading carries its own evidence.
        passes: u32,
    },
    /// At the cap, still accumulating.
    Climbing {
        /// How many so far, against [`GATE_PASSES`].
        passes: u32,
    },
    /// Not at the cap yet, so the question does not arise.
    Below,
}

/// Read the gate off a slug's position and its run of cold passes at the cap.
///
/// `cap_passes` counts consecutive first-attempt passes whose **effective**
/// interval reached the cap — the interval since the previous entry of any
/// kind, so practising a slug daily cannot dress a one-day recall up as a
/// seven-day one.
pub fn gate(step: Step, cap_passes: u32) -> Gate {
    if !step.at_cap() {
        Gate::Below
    } else if cap_passes >= GATE_PASSES {
        Gate::Ready { passes: cap_passes }
    } else {
        Gate::Climbing { passes: cap_passes }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::{
        Class, GATE_PASSES, Gate, LADDER, Standing, Step, days_between, due_day, gate, is_stretch,
        standing,
    };

    #[test]
    fn the_ladder_climbs_and_then_holds() {
        let mut step = Step::FIRST;
        let mut seen = vec![step.interval()];
        for _ in 0..8 {
            step = step.advanced();
            seen.push(step.interval());
        }
        assert_eq!(seen, vec![1, 1, 2, 4, 7, 7, 7, 7, 7]);
        assert!(step.at_cap());
    }

    #[test]
    fn a_recorded_step_past_the_ladder_clamps_rather_than_panicking() {
        assert_eq!(Step::from_recorded(200), Step::cap());
        assert_eq!(Step::from_recorded(1).interval(), LADDER[1]);
    }

    #[test]
    fn a_new_slug_falls_due_the_day_after_enrolment() {
        let added = date(2026, 9, 10);
        assert_eq!(due_day(added, Step::FIRST), date(2026, 9, 11));
        assert_eq!(
            standing(added, added, Step::FIRST, None),
            Standing::Waiting {
                until: date(2026, 9, 11)
            }
        );
        assert_eq!(
            standing(date(2026, 9, 11), added, Step::FIRST, None),
            Standing::Due
        );
    }

    #[test]
    fn a_long_absence_leaves_the_slug_due_rather_than_confused() {
        let anchor = date(2026, 9, 1);
        assert_eq!(
            standing(date(2026, 9, 29), anchor, Step::cap(), None),
            Standing::Due
        );
    }

    #[test]
    fn a_horizon_holds_the_slug_and_then_hands_over_a_probe() {
        let anchor = date(2026, 9, 1);
        let until = date(2026, 10, 16);
        assert_eq!(
            standing(date(2026, 9, 20), anchor, Step::cap(), Some(until)),
            Standing::Held { until }
        );
        assert_eq!(
            standing(until, anchor, Step::cap(), Some(until)),
            Standing::Probe
        );
    }

    #[test]
    fn only_a_review_moves_the_step() {
        assert!(Class::Review.scores());
        assert!(!Class::Practice.scores());
        assert!(!Class::Probe.scores());
    }

    #[test]
    fn days_between_clamps_a_backwards_pair_rather_than_wrapping() {
        assert_eq!(days_between(date(2026, 9, 1), date(2026, 9, 8)), 7);
        assert_eq!(days_between(date(2026, 9, 8), date(2026, 9, 1)), 0);
    }

    #[test]
    fn a_stretch_is_twice_the_schedule_or_more() {
        assert!(!is_stretch(7, 7));
        assert!(!is_stretch(7, 13));
        assert!(is_stretch(7, 14));
        assert!(is_stretch(1, 2));
    }

    #[test]
    fn the_gate_reads_off_the_cap_and_the_run_of_cold_passes() {
        assert_eq!(gate(Step::FIRST, 99), Gate::Below);
        assert_eq!(
            gate(Step::cap(), GATE_PASSES - 1),
            Gate::Climbing {
                passes: GATE_PASSES - 1
            }
        );
        assert_eq!(
            gate(Step::cap(), GATE_PASSES),
            Gate::Ready {
                passes: GATE_PASSES
            }
        );
    }
}
