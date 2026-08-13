//! The text report.
//!
//! A headline that always shows, breakdowns that show only when `--by` names
//! them, and notes that each fire on their own condition. A bare run is the
//! figure it was called for and the caveats on that figure, nothing else.

use crate::aggregate::{DOCS_COHORT, Report, SOURCE_COHORT};
use crate::span::Counts;

use super::notes::Notes;
use super::table::{Column, Table};
use super::{Blocks, Presentation, percent, thousands};

pub fn render(report: &Report, show: Presentation) -> String {
    let mut blocks = Blocks::default();
    blocks.push(headline(report));
    blocks.push(breakdown(report, show));
    blocks.push(ranked(report, show, Rank::File));
    blocks.push(ranked(report, show, Rank::Section));
    blocks.push(notes(report, show).render());
    blocks.render()
}

/// The density alone, for a caller acting on the exit code. `n/a` rather than a
/// number when nothing countable was found, matching what the snapshot carries.
pub fn value(report: &Report) -> String {
    match report.headline().density {
        Some(density) => format!("{:.1}\n", density * 100.0),
        None => "n/a\n".to_string(),
    }
}

/// The figure, and the one comparator that complements rather than repeats it.
fn headline(report: &Report) -> String {
    let unit = report.unit;
    let total = report.headline();
    if report.cohorts.is_empty() {
        return "prose density  n/a   (no supported files found)\n".to_string();
    }
    let base = total.counts.prose(unit) + total.counts.code(unit);
    format!(
        "prose density  {}   (prose {} / base {} {})\n{}",
        percent(total.density),
        thousands(total.counts.prose(unit)),
        thousands(base),
        unit.label(),
        docs_line(report),
    )
}

/// The roll-up leads its group and its parts indent under it, so the table
/// visibly sums to the headline rather than sitting beside it.
fn breakdown(report: &Report, show: Presentation) -> String {
    if report.cohorts.is_empty() || !(show.views.by_cohort || show.views.by_language) {
        return String::new();
    }
    let unit = report.unit;
    let total = report.headline();
    // `--by language` is `--by cohort` decomposed one level further, so asking
    // for both is asking for the deeper one.
    let languages = show.views.by_language;

    // Provenance only ever varies on a language row, and a column sizes to its
    // header even when every cell under it is blank — so the shallower table
    // does not carry one.
    let mut columns = vec![Column::left(if languages {
        "total / cohort / language"
    } else {
        "total / cohort"
    })];
    if languages {
        columns.push(Column::left("provenance"));
    }
    columns.extend([
        Column::right("density"),
        Column::right("prose"),
        Column::right("code"),
        Column::right("files"),
    ]);
    let mut table = Table::new(columns);

    let roll = |label: &str, density: Option<f64>, counts: Counts, files: u64| {
        let mut row = vec![label.to_string()];
        if languages {
            row.push(String::new());
        }
        row.extend([
            percent(density),
            thousands(counts.prose(unit)),
            thousands(counts.code(unit)),
            thousands(files),
        ]);
        row
    };

    table.push(0, roll("total", total.density, total.counts, total.files));
    for cohort in &report.cohorts {
        table.push(
            1,
            roll(&cohort.cohort, cohort.density, cohort.counts, cohort.files),
        );
        if !languages {
            continue;
        }
        for language in &cohort.languages {
            table.push(
                2,
                vec![
                    language.language.clone(),
                    language.provenance.label().to_string(),
                    percent(language.density),
                    thousands(language.counts.prose(unit)),
                    thousands(language.counts.code(unit)),
                    thousands(language.files),
                ],
            );
        }
    }
    table.render()
}

/// The two views that rank rather than roll up. They share a shape: measurements
/// first, then the key, which is long and of no fixed width.
#[derive(Clone, Copy)]
enum Rank {
    File,
    Section,
}

fn ranked(report: &Report, show: Presentation, rank: Rank) -> String {
    let unit = report.unit;
    let (wanted, noun) = match rank {
        Rank::File => (show.views.by_file, "file"),
        Rank::Section => (show.views.by_section, "section"),
    };
    if !wanted {
        return String::new();
    }
    let rows: Vec<(Option<f64>, u64, u64, String)> = match rank {
        Rank::File => report
            .files
            .iter()
            .flatten()
            .map(|f| {
                (
                    f.density,
                    f.counts.prose(unit),
                    f.counts.code(unit),
                    f.path.clone(),
                )
            })
            .collect(),
        Rank::Section => report
            .sections
            .iter()
            .flatten()
            .map(|s| {
                (
                    s.density,
                    s.counts.prose(unit),
                    s.counts.code(unit),
                    format!("{}#{}", s.path, s.section),
                )
            })
            .collect(),
    };

    let mut table = Table::new(vec![
        Column::right("density"),
        Column::right("prose"),
        Column::right("code"),
        Column::left(noun),
    ]);
    for (density, prose, code, key) in rows.iter().take(show.top) {
        table.push(
            0,
            vec![
                percent(*density),
                thousands(*prose),
                thousands(*code),
                key.clone(),
            ],
        );
    }
    table.render()
}

fn notes(report: &Report, show: Presentation) -> Notes {
    let mut notes = Notes::default();
    if let Some(files) = &report.files
        && show.views.by_file
    {
        notes.truncated(show.top, files.len(), "file");
    }
    if let Some(sections) = &report.sections
        && show.views.by_section
    {
        notes.truncated(show.top, sections.len(), "section");
    }
    notes.census(report);
    notes.corpora(report);
    notes.unit(report.unit);
    if !show.views.any() {
        notes.views(true);
    }
    notes
}

/// Documentation prose against the code it documents. A comparator for the
/// before-and-after loop, never a threshold: the direction is what matters.
///
/// It complements the headline rather than repeating it. Move prose from a
/// comment into a document and the headline holds still, by design — this line
/// is what rises, and says where the prose went.
fn docs_line(report: &Report) -> String {
    let unit = report.unit;
    let Some(docs) = report.cohort(DOCS_COHORT) else {
        return String::new();
    };
    let prose = docs.counts.prose(unit);
    if prose == 0 {
        return String::new();
    }

    let mut line = format!("  docs prose {} {}", thousands(prose), unit.label());
    if let Some(code) = report
        .cohort(SOURCE_COHORT)
        .map(|c| c.counts.code(unit))
        .filter(|code| *code > 0)
    {
        line.push_str(&format!(
            " — {:.1}% of source code",
            prose as f64 / code as f64 * 100.0
        ));
    }
    let local: u64 = docs
        .languages
        .iter()
        .filter(|l| l.provenance == crate::walk::Provenance::Local)
        .map(|l| l.counts.prose(unit))
        .sum();
    if local > 0 {
        line.push_str(&format!(
            ", {:.1}% of it local-only",
            local as f64 / prose as f64 * 100.0
        ));
    }
    line.push('\n');
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::{SCHEMA_VERSION, Totals};
    use crate::span::Unit;
    use std::collections::BTreeMap;

    fn empty() -> Report {
        Report {
            schema_version: SCHEMA_VERSION,
            tool: "ernest".to_string(),
            unit: Unit::Chars,
            files_scanned: 0,
            files_skipped: 0,
            files_failed: 0,
            unsupported: BTreeMap::new(),
            ernestignore: Vec::new(),
            total: Totals::default(),
            cohorts: Vec::new(),
            files: None,
            sections: None,
        }
    }

    fn bare() -> Presentation {
        Presentation {
            views: Default::default(),
            top: 20,
        }
    }

    /// Nothing found is still a report, and it used to carry a stray blank line
    /// where the unrequested table would have gone.
    #[test]
    fn a_report_of_nothing_has_no_gap_in_it() {
        let text = render(&empty(), bare());
        assert!(!text.contains("\n\n\n"), "{text:?}");
        assert!(text.ends_with('\n'));
    }

    #[test]
    fn a_bare_run_names_the_views_it_did_not_show() {
        assert!(render(&empty(), bare()).contains("--by file|section|cohort|language"));
    }

    #[test]
    fn asking_for_a_view_retires_the_menu() {
        let show = Presentation {
            views: crate::aggregate::Views {
                by_language: true,
                ..Default::default()
            },
            top: 20,
        };
        assert!(!render(&empty(), show).contains("--by file"));
    }

    #[test]
    fn the_value_format_carries_the_absence_of_a_density() {
        assert_eq!(value(&empty()), "n/a\n");
    }
}
