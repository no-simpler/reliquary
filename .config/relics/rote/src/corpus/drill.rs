//! The drill, as replay reads it.
//!
//! The record stores what a drill measured; what it *was* — a review or a
//! practice, at what rung, after what gap — is derived here from the ladder and
//! the drills before it. Every reading in `stats` is over these, so a change to
//! the ladder re-derives the whole history rather than leaving two schedules in
//! one file.
//!
//! It lives here rather than beside the interactive loop because `stats` needs
//! it and never opens a terminal.

use jiff::Timestamp;
use jiff::civil::Date;

use super::record::{EngramId, Outcome};
use crate::ladder::Occasion;

/// One drill, with what the schedule made of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Drill {
    /// The engram that was asked for.
    pub engram: EngramId,
    /// The drill day.
    pub day: Date,
    /// When it was written.
    pub at: Timestamp,
    /// Whether the schedule asked, read off the engram's standing on the day.
    pub occasion: Occasion,
    /// What the ladder asked for at the rung the engram stood on.
    pub scheduled_days: u32,
    /// Days since the schedule's anchor.
    pub since_anchor: u32,
    /// Days since the secret was last in front of a person by any route. The
    /// honest retention interval.
    pub gap: u32,
    /// How the cold capture was judged.
    pub outcome: Outcome,
    /// Milliseconds from the prompt appearing to the first keystroke of the
    /// cold capture.
    pub ttfk_ms: Option<u64>,
    /// Captures after the cold one.
    pub follow_ups: u16,
    /// Whether a follow-up passed.
    pub recovered: bool,
    /// Whether the answer was looked up during a follow-up.
    pub aided: bool,
}

impl Drill {
    /// Whether the cold capture passed.
    pub fn passed(&self) -> bool {
        self.outcome == Outcome::Pass
    }
}
