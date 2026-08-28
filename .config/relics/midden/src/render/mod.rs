pub mod agent;
pub mod human;
pub mod json;

use camino::Utf8Path;

use anyhow::Result;

use crate::note::Status;
use crate::store::Record;
use relic_core::fmt::plural;
use relic_core::ui::Format;

pub const NO_TARGET: &str = "(no target)";

pub struct View<'a> {
    /// The project a listing was narrowed to, when it was narrowed at all. The
    /// corpus is machine-wide, so the unnarrowed case is the ordinary one.
    pub scope: Option<&'a Utf8Path>,
    pub records: &'a [Record],
    pub color: bool,
    /// One instant for the whole frame. A render that reaches for the clock per
    /// row can report two different "now"s in one table.
    pub now: jiff::Timestamp,
}

pub fn list(view: &View<'_>, format: Format) -> Result<()> {
    match format {
        Format::Human => human::list(view),
        Format::Agent => agent::list(view),
        Format::Json => json::list(view),
    }
}

/// Notes that would be fixed in the same place. Grouping by target is what
/// turns a heap into a worklist: one section is one file to open.
pub struct Group<'a> {
    pub target: String,
    pub records: Vec<&'a Record>,
}

impl Group<'_> {
    pub fn weight(&self) -> u32 {
        self.records.iter().map(|record| record.occurrences()).sum()
    }
}

pub struct Digest<'a> {
    pub scope: Option<&'a Utf8Path>,
    pub groups: &'a [Group<'a>],
    pub color: bool,
    /// One instant for the whole frame, for the same reason [`View`] carries one.
    pub now: jiff::Timestamp,
}

pub fn digest(view: &Digest<'_>, format: Format) -> Result<()> {
    match format {
        Format::Human => human::digest(view),
        Format::Agent => agent::digest(view),
        Format::Json => json::digest(view),
    }
}

/// One note as the fixed cells that precede its title. Columns run in order of
/// increasing variability so the title, the only unbounded cell, comes last and
/// is never padded — alignment then costs nothing at the end of a line.
pub struct Row {
    pub cells: Vec<String>,
    pub title: String,
    pub detail: String,
    pub notes: Vec<String>,
}

pub fn row(position: usize, record: &Record, now: jiff::Timestamp) -> Row {
    match &record.note {
        Ok(note) => {
            let mut notes = Vec::new();
            if note.status != Status::Open {
                notes.push(note.status.to_string());
            }
            if let Some(target) = note.target.as_deref() {
                notes.push(format!("fix in: {target}"));
            }
            Row {
                cells: vec![
                    position.to_string(),
                    record.id.to_string(),
                    note.kind.to_string(),
                    count(note.occurrences),
                    relic_core::fmt::age(note.updated, now),
                ],
                title: note.title.clone(),
                detail: note.detail.clone().unwrap_or_default(),
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
            title: error.to_string(),
            detail: String::new(),
            notes: vec![record.path.to_string()],
        },
    }
}

/// How many times, shown only once it is more than once: a count of one is the
/// default state of every note and says nothing.
pub fn count(occurrences: u32) -> String {
    if occurrences > 1 {
        format!("x{occurrences}")
    } else {
        String::new()
    }
}

/// Renders rows with every fixed column padded to its widest value, and returns
/// the column at which titles start, so a caller can indent continuation lines
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
            line.push_str(&row.title);
            line
        })
        .collect();
    (lines, head)
}

/// The line every renderer opens with: what was read, and how much of it.
pub fn heading(scope: Option<&Utf8Path>, count: usize) -> String {
    match scope {
        Some(project) => format!("midden {} — {}", project, plural(count, "note", "notes")),
        None => format!("midden — {}", plural(count, "note", "notes")),
    }
}
