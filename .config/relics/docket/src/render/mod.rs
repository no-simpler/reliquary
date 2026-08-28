pub mod agent;
pub mod human;
pub mod json;

use std::path::Path;

use anyhow::Result;

use crate::item::Item;
use crate::query::Hit;
use crate::store::Record;
use relic_core::ui::Format;

/// How wide the project column may get. A path is unbounded and the unbounded
/// column is the tagline, so this one is cut to fit the way a name is.
const PROJECT_MAX: usize = 40;

pub struct View<'a> {
    /// The project every item sits on, or nothing when the listing spans the
    /// machine and each row names its own.
    pub project: Option<&'a Path>,
    pub hits: &'a [Hit],
    pub color: bool,
    /// One instant for the whole frame. A render that reaches for the clock per
    /// row can report two different "now"s in one table.
    pub now: jiff::Timestamp,
    /// Whether a filter was in play, so an empty result can say whether there
    /// is nothing here or nothing that answered.
    pub narrowed: bool,
}

impl View<'_> {
    /// How many projects the listing covers, for the line that heads one
    /// spanning the machine.
    pub fn projects(&self) -> usize {
        let mut seen: Vec<&Path> = Vec::new();
        for hit in self.hits {
            if !seen.contains(&hit.record.project.as_path()) {
                seen.push(&hit.record.project);
            }
        }
        seen.len()
    }
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

pub fn row(position: usize, record: &Record, now: jiff::Timestamp) -> Row {
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
                    relic_core::fmt::age(item.created, now),
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
pub fn kind_badge(item: &Item) -> String {
    use crate::item::Rung;
    match &item.rung {
        Rung::Handoff => "handoff".to_owned(),
        Rung::Relay(chain) => format!("relay hop={}", chain.hop),
        Rung::Spec { stage, .. } => format!("spec/{stage}"),
    }
}

/// The row a listing shows for one hit: the cells `row` gives, the project
/// ahead of them when the listing spans the machine, and the notes a query
/// earned under them. `announce` takes `row` directly, so a banner stays the
/// blocked line alone.
pub fn hit_row(hit: &Hit, name_project: bool, now: jiff::Timestamp) -> Row {
    let mut row = row(hit.position, &hit.record, now);
    if name_project {
        row.cells.insert(0, project_cell(&hit.record.project));
    }
    if let Ok(item) = &hit.record.item
        && let Some(tags) = tag_line(item)
    {
        row.notes.push(format!("tags: {tags}"));
    }
    if let Some(excerpt) = &hit.excerpt {
        row.notes.push(format!("match: {excerpt}"));
    }
    row
}

/// The project a roster row sits on, cut to fit a column.
pub fn project_cell(project: &Path) -> String {
    let home = std::env::var_os("HOME");
    shorten(project, home.as_deref().map(Path::new))
}

/// Home becomes a tilde, and a path still too long loses its head rather than
/// its tail: the leading directories are what every project on a machine has in
/// common, and the trailing ones are what say which project this is.
fn shorten(project: &Path, home: Option<&Path>) -> String {
    let text = match home.and_then(|home| project.strip_prefix(home).ok()) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_owned(),
        Some(rest) => format!("~/{}", rest.display()),
        None => project.display().to_string(),
    };
    let width = text.chars().count();
    if width <= PROJECT_MAX {
        return text;
    }

    // One character of the budget goes to the elision, and the cut falls on a
    // separator when one is in reach, so the head that is dropped is whole
    // directories rather than half of one.
    let keep = PROJECT_MAX - 1;
    let from = width - keep;
    let tail: String = text.chars().skip(from).collect();
    let cut = match tail.find('/') {
        Some(at) if tail[..at].chars().count() * 2 <= keep => at,
        _ => 0,
    };
    format!("…{}", &tail[cut..])
}

/// The tags an item carries as one line, separated by the space a tag may not
/// contain. Each renderer labels it in its own case.
pub fn tag_line(item: &Item) -> Option<String> {
    if item.tags.is_empty() {
        return None;
    }
    Some(item.tags.join(" "))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_cell_shortens_home() {
        let home = Path::new("/Users/example");
        assert_eq!(
            shorten(Path::new("/Users/example/Developer/halo"), Some(home)),
            "~/Developer/halo"
        );
        assert_eq!(shorten(Path::new("/Users/example"), Some(home)), "~");
        assert_eq!(shorten(Path::new("/opt/tools"), Some(home)), "/opt/tools");
        assert_eq!(shorten(Path::new("/opt/tools"), None), "/opt/tools");
    }

    #[test]
    fn a_project_cell_loses_its_head_when_it_must() {
        let deep = Path::new("/Users/example/Developer/benefactor/services/offer/pillar/api");
        let cell = shorten(deep, Some(Path::new("/Users/example")));
        assert!(cell.starts_with('…'), "{cell}");
        assert!(cell.ends_with("pillar/api"), "{cell}");
        assert!(cell.chars().count() <= PROJECT_MAX, "{cell}");
    }
}
