//! Comparing two snapshots — the workflow ernest exists for. Measure before,
//! measure after, then look at where the difference came from.

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::aggregate::Report;
use crate::span::{Counts, Unit};

use super::{percent, percent_delta, signed, thousands};

/// Per-file movers shown before the rest are summarised away.
const FILE_ROWS: usize = 20;

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

    // Cohorts and languages present in either snapshot: something that
    // disappeared entirely is exactly the kind of movement worth seeing.
    let cohorts: Vec<&String> = union(
        before.cohorts.iter().map(|c| &c.cohort),
        after.cohorts.iter().map(|c| &c.cohort),
    );

    for cohort in cohorts {
        let b = before.cohorts.iter().find(|c| &c.cohort == cohort);
        let a = after.cohorts.iter().find(|c| &c.cohort == cohort);
        out.push('\n');
        out.push_str(&format!(
            "  {:<10} {:>9} {:>9} {:>12} {:>12}\n",
            cohort, "density", "delta", "prose", "delta"
        ));

        let languages: Vec<&String> = union(
            b.into_iter().flat_map(|c| c.languages.iter().map(|l| &l.language)),
            a.into_iter().flat_map(|c| c.languages.iter().map(|l| &l.language)),
        );
        for language in languages {
            let lb = b.and_then(|c| c.languages.iter().find(|l| &l.language == language));
            let la = a.and_then(|c| c.languages.iter().find(|l| &l.language == language));
            out.push_str(&row(
                language,
                la.and_then(|l| l.density),
                lb.and_then(|l| l.density),
                la.map_or(Counts::default(), |l| l.counts),
                lb.map_or(Counts::default(), |l| l.counts),
                unit,
            ));
        }
        out.push_str(&row(
            "total",
            a.and_then(|c| c.density),
            b.and_then(|c| c.density),
            a.map_or(Counts::default(), |c| c.counts),
            b.map_or(Counts::default(), |c| c.counts),
            unit,
        ));
    }

    if let (Some(bf), Some(af)) = (&before.files, &after.files) {
        let mut moved: BTreeMap<&str, (i64, Option<f64>)> = BTreeMap::new();
        for file in bf {
            moved.insert(file.path.as_str(), (-(file.counts.prose(unit) as i64), None));
        }
        for file in af {
            let entry = moved.entry(file.path.as_str()).or_insert((0, None));
            entry.0 += file.counts.prose(unit) as i64;
            entry.1 = file.density;
        }
        let mut rows: Vec<(&str, i64, Option<f64>)> = moved
            .into_iter()
            .filter(|(_, (delta, _))| *delta != 0)
            .map(|(path, (delta, density))| (path, delta, density))
            .collect();
        rows.sort_by(|x, y| y.1.abs().cmp(&x.1.abs()).then_with(|| x.0.cmp(y.0)));

        if !rows.is_empty() {
            out.push('\n');
            out.push_str(&format!(
                "  {:>12} {:>9}  {}\n",
                "prose delta", "density", "file"
            ));
            for (path, delta, density) in rows.iter().take(FILE_ROWS) {
                out.push_str(&format!(
                    "  {:>12} {:>9}  {}\n",
                    signed(*delta),
                    percent(*density),
                    path
                ));
            }
            if rows.len() > FILE_ROWS {
                out.push_str(&format!(
                    "  … {} more files moved\n",
                    thousands((rows.len() - FILE_ROWS) as u64)
                ));
            }
        }
    } else {
        out.push_str("\n  per-file movement needs both snapshots taken with --by-file\n");
    }

    Ok(out)
}

fn row(
    label: &str,
    after: Option<f64>,
    before: Option<f64>,
    after_counts: Counts,
    before_counts: Counts,
    unit: Unit,
) -> String {
    format!(
        "  {:<10} {:>9} {:>9} {:>12} {:>12}\n",
        label,
        percent(after),
        percent_delta(before, after),
        thousands(after_counts.prose(unit)),
        signed(after_counts.prose(unit) as i64 - before_counts.prose(unit) as i64),
    )
}

/// Ordered union of two key sequences, so a key in only one snapshot still shows.
fn union<'a>(
    left: impl Iterator<Item = &'a String>,
    right: impl Iterator<Item = &'a String>,
) -> Vec<&'a String> {
    let mut keys: Vec<&String> = left.chain(right).collect();
    keys.sort();
    keys.dedup();
    keys
}
