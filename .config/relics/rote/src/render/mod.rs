//! One row model, three renderers.
//!
//! Columns run in order of increasing variability, so the widest cell is last
//! and never has to be padded. Color is resolved once, at the edge, and passed
//! down: a renderer that probes the terminal for itself is a second opinion
//! where there should be one.

pub mod agent;
pub mod human;
pub mod json;

use crate::tui::Measured;

/// A table, before anyone has decided how it looks.
#[derive(Clone, Debug, Default)]
pub struct Table {
    /// Column headings.
    pub headers: Vec<&'static str>,
    /// Rows, each the same length as the headings.
    pub rows: Vec<Vec<String>>,
}

impl Table {
    /// A table with these headings and nothing in it yet.
    pub fn new(headers: &[&'static str]) -> Self {
        Self {
            headers: headers.to_vec(),
            rows: Vec::new(),
        }
    }

    /// Add a row.
    pub fn push(&mut self, cells: Vec<String>) {
        self.rows.push(cells);
    }

    /// Whether there is anything to draw.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }

    /// Column widths, measured rather than declared.
    fn widths(&self) -> Vec<usize> {
        let mut widths: Vec<usize> = self
            .headers
            .iter()
            .map(|header| header.chars().count())
            .collect();
        for row in &self.rows {
            for (index, cell) in row.iter().enumerate() {
                let width = cell.chars().count();
                if let Some(slot) = widths.get_mut(index)
                    && *slot < width
                {
                    *slot = width;
                }
            }
        }
        widths
    }
}

/// A duration in seconds to one decimal, or a dash for none.
///
/// One spelling for every place a latency is shown, so a card and a table
/// cannot drift a decimal apart.
pub fn seconds(ms: Option<u64>) -> String {
    match ms {
        Some(ms) => format!(
            "{:.1}s",
            f64::from(u32::try_from(ms).unwrap_or(u32::MAX)) / 1000.0
        ),
        None => "—".to_owned(),
    }
}

/// Both spans of a capture, or nothing at all when neither stands.
///
/// The retrieval and the typing, summed by the glyph between them — the two
/// measured spans, which is not wall time: an away-span and a field erased back
/// to empty are both excluded from either side.
#[must_use]
pub fn spans(measured: Measured) -> String {
    let Measured {
        ttfk_ms,
        capture_ms,
    } = measured;
    if ttfk_ms.is_none() && capture_ms.is_none() {
        return String::new();
    }
    format!("{} + {}", seconds(ttfk_ms), seconds(capture_ms))
}

/// Aligned plain text, headings included, with the last column unpadded.
pub fn aligned(table: &Table) -> Vec<String> {
    let widths = table.widths();
    let mut lines = Vec::with_capacity(table.rows.len().saturating_add(1));
    lines.push(row(
        &table
            .headers
            .iter()
            .map(|header| (*header).to_owned())
            .collect::<Vec<_>>(),
        &widths,
    ));
    for cells in &table.rows {
        lines.push(row(cells, &widths));
    }
    lines
}

fn row(cells: &[String], widths: &[usize]) -> String {
    let last = cells.len().saturating_sub(1);
    let mut line = String::new();
    for (index, cell) in cells.iter().enumerate() {
        if index == last {
            line.push_str(cell);
        } else {
            let width = widths.get(index).copied().unwrap_or(0);
            let pad = width.saturating_sub(cell.chars().count());
            line.push_str(cell);
            line.push_str(&" ".repeat(pad.saturating_add(2)));
        }
    }
    line.trim_end().to_owned()
}

#[cfg(test)]
mod tests {
    use super::{Measured, Table, aligned};

    fn table() -> Table {
        let mut table = Table::new(&["slug", "state"]);
        table.push(vec!["a".to_owned(), "due".to_owned()]);
        table.push(vec!["escrow-p".to_owned(), "in 5d".to_owned()]);
        table
    }

    #[test]
    fn columns_line_up_and_the_last_one_is_not_padded() {
        let lines = aligned(&table());
        assert_eq!(lines.first().unwrap(), "slug      state");
        assert_eq!(lines.get(1).unwrap(), "a         due");
        assert_eq!(lines.get(2).unwrap(), "escrow-p  in 5d");
        for line in &lines {
            assert_eq!(line.trim_end(), line, "no trailing whitespace");
        }
    }

    #[test]
    fn a_latency_is_seconds_to_one_decimal_or_a_dash() {
        assert_eq!(super::seconds(Some(1_400)), "1.4s");
        assert_eq!(super::seconds(Some(0)), "0.0s");
        assert_eq!(super::seconds(None), "—");
    }

    #[test]
    fn a_pair_of_spans_sums_the_two_that_were_measured() {
        let both = Measured {
            ttfk_ms: Some(2_100),
            capture_ms: Some(11_400),
        };
        assert_eq!(super::spans(both), "2.1s + 11.4s");
    }

    #[test]
    fn a_void_span_is_a_dash_and_two_of_them_are_nothing_at_all() {
        let recovered = Measured {
            ttfk_ms: None,
            capture_ms: Some(11_400),
        };
        let void = super::seconds(None);
        assert_eq!(super::spans(recovered), format!("{void} + 11.4s"));
        let interrupted = Measured {
            ttfk_ms: Some(2_100),
            capture_ms: None,
        };
        assert_eq!(super::spans(interrupted), format!("2.1s + {void}"));
        assert_eq!(
            super::spans(Measured::default()),
            "",
            "a row with nothing measured says nothing, rather than saying it twice"
        );
    }

    #[test]
    fn an_empty_table_says_so() {
        assert!(Table::new(&["a"]).is_empty());
        assert!(!table().is_empty());
    }
}
