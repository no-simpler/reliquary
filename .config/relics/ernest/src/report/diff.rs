//! Comparing two snapshots — the workflow ernest exists for. Measure before,
//! measure after, then look at where the difference came from.
//!
//! Same three registers as the measurement report, and the same `--by`: a bare
//! diff is the delta, and the rows that produced it are asked for.

use std::fmt::Write;

use std::collections::BTreeMap;

use anyhow::{Result, bail};

use crate::aggregate::{CohortReport, DOCS_COHORT, LanguageReport, Report, SOURCE_COHORT};
use crate::span::{Counts, Unit};
use crate::walk::Provenance;

use super::notes::Notes;
use super::table::{Column, Table};
use super::{Blocks, Presentation, Verbosity, count, percent, percent_delta, signed, thousands};

/// The whole comparison, rendered.
///
/// # Errors
///
/// When the two snapshots were measured in different units, which makes every
/// row of the comparison meaningless rather than merely odd.
pub fn render(before: &Report, after: &Report, show: Presentation) -> Result<String> {
    let unit = same_unit(before, after)?;
    let mut blocks = Blocks::default();
    let mut notes = Notes::default();

    blocks.push(headline(before, after, unit));
    blocks.push(breakdown(before, after, show, unit));

    if show.views.by_file || show.views.by_section {
        same_ranking(before, after)?;
    }

    if show.views.by_file {
        match (&before.files, &after.files) {
            (Some(b), Some(a)) => blocks.push(movers(
                "file",
                show,
                &mut notes,
                b.iter()
                    .map(|f| (f.path.clone(), f.counts.prose(unit), f.density)),
                a.iter()
                    .map(|f| (f.path.clone(), f.counts.prose(unit), f.density)),
            )),
            _ => notes.push("per-file movement needs both snapshots taken with --by file"),
        }
    }

    if show.views.by_section {
        match (&before.sections, &after.sections) {
            (Some(b), Some(a)) => {
                let key = |path: &str, section: &str| format!("{path}#{section}");
                blocks.push(movers(
                    "section",
                    show,
                    &mut notes,
                    b.iter()
                        .map(|s| (key(&s.path, &s.section), s.counts.prose(unit), s.density)),
                    a.iter()
                        .map(|s| (key(&s.path, &s.section), s.counts.prose(unit), s.density)),
                ));
            }
            _ => notes.push("per-section movement needs both snapshots taken with --by section"),
        }
    }

    // Quiet keeps what qualifies a block that was printed — the truncation notes
    // `movers` collected above, and the two lines explaining a block that could
    // not be — and drops what comments on the run.
    if show.verbosity == Verbosity::Quiet {
        blocks.push(notes.render());
        return Ok(blocks.render());
    }

    // A density delta is only about prose if the corpus behind it held still.
    // Files arriving or leaving move the figure for a reason that is not work.
    if before.files_scanned != after.files_scanned {
        notes.push(format!(
            "{} measured before, {} after — the corpus changed too",
            count(before.files_scanned, "file"),
            thousands(after.files_scanned),
        ));
    }
    notes.corpora(after, show.verbosity);
    notes.unit(unit);
    if !show.views.any() {
        notes.views(false);
    }
    blocks.push(notes.render());

    Ok(blocks.render())
}

/// The density alone, as a change in percentage points. What `--format value`
/// writes on this side, so a gate reading a diff speaks the dialect one reading
/// a measurement does.
///
/// # Errors
///
/// When the two snapshots were measured in different units.
pub fn quiet(before: &Report, after: &Report) -> Result<String> {
    same_unit(before, after)?;
    Ok(format!(
        "{}\n",
        percent_delta(before.headline().density, after.headline().density)
    ))
}

/// Two snapshots whose ranked views cover different sets cannot be compared row
/// by row: every file inside one scope and outside the other reports as a
/// full-weight arrival or deletion, which is a silently wrong answer of exactly
/// the kind `same_unit` exists to prevent.
///
/// Checked only where a ranked view was asked for. The headline is unscoped by
/// construction, so comparing a scoped snapshot against an unscoped one at the
/// headline is *correct*, and refusing it would be over-strict — which is why
/// `quiet`, being headline-only, never calls this.
fn same_ranking(before: &Report, after: &Report) -> Result<()> {
    if before.ranking.asked != after.ranking.asked {
        bail!(
            "snapshots rank different scopes ({} and {}) — re-measure both the same way",
            before.ranking.label(),
            after.ranking.label()
        );
    }
    Ok(())
}

fn same_unit(before: &Report, after: &Report) -> Result<Unit> {
    if before.unit != after.unit {
        bail!(
            "snapshots use different units ({} and {}) — re-measure both the same way",
            before.unit.label(),
            after.unit.label()
        );
    }
    Ok(after.unit)
}

fn headline(before: &Report, after: &Report, unit: Unit) -> String {
    let (b, a) = (before.headline(), after.headline());
    if before.cohorts.is_empty() && after.cohorts.is_empty() {
        return "prose density  n/a   (no supported files found)\n".to_owned();
    }
    let prose_delta =
        crate::span::delta(a.counts.prose(unit)) - crate::span::delta(b.counts.prose(unit));
    format!(
        "prose density  {} -> {}   ({} pp,  prose {} {})\n{}",
        percent(b.density),
        percent(a.density),
        percent_delta(b.density, a.density),
        signed(prose_delta),
        unit.label(),
        docs_line(before, after, unit),
    )
}

/// Documentation prose against the code it documents, before and after. The
/// headline holds still on a pure relocation by design, so this is the line that
/// moves and says where the prose went — which makes a comparison the place it
/// earns its keep most.
fn docs_line(before: &Report, after: &Report, unit: Unit) -> String {
    let prose = |report: &Report| {
        report
            .cohort(DOCS_COHORT)
            .map_or(0, |docs| docs.counts.prose(unit))
    };
    let (b, a) = (prose(before), prose(after));
    if b == 0 && a == 0 {
        return String::new();
    }

    let mut line = format!(
        "  docs prose {} -> {} {}",
        thousands(b),
        thousands(a),
        unit.label()
    );
    let against = |report: &Report, prose: u64| {
        report
            .cohort(SOURCE_COHORT)
            .map(|c| c.counts.code(unit))
            .filter(|code| *code > 0)
            .map(|code| crate::span::approx(prose) / crate::span::approx(code) * 100.0)
    };
    if let (Some(b), Some(a)) = (against(before, b), against(after, a)) {
        let _ = write!(line, " — {b:.1}% -> {a:.1}% of source code");
    }
    line.push('\n');
    line
}

/// Relocation is the case this table exists to expose: a headline holding still
/// above two cohort rows moving in opposite directions.
fn breakdown(before: &Report, after: &Report, show: Presentation, unit: Unit) -> String {
    if !(show.views.by_cohort || show.views.by_language) {
        return String::new();
    }
    let languages = show.views.by_language;

    // Cohorts, provenances and languages present in either snapshot: something
    // that disappeared entirely is exactly the kind of movement worth seeing.
    let mut cohorts = union(
        before.cohorts.iter().map(|c| c.cohort.clone()),
        after.cohorts.iter().map(|c| c.cohort.clone()),
    );
    if cohorts.is_empty() {
        return String::new();
    }
    // Source leads, as it does in the reports being compared.
    cohorts.sort_by_key(|c| (c != SOURCE_COHORT, c.clone()));

    // Provenance only ever varies on a language row, and a column sizes to its
    // header even when every cell under it is blank.
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
        Column::right("pp delta"),
        Column::right("prose"),
        Column::right("prose delta"),
    ]);
    let mut table = Table::new(columns);

    // A roll-up row in a table that carries the column still needs its blank
    // cell; a table without the column needs no cell at all.
    let blank = languages.then_some("");

    let (b, a) = (before.headline(), after.headline());
    table.push(
        0,
        row(
            "total", blank, a.density, b.density, a.counts, b.counts, unit,
        ),
    );
    for cohort in cohorts {
        let b = before.cohorts.iter().find(|c| c.cohort == cohort);
        let a = after.cohorts.iter().find(|c| c.cohort == cohort);
        table.push(
            1,
            row(
                &cohort,
                blank,
                a.and_then(|c| c.density),
                b.and_then(|c| c.density),
                a.map_or(Counts::default(), |c| c.counts),
                b.map_or(Counts::default(), |c| c.counts),
                unit,
            ),
        );
        if !languages {
            continue;
        }

        let rows = union(
            b.into_iter().flat_map(|c| {
                c.languages
                    .iter()
                    .map(|l| (l.language.clone(), l.provenance))
            }),
            a.into_iter().flat_map(|c| {
                c.languages
                    .iter()
                    .map(|l| (l.language.clone(), l.provenance))
            }),
        );
        for (language, provenance) in rows {
            let lb = language_row(b, provenance, &language);
            let la = language_row(a, provenance, &language);
            table.push(
                2,
                row(
                    &language,
                    Some(provenance.label()),
                    la.and_then(|l| l.density),
                    lb.and_then(|l| l.density),
                    la.map_or(Counts::default(), |l| l.counts),
                    lb.map_or(Counts::default(), |l| l.counts),
                    unit,
                ),
            );
        }
    }
    table.render()
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
    show: Presentation,
    notes: &mut Notes,
    before: impl Iterator<Item = (String, u64, Option<f64>)>,
    after: impl Iterator<Item = (String, u64, Option<f64>)>,
) -> String {
    let mut moved: BTreeMap<String, (i64, Option<f64>)> = BTreeMap::new();
    for (key, prose, _) in before {
        moved.entry(key).or_insert((0, None)).0 -= crate::span::delta(prose);
    }
    for (key, prose, density) in after {
        let entry = moved.entry(key).or_insert((0, None));
        entry.0 += crate::span::delta(prose);
        entry.1 = density;
    }
    let mut rows: Vec<(String, i64, Option<f64>)> = moved
        .into_iter()
        .filter(|(_, (delta, _))| *delta != 0)
        .map(|(key, (delta, density))| (key, delta, density))
        .collect();
    rows.sort_by(|x, y| y.1.abs().cmp(&x.1.abs()).then_with(|| x.0.cmp(&y.0)));

    let mut table = Table::new(vec![
        Column::right("prose delta"),
        Column::right("density"),
        Column::left(noun),
    ]);
    for (key, delta, density) in rows.iter().take(show.top) {
        table.push(0, vec![signed(*delta), percent(*density), key.clone()]);
    }
    notes.truncated(show.top, rows.len(), noun);
    table.render()
}

/// A row is only comparable within its provenance: local prose and shared prose
/// move for different reasons.
fn row(
    label: &str,
    provenance: Option<&str>,
    after: Option<f64>,
    before: Option<f64>,
    after_counts: Counts,
    before_counts: Counts,
    unit: Unit,
) -> Vec<String> {
    let mut row = vec![label.to_owned()];
    // `None` is a table without the column at all; a roll-up row in a table that
    // has one still needs its blank cell.
    if let Some(provenance) = provenance {
        row.push(provenance.to_owned());
    }
    row.extend([
        percent(after),
        percent_delta(before, after),
        thousands(after_counts.prose(unit)),
        signed(
            crate::span::delta(after_counts.prose(unit))
                - crate::span::delta(before_counts.prose(unit)),
        ),
    ]);
    row
}

/// Ordered union of two key sequences, so a key in only one snapshot still shows.
fn union<T: Ord>(left: impl Iterator<Item = T>, right: impl Iterator<Item = T>) -> Vec<T> {
    let mut keys: Vec<T> = left.chain(right).collect();
    keys.sort();
    keys.dedup();
    keys
}
