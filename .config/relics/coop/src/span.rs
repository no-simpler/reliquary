//! Durations as a declaration writes them.
//!
//! `jiff` parses ISO 8601 (`P1D`), which nobody types into a config file. This
//! is the shape a human reaches for, and it is deliberately tiny: an integer and
//! one unit, with no compound forms, because `1d12h` invites a parser that grows
//! a grammar and this one must stay auditable at a glance.

use std::time::Duration;

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;
const DAY: u64 = 24 * HOUR;
const WEEK: u64 = 7 * DAY;

/// Parse `30s`, `10m`, `4h`, `1d`, `2w`.
///
/// A bare integer is refused rather than assigned a default unit: the one thing
/// worse than an unreadable duration is a readable one that means something
/// else.
pub fn parse(text: &str) -> Option<Duration> {
    let trimmed = text.trim();
    let mut chars = trimmed.chars();
    let unit = chars.next_back()?;
    let digits = chars.as_str();
    if digits.is_empty() {
        return None;
    }
    let count: u64 = digits.parse().ok()?;
    let seconds = match unit {
        's' => 1,
        'm' => MINUTE,
        'h' => HOUR,
        'd' => DAY,
        'w' => WEEK,
        _ => return None,
    };
    count.checked_mul(seconds).map(Duration::from_secs)
}

#[cfg(test)]
mod tests {
    use super::parse;
    use std::time::Duration;

    #[test]
    fn every_unit_is_understood() {
        assert_eq!(parse("30s"), Some(Duration::from_secs(30)));
        assert_eq!(parse("10m"), Some(Duration::from_secs(600)));
        assert_eq!(parse("4h"), Some(Duration::from_secs(14_400)));
        assert_eq!(parse("1d"), Some(Duration::from_secs(86_400)));
        assert_eq!(parse("2w"), Some(Duration::from_secs(1_209_600)));
    }

    #[test]
    fn surrounding_space_is_not_a_syntax_error() {
        assert_eq!(parse("  1d  "), Some(Duration::from_secs(86_400)));
    }

    #[test]
    fn a_bare_number_is_refused_rather_than_guessed() {
        assert_eq!(parse("60"), None);
    }

    #[test]
    fn nonsense_is_none_rather_than_zero() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("d"), None);
        assert_eq!(parse("1y"), None);
        assert_eq!(parse("-1d"), None);
        assert_eq!(parse("1.5h"), None);
    }

    #[test]
    fn an_overflowing_count_is_none_rather_than_a_wrap() {
        assert_eq!(parse("99999999999999999999w"), None);
    }
}
