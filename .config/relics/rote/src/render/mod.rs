//! One row model, three renderers.
//!
//! Columns run in order of increasing variability, so the widest cell is last
//! and never has to be padded. Colour is resolved once, at the edge, and passed
//! down: a renderer that probes the terminal for itself is a second opinion
//! where there should be one.

pub mod agent;
pub mod human;
pub mod json;

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
    use super::{Table, aligned};

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
    fn an_empty_table_says_so() {
        assert!(Table::new(&["a"]).is_empty());
        assert!(!table().is_empty());
    }
}
