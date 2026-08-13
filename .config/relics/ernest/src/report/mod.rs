//! Presentation. `human` prints a report, `json` serialises one, `diff`
//! compares two; `table` lays out the rows all of them share, `notes` the
//! conditional lines that close both text reports.
//!
//! Text output has three registers: a headline that always shows, a body that
//! shows only what `--by` names, and notes that each fire on their own
//! condition. Only the first is unconditional, which is what keeps a bare run
//! down to the figure it was called for.

pub mod diff;
pub mod human;
pub mod json;
pub mod notes;
pub mod table;

use crate::aggregate::Views;

/// What to show of a report, and how much of each ranked view. Shared by both
/// text renderers so `--by` and `--top` mean one thing across the tool.
#[derive(Debug, Clone, Copy)]
pub struct Presentation {
    pub views: Views,
    pub top: usize,
}

/// A report is blocks joined by exactly one blank line. An empty block is
/// dropped rather than separated, so a view that produced no rows cannot leave
/// a gap behind it — the defect a run finding no supported files used to show.
#[derive(Debug, Default)]
pub struct Blocks(Vec<String>);

impl Blocks {
    pub fn push(&mut self, block: impl Into<String>) {
        let block = block.into();
        if !block.trim().is_empty() {
            self.0.push(block.trim_end().to_string());
        }
    }

    pub fn render(&self) -> String {
        if self.0.is_empty() {
            return String::new();
        }
        format!("{}\n", self.0.join("\n\n"))
    }
}

/// The noun a count takes. English's regular rule and nothing else; an
/// irregular noun spells itself at the call site. Returning the noun rather
/// than the phrase lets a caller put the number where its sentence wants it.
pub fn plural(n: u64, noun: &str) -> String {
    if n == 1 {
        noun.to_string()
    } else {
        format!("{noun}s")
    }
}

/// A count and its noun, agreeing.
pub fn count(n: u64, noun: &str) -> String {
    format!("{} {}", thousands(n), plural(n, noun))
}

/// Thousands-separated integer, so six-figure character counts stay readable.
pub fn thousands(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
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

    #[test]
    fn agrees_a_count_with_its_noun() {
        assert_eq!(count(0, "file"), "0 files");
        assert_eq!(count(1, "file"), "1 file");
        assert_eq!(count(1_004, "file"), "1,004 files");
    }

    #[test]
    fn separates_blocks_by_exactly_one_blank_line() {
        let mut blocks = Blocks::default();
        blocks.push("headline\n");
        // What an unrequested view hands back, and what used to leave a gap.
        blocks.push(String::new());
        blocks.push("  a note\n");
        assert_eq!(blocks.render(), "headline\n\n  a note\n");
    }

    #[test]
    fn renders_nothing_from_nothing() {
        assert_eq!(Blocks::default().render(), "");
    }
}
