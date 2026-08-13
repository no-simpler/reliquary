//! Comparing two snapshots — the workflow ernest exists for. Measure before,
//! measure after, then look at where the difference came from.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::aggregate::{CohortReport, HEADLINE_COHORT, LanguageReport, Report};
use crate::span::{Counts, Unit};
use crate::walk::Provenance;

use super::table::{Column, Table};
use super::{percent, percent_delta, signed, thousands};

/// Movers shown before the rest are summarised away.
const ROWS: usize = 20;

pub fn render(before: &Report, after: &Report) -> Result<String> {
    if before.unit != after.unit {
        bail!(
            "snapshots use different units ({} and {}) — re-measure both the same way",
            before.unit.label(),
            after.unit.label()
        );
    }
    let unit = after.unit;
    let mut out = String::new();

    let (b, a) = (before.headline(), after.headline());
    if b.is_none() && a.is_none() {
        // A zero source delta beside a moving docs table reads as a bug.
        out.push_str("prose density  n/a   (nothing in the source cohort)\n");
    } else {
        let prose_delta = a.map_or(0, |c| c.counts.prose(unit) as i64)
            - b.map_or(0, |c| c.counts.prose(unit) as i64);
        out.push_str(&format!(
            "prose density  {} -> {}   ({} pp,  prose {} {})\n",
            percent(b.and_then(|c| c.density)),
            percent(a.and_then(|c| c.density)),
            percent_delta(b.and_then(|c| c.density), a.and_then(|c| c.density)),
            signed(prose_delta),
            unit.label(),
        ));
    }

    // Cohorts, provenances and languages present in either snapshot: something
    // that disappeared entirely is exactly the kind of movement worth seeing.
    let mut cohorts = union(
        before.cohorts.iter().map(|c| c.cohort.clone()),
        after.cohorts.iter().map(|c| c.cohort.clone()),
    );
    // The headline cohort leads, as it does in the reports being compared.
    cohorts.sort_by_key(|c| (c != HEADLINE_COHORT, c.clone()));

    let mut breakdown = Table::new(vec![
        Column::left("cohort / language"),
        Column::left("provenance"),
        Column::right("density"),
        Column::right("pp delta"),
        Column::right("prose"),
        Column::right("prose delta"),
    ]);
    for cohort in cohorts {
        let b = before.cohorts.iter().find(|c| c.cohort == cohort);
        let a = after.cohorts.iter().find(|c| c.cohort == cohort);
        breakdown.push(
            0,
            row(
                &cohort,
                "",
                a.and_then(|c| c.density),
                b.and_then(|c| c.density),
                a.map_or(Counts::default(), |c| c.counts),
                b.map_or(Counts::default(), |c| c.counts),
                unit,
            ),
        );

        let rows = union(
            b.into_iter()
                .flat_map(|c| c.languages.iter().map(|l| (l.language.clone(), l.provenance))),
            a.into_iter()
                .flat_map(|c| c.languages.iter().map(|l| (l.language.clone(), l.provenance))),
        );
        for (language, provenance) in rows {
            let lb = language_row(b, provenance, &language);
            let la = language_row(a, provenance, &language);
            breakdown.push(
                1,
                row(
                    &language,
                    provenance.label(),
                    la.and_then(|l| l.density),
                    lb.and_then(|l| l.density),
                    la.map_or(Counts::default(), |l| l.counts),
                    lb.map_or(Counts::default(), |l| l.counts),
                    unit,
                ),
            );
        }
    }
    out.push('\n');
    out.push_str(&breakdown.render());

    match (&before.files, &after.files) {
        (Some(bf), Some(af)) => out.push_str(&movers(
            "file",
            bf.iter().map(|f| (f.path.clone(), f.counts.prose(unit), f.density)),
            af.iter().map(|f| (f.path.clone(), f.counts.prose(unit), f.density)),
        )),
        _ => out.push_str("\n  per-file movement needs both snapshots taken with --by file\n"),
    }

    if let (Some(bs), Some(as_)) = (&before.sections, &after.sections) {
        let key = |path: &str, section: &str| format!("{path}#{section}");
        out.push_str(&movers(
            "section",
            bs.iter()
                .map(|s| (key(&s.path, &s.section), s.counts.prose(unit), s.density)),
            as_.iter()
                .map(|s| (key(&s.path, &s.section), s.counts.prose(unit), s.density)),
        ));
    }

    Ok(out)
}

fn language_row<'a>(
    cohort: Option<&'a CohortReport>,
    provenance: Provenance,
    language: &str,
) -> Option<&'a LanguageReport> {
    cohort.and_then(|c| {
        c.languages
            .iter()
            .find(|l| l.provenance == provenance && l.language == language)
    })
}

/// Rows whose prose moved, most movement first. A key present in only one
/// snapshot still shows, with its whole weight as the delta.
fn movers(
    noun: &'static str,
    before: impl Iterator<Item = (String, u64, Option<f64>)>,
    after: impl Iterator<Item = (String, u64, Option<f64>)>,
) -> String {
    let mut moved: BTreeMap<String, (i64, Option<f64>)> = BTreeMap::new();
    for (key, prose, _) in before {
        moved.entry(key).or_insert((0, None)).0 -= prose as i64;
    }
    for (key, prose, density) in after {
        let entry = moved.entry(key).or_insert((0, None));
        entry.0 += prose as i64;
        entry.1 = density;
    }
    let mut rows: Vec<(String, i64, Option<f64>)> = moved
        .into_iter()
        .filter(|(_, (delta, _))| *delta != 0)
        .map(|(key, (delta, density))| (key, delta, density))
        .collect();
    rows.sort_by(|x, y| y.1.abs().cmp(&x.1.abs()).then_with(|| x.0.cmp(&y.0)));

    if rows.is_empty() {
        return String::new();
    }

    let mut table = Table::new(vec![
        Column::right("prose delta"),
        Column::right("density"),
        Column::left(noun),
    ]);
    for (key, delta, density) in rows.iter().take(ROWS) {
        table.push(0, vec![signed(*delta), percent(*density), key.clone()]);
    }
    let mut out = format!("\n{}", table.render());
    if rows.len() > ROWS {
        out.push_str(&format!(
            "  … {} more {}s moved\n",
            thousands((rows.len() - ROWS) as u64),
            noun
        ));
    }
    out
}

/// A row is only comparable within its provenance: local prose and shared prose
/// move for different reasons.
fn row(
    label: &str,
    provenance: &str,
    after: Option<f64>,
    before: Option<f64>,
    after_counts: Counts,
    before_counts: Counts,
    unit: Unit,
) -> Vec<String> {
    vec![
        label.to_string(),
        provenance.to_string(),
        percent(after),
        percent_delta(before, after),
        thousands(after_counts.prose(unit)),
        signed(after_counts.prose(unit) as i64 - before_counts.prose(unit) as i64),
    ]
}

/// Ordered union of two key sequences, so a key in only one snapshot still shows.
fn union<T: Ord>(left: impl Iterator<Item = T>, right: impl Iterator<Item = T>) -> Vec<T> {
    let mut keys: Vec<T> = left.chain(right).collect();
    keys.sort();
    keys.dedup();
    keys
}
