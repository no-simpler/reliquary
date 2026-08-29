//! The three shapes every listing comes in: a table for a person, aligned
//! columns for a model, and JSON for a script.
//!
//! One row model feeds all three, so a column can never mean one thing in the
//! table and another in the JSON.

use anyhow::Result;
use comfy_table::presets::UTF8_HORIZONTAL_ONLY;
use comfy_table::{Attribute, Cell, Color, ContentArrangement, Table};
use serde::Serialize;

use relic_core::ui::Format;

/// One mode, as `list` reports it.
#[derive(Serialize)]
pub struct ModeRow {
    /// The `+token`.
    pub name: String,
    /// Each clause of its schedule, spelled for a reader.
    pub triggers: Vec<String>,
    /// What a refresh says, when it differs from the body.
    pub refrain: Option<String>,
    /// Where it was found.
    pub path: String,
    /// Why it cannot be used, when it cannot.
    pub broken: Option<String>,
}

/// One session, as `explain` reports it.
#[derive(Serialize)]
pub struct Explain {
    /// Which session.
    pub session: String,
    /// Compactions survived.
    pub generation: u32,
    /// Prompts seen.
    pub turns: u64,
    /// The window size when this was last written.
    pub tokens: u64,
    /// One row per clause of every active mode.
    pub clauses: Vec<Clause>,
}

/// One clause of one active mode, and where it stands.
#[derive(Serialize)]
pub struct Clause {
    /// The mode.
    pub mode: String,
    /// Times it has been said.
    pub fires: u32,
    /// The clause, as the mode file spells it.
    pub trigger: String,
    /// Where it stands.
    pub standing: String,
}

/// Left-aligns every column but the last, which is never padded.
fn aligned(rows: &[Vec<String>]) -> Vec<String> {
    let widths = rows.iter().fold(Vec::new(), |mut widths: Vec<usize>, row| {
        for (i, cell) in row.iter().enumerate() {
            match widths.get_mut(i) {
                Some(width) => *width = (*width).max(cell.chars().count()),
                None => widths.push(cell.chars().count()),
            }
        }
        widths
    });
    rows.iter()
        .map(|row| {
            let last = row.len().saturating_sub(1);
            row.iter()
                .enumerate()
                .map(|(i, cell)| {
                    let pad = widths.get(i).copied().unwrap_or(0);
                    if i == last {
                        cell.clone()
                    } else {
                        format!("{cell:pad$}")
                    }
                })
                .collect::<Vec<_>>()
                .join("  ")
                .trim_end()
                .to_owned()
        })
        .collect()
}

/// Colour is a decision made once, at the edge, rather than a condition every
/// cell repeats.
fn paint(cell: Cell, color: bool, with: Color) -> Cell {
    if color { cell.fg(with) } else { cell }
}

fn table(header: &[&str], color: bool) -> Table {
    let mut table = Table::new();
    // comfy-table would otherwise decide for itself by probing the terminal.
    // Colour is already resolved from --color, NO_COLOR and the output mode, and
    // one decision is the whole point.
    if color {
        table.enforce_styling();
    }
    table
        .load_style(UTF8_HORIZONTAL_ONLY)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(
            header
                .iter()
                .map(|h| Cell::new(h).add_attribute(Attribute::Bold)),
        );
    table
}

/// The mode corpus.
///
/// # Errors
///
/// When the JSON shape cannot be written.
pub fn list(format: Format, color: bool, rows: &[ModeRow]) -> Result<()> {
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(rows)?),
        Format::Agent => {
            if rows.is_empty() {
                println!("mantra (no modes)");
                return Ok(());
            }
            println!("mantra {} modes", rows.len());
            let cells: Vec<Vec<String>> = rows
                .iter()
                .map(|row| {
                    vec![
                        row.name.clone(),
                        row.broken.clone().map_or_else(
                            || row.triggers.join(", "),
                            |why| format!("BROKEN: {why}"),
                        ),
                        if row.refrain.is_some() {
                            "refrain".to_owned()
                        } else {
                            "body".to_owned()
                        },
                    ]
                })
                .collect();
            for line in aligned(&cells) {
                println!("{line}");
            }
        }
        Format::Human => {
            if rows.is_empty() {
                println!("no modes");
                return Ok(());
            }
            let mut out = table(&["MODE", "SAID", "REFRESH SAYS"], color);
            for row in rows {
                let said = match &row.broken {
                    Some(why) => paint(Cell::new(format!("broken: {why}")), color, Color::Red),
                    None => Cell::new(row.triggers.join("\n")),
                };
                out.add_row(vec![
                    Cell::new(&row.name).add_attribute(Attribute::Bold),
                    said,
                    Cell::new(row.refrain.as_deref().unwrap_or("the body")),
                ]);
            }
            println!("{out}");
        }
    }
    Ok(())
}

/// One session's standing.
///
/// # Errors
///
/// When the JSON shape cannot be written.
pub fn explain(format: Format, color: bool, view: &Explain) -> Result<()> {
    match format {
        Format::Json => println!("{}", serde_json::to_string_pretty(view)?),
        Format::Agent => {
            println!(
                "mantra {} tokens {} turns {} generation {}",
                view.session, view.tokens, view.turns, view.generation
            );
            if view.clauses.is_empty() {
                println!("(no modes active)");
                return Ok(());
            }
            let cells: Vec<Vec<String>> = view
                .clauses
                .iter()
                .map(|c| {
                    vec![
                        c.mode.clone(),
                        format!("x{}", c.fires),
                        c.trigger.clone(),
                        c.standing.clone(),
                    ]
                })
                .collect();
            for line in aligned(&cells) {
                println!("{line}");
            }
        }
        Format::Human => {
            println!(
                "session {}  —  {} tokens, {} turns, generation {}",
                view.session, view.tokens, view.turns, view.generation
            );
            if view.clauses.is_empty() {
                println!("no modes active");
                return Ok(());
            }
            let mut out = table(&["MODE", "SAID", "CLAUSE", "STANDING"], color);
            for clause in &view.clauses {
                out.add_row(vec![
                    Cell::new(&clause.mode).add_attribute(Attribute::Bold),
                    Cell::new(format!("{}x", clause.fires)),
                    Cell::new(&clause.trigger),
                    paint(
                        Cell::new(&clause.standing),
                        color && clause.standing.starts_with("due"),
                        Color::Green,
                    ),
                ]);
            }
            println!("{out}");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_column_but_the_last_is_padded() {
        let rows = vec![
            vec!["a".to_owned(), "bb".to_owned(), "c".to_owned()],
            vec!["dddd".to_owned(), "e".to_owned(), "ffff".to_owned()],
        ];
        assert_eq!(aligned(&rows), ["a     bb  c", "dddd  e   ffff"]);
    }

    #[test]
    fn a_short_last_cell_is_not_padded_out() {
        let rows = vec![
            vec!["a".to_owned(), "b".to_owned()],
            vec!["a".to_owned(), "bbbb".to_owned()],
        ];
        assert_eq!(aligned(&rows), ["a  b", "a  bbbb"]);
    }

    #[test]
    fn a_ragged_row_does_not_panic() {
        let rows = vec![vec!["a".to_owned()], vec!["a".to_owned(), "b".to_owned()]];
        assert_eq!(aligned(&rows), ["a", "a  b"]);
    }
}
