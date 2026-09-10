//! Boxed, colored, and meant to be read once. What a terminal gets.

use comfy_table::presets::UTF8_HORIZONTAL_ONLY;
use comfy_table::{ContentArrangement, Table as Grid};

use super::Table;

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";

/// Bold, when color is on.
pub fn bold(text: &str, color: bool) -> String {
    paint(text, BOLD, color)
}

/// Dim, when color is on.
pub fn dim(text: &str, color: bool) -> String {
    paint(text, DIM, color)
}

fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("{code}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

/// A titled block: heading, grid, then any closing lines.
pub fn block(heading: &str, table: &Table, notes: &[String], color: bool) -> String {
    let mut lines = Vec::new();
    if !heading.is_empty() {
        lines.push(bold(heading, color));
        lines.push(String::new());
    }
    if table.is_empty() {
        lines.push(dim("nothing", color));
    } else {
        lines.push(grid(table, color));
    }
    if !notes.is_empty() {
        lines.push(String::new());
        lines.extend(notes.iter().map(|note| dim(note, color)));
    }
    lines.join("\n")
}

fn grid(table: &Table, color: bool) -> String {
    let mut grid = Grid::new();
    // Color is already decided; comfy-table would otherwise probe the terminal
    // and reach a second opinion.
    if color {
        grid.enforce_styling();
    } else {
        grid.force_no_tty();
    }
    grid.load_style(UTF8_HORIZONTAL_ONLY)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(table.headers.clone());
    for row in &table.rows {
        grid.add_row(row.clone());
    }
    // comfy-table pads the last column, and a trailing space is what makes a
    // snapshot flap and a diff noisy.
    grid.to_string()
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::super::Table;
    use super::{block, bold, dim};

    fn table() -> Table {
        let mut table = Table::new(&["slug", "state"]);
        table.push(vec!["escrow-p".to_owned(), "due".to_owned()]);
        table
    }

    #[test]
    fn colour_is_only_applied_when_it_was_asked_for() {
        assert_eq!(bold("x", false), "x");
        assert_eq!(dim("x", false), "x");
        assert!(bold("x", true).contains("\x1b["));
        assert!(dim("x", true).contains("\x1b["));
    }

    #[test]
    fn a_block_carries_the_heading_the_rows_and_the_notes() {
        let text = block("schedule", &table(), &["a note".to_owned()], false);
        assert!(text.starts_with("schedule"));
        assert!(text.contains("escrow-p"));
        assert!(text.contains("a note"));
        assert!(!text.contains("\x1b["));
    }

    #[test]
    fn an_empty_table_renders_as_a_word_rather_than_an_empty_grid() {
        let text = block("schedule", &Table::new(&["slug"]), &[], false);
        assert!(text.contains("nothing"));
        assert!(!text.contains('─'));
    }
}
