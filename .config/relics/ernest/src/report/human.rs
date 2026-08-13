//! The default report.

use crate::aggregate::{HEADLINE_COHORT, Report};
use crate::span::Unit;

use super::table::{Column, Table};
use super::{percent, thousands};

/// Rows shown before the rest are summarised away.
const ROWS: usize = 20;

/// The cohort documentation formats land in. Its density sits near 100% in any
/// real project, so volume is what carries the signal.
const DOCS_COHORT: &str = "docs";

pub fn render(report: &Report) -> String {
    let unit = report.unit;
    let mut out = String::new();

    match report.headline() {
        Some(headline) => {
            let base = headline.counts.prose(unit) + headline.counts.code(unit);
            out.push_str(&format!(
                "prose density  {}   (prose {} / base {} {})\n",
                percent(headline.density),
                thousands(headline.counts.prose(unit)),
                thousands(base),
                unit.label(),
            ));
        }
        // No source cohort is not the same as nothing measured: a documentation
        // tree is a perfectly good thing to point ernest at.
        None if report.cohorts.is_empty() => {
            out.push_str("prose density  n/a   (no supported files found)\n")
        }
        None => out.push_str("prose density  n/a   (nothing in the source cohort)\n"),
    }

    out.push('\n');
    let mut breakdown = Table::new(vec![
        Column::left("cohort / language"),
        Column::left("provenance"),
        Column::right("density"),
        Column::right("prose"),
        Column::right("code"),
        Column::right("files"),
    ]);
    for cohort in &report.cohorts {
        // The roll-up leads its group and the languages indent under it, so the
        // sum is never mistaken for one more row of the breakdown.
        breakdown.push(
            0,
            vec![
                cohort.cohort.clone(),
                String::new(),
                percent(cohort.density),
                thousands(cohort.counts.prose(unit)),
                thousands(cohort.counts.code(unit)),
                thousands(cohort.files),
            ],
        );
        for language in &cohort.languages {
            breakdown.push(
                1,
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
    out.push_str(&breakdown.render());

    out.push_str(&docs_line(report));

    if let Some(files) = &report.files {
        out.push('\n');
        let mut table = rows_table("file");
        for file in files.iter().take(ROWS) {
            table.push(
                0,
                vec![
                    percent(file.density),
                    thousands(file.counts.prose(unit)),
                    thousands(file.counts.code(unit)),
                    file.path.clone(),
                ],
            );
        }
        out.push_str(&table.render());
        // Say what was left out; a truncated list that looks complete is worse
        // than no list.
        if files.len() > ROWS {
            out.push_str(&format!(
                "  … {} more files, ordered by prose; --json carries them all\n",
                thousands((files.len() - ROWS) as u64),
            ));
        }
    }

    if let Some(sections) = &report.sections {
        out.push('\n');
        let mut table = rows_table("section");
        for section in sections.iter().take(ROWS) {
            table.push(
                0,
                vec![
                    percent(section.density),
                    thousands(section.counts.prose(unit)),
                    thousands(section.counts.code(unit)),
                    format!("{}#{}", section.path, section.section),
                ],
            );
        }
        out.push_str(&table.render());
        if sections.len() > ROWS {
            out.push_str(&format!(
                "  … {} more sections, ordered by prose; --json carries them all\n",
                thousands((sections.len() - ROWS) as u64),
            ));
        }
    }

    out.push('\n');
    let uninteresting: u64 = report.cohorts.iter().map(|c| c.counts.ignored(unit)).sum();
    if uninteresting > 0 {
        out.push_str(&format!(
            "  {} {} uninteresting — open tags, shebangs, tooling directives, generated regions\n",
            thousands(uninteresting),
            unit.label(),
        ));
    }
    out.push_str(&format!(
        "  {} files measured, {} skipped as unsupported",
        thousands(report.files_scanned),
        thousands(report.files_skipped),
    ));
    if report.files_failed > 0 {
        out.push_str(&format!(", {} unreadable", thousands(report.files_failed)));
    }
    out.push('\n');

    if unit == Unit::Lines {
        out.push_str("  counting lines; a line belongs to whichever class holds most of it\n");
    }

    out
}

/// The shape both drill-down views share: measurements first, then the key,
/// which is long and of no fixed width.
fn rows_table(key: &'static str) -> Table {
    Table::new(vec![
        Column::right("density"),
        Column::right("prose"),
        Column::right("code"),
        Column::left(key),
    ])
}

/// Documentation prose against the code it documents. A comparator for the
/// before-and-after loop, never a threshold: the direction is what matters.
fn docs_line(report: &Report) -> String {
    let unit = report.unit;
    let Some(docs) = report.cohort(DOCS_COHORT) else {
        return String::new();
    };
    let prose = docs.counts.prose(unit);
    if prose == 0 {
        return String::new();
    }

    let mut line = format!("\n  docs prose {} {}", thousands(prose), unit.label());
    if let Some(code) = report
        .cohort(HEADLINE_COHORT)
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
