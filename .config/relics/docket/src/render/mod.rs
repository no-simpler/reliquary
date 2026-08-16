pub mod agent;
pub mod human;
pub mod json;

use std::path::Path;

use anyhow::Result;

use crate::store::Record;
use crate::ui::Format;

pub struct View<'a> {
    pub project: &'a Path,
    pub records: &'a [Record],
    pub color: bool,
}

pub fn list(view: &View<'_>, format: Format) -> Result<()> {
    match format {
        Format::Human => human::list(view),
        Format::Agent => agent::list(view),
        Format::Json => json::list(view),
    }
}

/// One item as the fixed cells that precede its tagline. Columns run in order of
/// increasing variability so the tagline, the widest cell, comes last and is
/// never padded — alignment then costs nothing at the end of a line.
pub struct Row {
    pub cells: Vec<String>,
    pub tagline: String,
    pub notes: Vec<String>,
}

pub fn row(position: usize, record: &Record) -> Row {
    match &record.item {
        Ok(item) => {
            let mut notes = Vec::new();
            if let Some(reason) = item
                .blocked
                .as_deref()
                .map(str::trim)
                .filter(|r| !r.is_empty())
            {
                notes.push(format!("blocked: {reason}"));
            }
            Row {
                cells: vec![
                    position.to_string(),
                    record.id.to_string(),
                    kind_badge(item),
                    crate::ui::age(item.created),
                    // Clamped, not trusted: an item written before the name was
                    // bounded would otherwise widen the column for every row.
                    crate::field::clamp(&item.name, crate::field::NAME_MAX),
                ],
                tagline: item.tagline.trim().to_owned(),
                notes,
            }
        }
        Err(error) => Row {
            cells: vec![
                "!".to_owned(),
                record.id.to_string(),
                "INVALID".to_owned(),
                String::new(),
                String::new(),
            ],
            tagline: error.to_string(),
            notes: vec![record.path.display().to_string()],
        },
    }
}

/// Renders rows with every fixed column padded to its widest value, and returns
/// the column at which taglines start, so a caller can indent continuation lines
/// under them.
pub fn aligned(rows: &[Row], indent: &str) -> (Vec<String>, usize) {
    let mut widths = vec![0usize; rows.iter().map(|r| r.cells.len()).max().unwrap_or(0)];
    for row in rows {
        for (index, cell) in row.cells.iter().enumerate() {
            widths[index] = widths[index].max(cell.chars().count());
        }
    }
    let head = indent.chars().count() + widths.iter().map(|w| w + 2).sum::<usize>();

    let lines = rows
        .iter()
        .map(|row| {
            let mut line = String::from(indent);
            for (index, cell) in row.cells.iter().enumerate() {
                line.push_str(&format!("{cell:<width$}  ", width = widths[index]));
            }
            line.push_str(&row.tagline);
            line.truncate(line.trim_end().len());
            line
        })
        .collect();
    (lines, head)
}

/// The badge every renderer shows for an item's rung: kind, plus the one
/// qualifier that rung carries.
pub fn kind_badge(item: &crate::item::Item) -> String {
    use crate::item::Rung;
    match &item.rung {
        Rung::Handoff => "handoff".to_owned(),
        Rung::Relay(chain) => format!("relay hop={}", chain.hop),
        Rung::Spec { stage, .. } => format!("spec/{stage}"),
    }
}
