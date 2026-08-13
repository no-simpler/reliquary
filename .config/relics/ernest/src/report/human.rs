//! The default report.

use crate::aggregate::{Report, SOURCE_COHORT};
use crate::span::Unit;

use super::table::{Column, Table};
use super::{percent, thousands};

/// Rows shown before the rest are summarised away.
const ROWS: usize = 20;

/// Extensions named before the rest are summarised away.
const GAPS: usize = 6;

/// The cohort documentation formats land in. Its density sits near 100% in any
/// real project, so volume is what carries its own signal — but its prose still
/// counts toward the headline.
const DOCS_COHORT: &str = "docs";

pub fn render(report: &Report) -> String {
    let unit = report.unit;
    let mut out = String::new();

    let total = report.headline();
    if report.cohorts.is_empty() {
        out.push_str("prose density  n/a   (no supported files found)\n");
    } else {
        let base = total.counts.prose(unit) + total.counts.code(unit);
        out.push_str(&format!(
            "prose density  {}   (prose {} / base {} {})\n",
            percent(total.density),
            thousands(total.counts.prose(unit)),
            thousands(base),
            unit.label(),
        ));
    }

    out.push('\n');
    let mut breakdown = Table::new(vec![
        Column::left("total / cohort / language"),
        Column::left("provenance"),
        Column::right("density"),
        Column::right("prose"),
        Column::right("code"),
        Column::right("files"),
    ]);
    // The roll-up leads its group and its parts indent under it, so the table
    // visibly sums to the headline rather than sitting beside it.
    if !report.cohorts.is_empty() {
        breakdown.push(
            0,
            vec![
                "total".to_string(),
                String::new(),
                percent(total.density),
                thousands(total.counts.prose(unit)),
                thousands(total.counts.code(unit)),
                thousands(total.files),
            ],
        );
    }
    for cohort in &report.cohorts {
        breakdown.push(
            1,
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
    let uninteresting = report.headline().counts.ignored(unit);
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
    out.push_str(&unsupported_line(report));

    for path in &report.ernestignore {
        out.push_str(&format!("  {path} applied — a declared corpus is not measured\n"));
    }

    if unit == Unit::Lines {
        out.push_str("  counting lines; a line belongs to whichever class holds most of it\n");
    }

    out
}

/// Which extensions the skipped files were. The headline sums every cohort, so
/// an unwritten profile pulls it toward whichever cohort *is* covered — this is
/// what stops that reading as a measurement, and it names the profile to write.
fn unsupported_line(report: &Report) -> String {
    if report.unsupported.is_empty() {
        return String::new();
    }
    let mut gaps: Vec<(&String, &u64)> = report.unsupported.iter().collect();
    gaps.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    let named: Vec<String> = gaps
        .iter()
        .take(GAPS)
        .map(|(ext, n)| format!("{ext} {}", thousands(**n)))
        .collect();
    let mut line = format!("  unsupported: {}", named.join(", "));
    if gaps.len() > GAPS {
        line.push_str(&format!(", and {} more", thousands((gaps.len() - GAPS) as u64)));
    }
    line.push('\n');
    line
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

    let mut line = format!("\n  docs prose {} {}", thousands(prose), unit.label());
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
