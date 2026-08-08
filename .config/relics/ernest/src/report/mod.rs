//! Presentation. `human` prints a report, `json` serialises one, `diff`
//! compares two.

pub mod diff;
pub mod human;
pub mod json;

/// Thousands-separated integer, so six-figure character counts stay readable.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Signed thousands-separated integer, for deltas.
pub fn signed(n: i64) -> String {
    let sign = if n < 0 { "-" } else { "+" };
    format!("{sign}{}", thousands(n.unsigned_abs()))
}

/// A density ratio as a percentage. `None` means nothing countable was found,
/// which is not the same as no prose.
pub fn percent(density: Option<f64>) -> String {
    match density {
        Some(d) => format!("{:.1}%", d * 100.0),
        None => "n/a".to_string(),
    }
}

/// A density change in percentage points.
pub fn percent_delta(before: Option<f64>, after: Option<f64>) -> String {
    match (before, after) {
        (Some(b), Some(a)) => {
            let pp = (a - b) * 100.0;
            format!("{}{:.1}", if pp < 0.0 { "-" } else { "+" }, pp.abs())
        }
        _ => "n/a".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_digits_in_threes() {
        assert_eq!(thousands(0), "0");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000), "1,000");
        assert_eq!(thousands(366_812), "366,812");
        assert_eq!(thousands(1_234_567), "1,234,567");
    }

    #[test]
    fn renders_densities_and_their_absence() {
        assert_eq!(percent(Some(0.1123)), "11.2%");
        assert_eq!(percent(Some(0.0)), "0.0%");
        assert_eq!(percent(None), "n/a");
    }

    #[test]
    fn signs_deltas_explicitly() {
        assert_eq!(signed(-1_004), "-1,004");
        assert_eq!(signed(1_004), "+1,004");
        assert_eq!(percent_delta(Some(0.112), Some(0.098)), "-1.4");
    }
}
