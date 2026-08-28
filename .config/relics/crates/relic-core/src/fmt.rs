//! Formatting a person reads, shared so two relics do not spell one quantity
//! two ways.
//!
//! The clock is a parameter, never a call. A function that reaches for
//! `Timestamp::now()` can only be tested at the resolution of the machine's
//! actual clock, and a render loop that reaches for it repeatedly can report
//! two different "now"s in one frame.

use jiff::Timestamp;

const SECONDS_PER_DAY: i64 = 86_400;

/// Whole days between two instants, clamped at zero.
///
/// Days, because nothing a relic reports turns on hours: an item is stale, or a
/// note is still current, and both are counted in days or weeks.
///
/// ```
/// use jiff::{SignedDuration, Timestamp};
/// use relic_core::fmt::age_days;
///
/// let now = Timestamp::now();
/// assert_eq!(age_days(now - SignedDuration::from_hours(50), now), 2);
/// assert_eq!(age_days(now + SignedDuration::from_hours(50), now), 0);
/// ```
#[must_use]
pub fn age_days(at: Timestamp, now: Timestamp) -> i64 {
    (now - at).get_seconds().max(0) / SECONDS_PER_DAY
}

/// An age in the coarsest unit that still says something.
///
/// ```
/// use jiff::{SignedDuration, Timestamp};
/// use relic_core::fmt::age;
///
/// let now = Timestamp::now();
/// assert_eq!(age(now, now), "today");
/// assert_eq!(age(now - SignedDuration::from_hours(24 * 14), now), "2w");
/// ```
#[must_use]
pub fn age(at: Timestamp, now: Timestamp) -> String {
    match age_days(at, now) {
        0 => "today".to_owned(),
        d if d < 7 => format!("{d}d"),
        d if d < 90 => format!("{}w", d / 7),
        d => format!("{}mo", d / 30),
    }
}

/// A count and the word for it, so a line reads as a sentence at one as well as
/// at many.
///
/// ```
/// use relic_core::fmt::plural;
///
/// assert_eq!(plural(1, "item", "items"), "1 item");
/// assert_eq!(plural(0, "item", "items"), "0 items");
/// ```
#[must_use]
pub fn plural(count: usize, one: &str, many: &str) -> String {
    if count == 1 {
        format!("{count} {one}")
    } else {
        format!("{count} {many}")
    }
}

#[cfg(test)]
mod tests {
    use jiff::SignedDuration;

    use super::*;

    fn days_ago(now: Timestamp, days: i64) -> Timestamp {
        now - SignedDuration::from_hours(24 * days)
    }

    #[test]
    fn ages_read_in_the_coarsest_useful_unit() {
        let now = Timestamp::now();
        assert_eq!(age(now, now), "today");
        assert_eq!(age(days_ago(now, 3), now), "3d");
        assert_eq!(age(days_ago(now, 14), now), "2w");
        assert_eq!(age(days_ago(now, 120), now), "4mo");
    }

    #[test]
    fn each_unit_starts_where_the_last_one_ends() {
        let now = Timestamp::now();
        assert_eq!(age(days_ago(now, 6), now), "6d");
        assert_eq!(age(days_ago(now, 7), now), "1w");
        assert_eq!(age(days_ago(now, 89), now), "12w");
        assert_eq!(age(days_ago(now, 90), now), "3mo");
    }

    #[test]
    fn a_future_timestamp_is_today_rather_than_negative() {
        let now = Timestamp::now();
        assert_eq!(age_days(days_ago(now, -5), now), 0);
        assert_eq!(age(days_ago(now, -5), now), "today");
    }

    proptest::proptest! {
        /// An age never falls as the instant recedes. A comparison written the
        /// other way round reads the same and is wrong at exactly one boundary.
        #[test]
        fn age_never_falls_as_an_instant_recedes(near in 0i64..4000, far in 0i64..4000) {
            let (near, far) = (near.min(far), near.max(far));
            let now = Timestamp::from_second(1_700_000_000).expect("a fixed instant");
            proptest::prop_assert!(age_days(days_ago(now, far), now) >= age_days(days_ago(now, near), now));
        }
    }

    #[test]
    fn plural_agrees_with_its_count() {
        assert_eq!(plural(1, "note", "notes"), "1 note");
        assert_eq!(plural(2, "note", "notes"), "2 notes");
        assert_eq!(plural(0, "note", "notes"), "0 notes");
    }
}
