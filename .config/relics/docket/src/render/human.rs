use std::fmt::Write;

use anyhow::Result;
// Horizontal rules only: a narrow terminal wraps the detail cell over several
// lines, and without a rule between rows one item's detail reads as the next
// item's.
use comfy_table::presets::UTF8_HORIZONTAL_ONLY;
use comfy_table::{Attribute, Cell, Color, ColumnConstraint, ContentArrangement, Table, Width};

use super::{View, kind_badge, project_cell, tag_line};
use relic_core::fmt::{age, age_days, plural};

/// Warm while an item is fresh, hot once it has been sitting long enough to be
/// worth a second look.
fn age_color(days: i64) -> Color {
    match days {
        0..=6 => Color::Green,
        7..=20 => Color::Yellow,
        _ => Color::Red,
    }
}

/// A column bound that no field can exceed: the stored caps are far below
/// `u16::MAX`, and saturating says so without an unreachable error arm.
fn column_width(cap: usize) -> u16 {
    u16::try_from(cap).unwrap_or(u16::MAX)
}

/// One long function on purpose: it is a table definition, and every line of it
/// is one column or one cell. Splitting it would move the layout somewhere the
/// reader has to assemble it from.
#[expect(
    clippy::too_many_lines,
    reason = "a table definition reads as one piece"
)]
pub fn list(view: &View<'_>) -> Result<()> {
    match view.project {
        Some(project) => println!("{project}"),
        None => println!("{}", plural(view.projects(), "project", "projects")),
    }
    if view.hits.is_empty() {
        println!(
            "{}",
            if view.narrowed {
                "nothing matches"
            } else {
                "nothing on the docket"
            }
        );
        return Ok(());
    }

    // A listing across the machine names each row's project ahead of the cells
    // every listing shows, which shifts every column after it by one.
    let named = view.project.is_none();
    let offset = usize::from(named);
    let mut header = vec!["#", "ID", "KIND", "AGE", "NAME", "TAGLINE"];
    if named {
        header.insert(0, "PROJECT");
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
        .set_header(header);

    // The display bound is the stored bound: a name and a tagline that pass
    // validation each occupy one row, and only a narrow terminal wraps them.
    for (column, cap) in [
        (offset + 4, column_width(crate::field::NAME_MAX)),
        (offset + 5, column_width(crate::field::TAGLINE_MAX)),
    ] {
        if let Some(column) = table.column_mut(column) {
            column.set_constraint(ColumnConstraint::UpperBoundary(Width::Fixed(cap)));
        }
    }

    for hit in view.hits {
        let record = &hit.record;
        let mut cells = Vec::new();
        if named {
            // An anchor rather than a signal, so it takes no colour.
            cells.push(Cell::new(project_cell(&record.project)));
        }
        match &record.item {
            Ok(item) => {
                let mut detail = item.tagline.trim_end().to_owned();
                if let Some(reason) = item.blocked.as_deref().filter(|r| !r.trim().is_empty()) {
                    detail = format!("BLOCKED: {}\n{detail}", reason.trim());
                }
                if let Some(tags) = tag_line(item) {
                    let _ = write!(detail, "\nTAGS: {tags}");
                }
                if let Some(excerpt) = &hit.excerpt {
                    let _ = write!(detail, "\nMATCH: {excerpt}");
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
                cells.extend([
                    Cell::new(hit.position),
                    id_cell(record.id, view.color, Color::White),
                    kind,
                    paint(
                        Cell::new(age(item.created, view.now)),
                        view.color,
                        age_color(age_days(item.created, view.now)),
                    ),
                    Cell::new(&item.name),
                    detail_cell,
                ]);
            }
            Err(error) => {
                cells.extend([
                    Cell::new(hit.position),
                    id_cell(record.id, view.color, Color::Red),
                    paint(Cell::new("INVALID"), view.color, Color::Red),
                    Cell::new(""),
                    Cell::new(""),
                    paint(
                        Cell::new(format!("{error}\n{}", record.path)),
                        view.color,
                        Color::Red,
                    ),
                ]);
            }
        }
        table.add_row(cells);
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
