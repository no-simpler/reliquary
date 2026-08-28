use anyhow::Result;
// Horizontal rules only: a narrow terminal wraps the detail cell over several
// lines, and without a rule between rows one note's detail reads as the next
// note's.
use comfy_table::presets::UTF8_HORIZONTAL_ONLY;
use comfy_table::{Attribute, Cell, Color, ColumnConstraint, ContentArrangement, Table, Width};

use relic_core::fmt::plural;

use super::{Digest, View, count, heading};
use crate::note::{Kind, Note, Status};
use crate::store::Record;
use relic_core::fmt::{age, age_days};

/// Cool while a note is fresh, hot once it has been sitting long enough that
/// nobody is going to act on it without being reminded.
fn age_color(days: i64) -> Color {
    match days {
        0..=6 => Color::Green,
        7..=30 => Color::Yellow,
        _ => Color::Red,
    }
}

/// Kinds that name a defect in what is written carry more weight than kinds
/// that name a cost, so they are told apart at a glance.
fn kind_color(kind: Kind) -> Color {
    match kind {
        Kind::Gap | Kind::Conflict | Kind::Stale => Color::Magenta,
        Kind::Rebuff => Color::Red,
        Kind::Hunt | Kind::Rework => Color::Yellow,
        Kind::Friction => Color::Blue,
    }
}

pub fn list(view: &View<'_>) -> Result<()> {
    println!("{}", heading(view.scope, view.records.len()));
    if view.records.is_empty() {
        println!("nothing on the midden");
        return Ok(());
    }
    println!("{}", table(view.records, view.color, view.now, true));
    Ok(())
}

pub fn digest(view: &Digest<'_>) -> Result<()> {
    let total: usize = view.groups.iter().map(|group| group.records.len()).sum();
    println!("{}", heading(view.scope, total));
    if view.groups.is_empty() {
        println!("nothing on the midden");
        return Ok(());
    }

    for group in view.groups {
        println!();
        println!(
            "{}  [{}]",
            group.target,
            plural(group.records.len(), "note", "notes")
        );
        println!(
            "{}",
            table_refs(&group.records, view.color, view.now, false)
        );
    }
    Ok(())
}

fn table(records: &[Record], color: bool, now: jiff::Timestamp, with_target: bool) -> Table {
    let refs: Vec<&Record> = records.iter().collect();
    table_refs(&refs, color, now, with_target)
}

fn table_refs(records: &[&Record], color: bool, now: jiff::Timestamp, with_target: bool) -> Table {
    let mut table = Table::new();
    // comfy-table would otherwise decide for itself by probing the terminal.
    // Colour is already resolved from --color, NO_COLOR and the output mode, and
    // one decision is the whole point.
    if color {
        table.enforce_styling();
    }
    let mut header = vec!["#", "ID", "KIND", "SEEN", "AGE", "TITLE", "DETAIL"];
    if with_target {
        header.push("FIX IN");
    }
    table
        .load_style(UTF8_HORIZONTAL_ONLY)
        .set_content_arrangement(ContentArrangement::Dynamic)
        .set_header(header);

    // The display bound is the stored bound: a title and a detail that pass
    // validation each occupy one row, and only a narrow terminal wraps them.
    for (column, cap) in [
        (5usize, crate::field::TITLE_MAX as u16),
        (6, crate::field::DETAIL_MAX as u16),
    ] {
        if let Some(column) = table.column_mut(column) {
            column.set_constraint(ColumnConstraint::UpperBoundary(Width::Fixed(cap)));
        }
    }

    for (index, record) in records.iter().enumerate() {
        match &record.note {
            Ok(note) => {
                let mut row = vec![
                    Cell::new(index + 1),
                    id_cell(record.id, color, Color::White),
                    paint(Cell::new(note.kind), color, kind_color(note.kind)),
                    Cell::new(count(note.occurrences)),
                    paint(
                        Cell::new(age(note.updated, now)),
                        color,
                        age_color(age_days(note.updated, now)),
                    ),
                    Cell::new(&note.title),
                    Cell::new(detail_of(note)),
                ];
                if with_target {
                    row.push(Cell::new(note.target.clone().unwrap_or_default()));
                }
                table.add_row(row);
            }
            Err(error) => {
                let mut row = vec![
                    Cell::new(index + 1),
                    id_cell(record.id, color, Color::Red),
                    paint(Cell::new("INVALID"), color, Color::Red),
                    Cell::new(""),
                    Cell::new(""),
                    Cell::new(record.path.display().to_string()),
                    paint(Cell::new(error), color, Color::Red),
                ];
                if with_target {
                    row.push(Cell::new(""));
                }
                table.add_row(row);
            }
        }
    }
    table
}

/// A status that is not open belongs beside the detail, because it changes how
/// the whole row should be read.
fn detail_of(note: &Note) -> String {
    let detail = note.detail.clone().unwrap_or_default();
    if note.status == Status::Open {
        return detail;
    }
    let badge = note.status.as_str().to_uppercase();
    if detail.is_empty() {
        badge
    } else {
        format!("{badge}\n{detail}")
    }
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
