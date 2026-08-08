//! The default report.

use crate::aggregate::{HEADLINE_COHORT, Report};
use crate::span::Unit;

use super::{percent, thousands};

/// Per-file rows shown before the rest are summarised away.
const FILE_ROWS: usize = 20;

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
        None => out.push_str("prose density  n/a   (no supported files found)\n"),
    }

    for cohort in &report.cohorts {
        out.push('\n');
        if cohort.cohort != HEADLINE_COHORT {
            out.push_str(&format!(
                "  {} cohort — reported separately, never folded into the headline\n",
                cohort.cohort
            ));
        }
        out.push_str(&format!(
            "  {:<10} {:>9} {:>12} {:>12} {:>8}\n",
            "language", "density", "prose", "code", "files"
        ));
        for language in &cohort.languages {
            out.push_str(&format!(
                "  {:<10} {:>9} {:>12} {:>12} {:>8}\n",
                language.language,
                percent(language.density),
                thousands(language.counts.prose(unit)),
                thousands(language.counts.code(unit)),
                thousands(language.files),
            ));
        }
        if cohort.languages.len() > 1 {
            out.push_str(&format!(
                "  {:<10} {:>9} {:>12} {:>12} {:>8}\n",
                "total",
                percent(cohort.density),
                thousands(cohort.counts.prose(unit)),
                thousands(cohort.counts.code(unit)),
                thousands(cohort.files),
            ));
        }
    }

    if let Some(files) = &report.files {
        out.push('\n');
        out.push_str(&format!(
            "  {:<9} {:>10} {:>12}  {}\n",
            "density", "prose", "code", "file"
        ));
        for file in files.iter().take(FILE_ROWS) {
            out.push_str(&format!(
                "  {:<9} {:>10} {:>12}  {}\n",
                percent(file.density),
                thousands(file.counts.prose(unit)),
                thousands(file.counts.code(unit)),
                file.path,
            ));
        }
        // Say what was left out; a truncated list that looks complete is worse
        // than no list.
        if files.len() > FILE_ROWS {
            out.push_str(&format!(
                "  … {} more files, ordered by prose; --json carries them all\n",
                thousands((files.len() - FILE_ROWS) as u64),
            ));
        }
    }

    out.push('\n');
    let uninteresting: u64 = report.cohorts.iter().map(|c| c.counts.ignored(unit)).sum();
    if uninteresting > 0 {
        out.push_str(&format!(
            "  {} {} uninteresting — open tags, shebangs, tooling directives, document markers\n",
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
        out.push_str(&format!(
            ", {} unreadable",
            thousands(report.files_failed)
        ));
    }
    out.push('\n');

    if unit == Unit::Lines {
        out.push_str("  counting lines; a line belongs to whichever class holds most of it\n");
    }

    out
}
