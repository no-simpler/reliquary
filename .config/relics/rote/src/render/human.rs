//! Boxed, coloured, and meant to be read once. What a terminal gets.

use comfy_table::presets::UTF8_HORIZONTAL_ONLY;
use comfy_table::{ContentArrangement, Table as Grid};
use relic_core::style::Style;

use super::Table;

/// A titled block: heading, grid, then any closing lines.
pub fn block(heading: &str, table: &Table, notes: &[String], style: Style) -> String {
    let mut lines = Vec::new();
    if !heading.is_empty() {
        lines.push(style.bold(heading));
        lines.push(String::new());
    }
    if table.is_empty() {
        lines.push(style.dim("nothing"));
    } else {
        lines.push(grid(table, style));
    }
    if !notes.is_empty() {
        lines.push(String::new());
        lines.extend(notes.iter().map(|note| style.dim(note)));
    }
    lines.join("\n")
}

fn grid(table: &Table, style: Style) -> String {
    let mut grid = Grid::new();
    // Colour is already decided; comfy-table would otherwise probe the terminal
    // and reach a second opinion.
    if style.colour {
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
    use relic_core::style::Style;

    use super::super::Table;
    use super::block;

    fn table() -> Table {
        let mut table = Table::new(&["slug", "state"]);
        table.push(vec!["escrow-p".to_owned(), "due".to_owned()]);
        table
    }

    #[test]
    fn a_block_carries_the_heading_the_rows_and_the_notes() {
        let text = block("schedule", &table(), &["a note".to_owned()], Style::PLAIN);
        assert!(text.starts_with("schedule"));
        assert!(text.contains("escrow-p"));
        assert!(text.contains("a note"));
        assert!(!text.contains("\x1b["));
    }

    #[test]
    fn colour_reaches_the_heading_when_asked_for() {
        let text = block("schedule", &table(), &[], Style::COLOUR);
        assert!(text.starts_with("\x1b[1mschedule"));
    }

    #[test]
    fn an_empty_table_renders_as_a_word_rather_than_an_empty_grid() {
        let text = block("schedule", &Table::new(&["slug"]), &[], Style::PLAIN);
        assert!(text.contains("nothing"));
        assert!(!text.contains('─'));
    }
}
