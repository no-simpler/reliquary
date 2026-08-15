use anyhow::Result;
// Horizontal rules only: description cells wrap to several lines, and without a
// rule between rows one item's detail reads as the next item's.
use comfy_table::presets::UTF8_HORIZONTAL_ONLY;
use comfy_table::{Attribute, Cell, Color, ColumnConstraint, ContentArrangement, Table, Width};

use super::{View, kind_badge};
use crate::ui::{age, age_days};

/// Warm while an item is fresh, hot once it has been sitting long enough to be
/// worth a second look.
fn age_color(days: i64) -> Color {
    match days {
        0..=6 => Color::Green,
        7..=20 => Color::Yellow,
        _ => Color::Red,
    }
}

pub fn list(view: &View<'_>) -> Result<()> {
    println!("{}", view.project.display());
    if view.records.is_empty() {
        println!("nothing on the docket");
        return Ok(());
    }

    let mut table = Table::new();
    // comfy-table would otherwise decide for itself by probing the terminal.
    // Colour is already resolved from --color, NO_COLOR and the output mode, and
    // one decision is the whole point.
    if view.color {
        table.enforce_styling();
    }
    table
        .load_style(UTF8_HORIZONTAL_ONLY)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(vec!["#", "ID", "KIND", "AGE", "TITLE", "DETAIL"]);

    // Without an upper bound the prose columns take the whole terminal, and a
    // description reads worse as one long line than as a wrapped cell.
    for (column, cap) in [(4usize, 44u16), (5, 64)] {
        if let Some(column) = table.column_mut(column) {
            column.set_constraint(ColumnConstraint::UpperBoundary(Width::Fixed(cap)));
        }
    }

    for (index, record) in view.records.iter().enumerate() {
        match &record.item {
            Ok(item) => {
                let mut detail = item.description.trim_end().to_owned();
                if let Some(reason) = item.blocked.as_deref().filter(|r| !r.trim().is_empty()) {
                    detail = format!("BLOCKED: {}\n{detail}", reason.trim());
                }
                let kind = paint(
                    Cell::new(kind_badge(item)),
                    view.color,
                    match item.kind() {
                        crate::item::Kind::Handoff => Color::Blue,
                        crate::item::Kind::Relay => Color::Cyan,
                        crate::item::Kind::Spec => Color::Magenta,
                    },
                );
                let detail_cell = if item.is_blocked() {
                    paint(Cell::new(detail), view.color, Color::Red)
                } else {
                    Cell::new(detail)
                };
                table.add_row(vec![
                    Cell::new(index + 1),
                    id_cell(record.id, view.color, Color::White),
                    kind,
                    paint(
                        Cell::new(age(item.created)),
                        view.color,
                        age_color(age_days(item.created)),
                    ),
                    Cell::new(&item.title),
                    detail_cell,
                ]);
            }
            Err(error) => {
                table.add_row(vec![
                    Cell::new(index + 1),
                    id_cell(record.id, view.color, Color::Red),
                    paint(Cell::new("INVALID"), view.color, Color::Red),
                    Cell::new(""),
                    Cell::new(record.path.display().to_string()),
                    paint(Cell::new(error), view.color, Color::Red),
                ]);
            }
        }
    }

    println!("{table}");
    Ok(())
}

fn paint(cell: Cell, color: bool, with: Color) -> Cell {
    if color { cell.fg(with) } else { cell }
}

/// The id is what a reader copies, so it carries weight — but bold is an escape
/// sequence like any other, and `--color never` has to mean no escapes at all.
fn id_cell(id: crate::id::Id, color: bool, with: Color) -> Cell {
    let cell = Cell::new(id);
    if color {
        cell.fg(with).add_attribute(Attribute::Bold)
    } else {
        cell
    }
}
