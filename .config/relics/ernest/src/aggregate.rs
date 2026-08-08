//! Rolling per-file results up into the report the CLI prints or serialises.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analyze::profiles::Cohort;
use crate::analyze::{analyze_file, analyze_sections};
use crate::span::{Counts, Unit};
use crate::walk::{Candidate, Provenance};

pub const SCHEMA_VERSION: u32 = 2;

/// The headline cohort. Prose-by-nature formats report separately.
pub const HEADLINE_COHORT: &str = "source";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub tool: String,
    pub unit: Unit,
    pub files_scanned: u64,
    pub files_skipped: u64,
    pub files_failed: u64,
    /// Never summed together — a documentation format would swamp a source
    /// denominator. The headline figure is the `source` cohort.
    pub cohorts: Vec<CohortReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileReport>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<SectionReport>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CohortReport {
    pub cohort: String,
    pub density: Option<f64>,
    pub files: u64,
    #[serde(flatten)]
    pub counts: Counts,
    pub languages: Vec<LanguageReport>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageReport {
    pub language: String,
    pub provenance: Provenance,
    pub density: Option<f64>,
    pub files: u64,
    #[serde(flatten)]
    pub counts: Counts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileReport {
    pub path: String,
    pub language: String,
    pub cohort: String,
    pub provenance: Provenance,
    pub density: Option<f64>,
    #[serde(flatten)]
    pub counts: Counts,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SectionReport {
    pub path: String,
    pub section: String,
    pub cohort: String,
    pub density: Option<f64>,
    #[serde(flatten)]
    pub counts: Counts,
}

impl Report {
    pub fn headline(&self) -> Option<&CohortReport> {
        self.cohorts.iter().find(|c| c.cohort == HEADLINE_COHORT)
    }

    pub fn cohort(&self, name: &str) -> Option<&CohortReport> {
        self.cohorts.iter().find(|c| c.cohort == name)
    }
}

/// Which extra rows to carry. Both are off by default: the summary is what a
/// bare run is for, and the rows are what a de-prosing pass drills into.
#[derive(Debug, Clone, Copy, Default)]
pub struct Views {
    pub by_file: bool,
    pub by_section: bool,
}

/// Accumulated counts and file tally for one `(provenance, language)` row.
type Rows = BTreeMap<(Provenance, &'static str), (Counts, u64)>;

struct Outcome {
    path: PathBuf,
    language: &'static str,
    cohort: &'static str,
    provenance: Provenance,
    counts: Counts,
    sections: Vec<(String, Counts)>,
}

/// Analyze every candidate in parallel, then fold the results deterministically.
pub fn run(candidates: &[Candidate], unit: Unit, skipped: u64, views: Views) -> Report {
    let outcomes: Vec<Option<Outcome>> = candidates
        .par_iter()
        .map(|candidate| {
            let bytes = std::fs::read(&candidate.path).ok()?;
            // Byte offsets from tree-sitter are only char boundaries in valid
            // UTF-8, and a file that is not text is not prose either.
            let src = String::from_utf8(bytes).ok()?;
            // Sections describe a document; a source file has none worth naming.
            let wants_sections = views.by_section && candidate.profile.cohort == Cohort::Docs;
            let (counts, sections) = if wants_sections {
                analyze_sections(&src, candidate.profile).ok()?
            } else {
                (analyze_file(&src, candidate.profile).ok()?, Vec::new())
            };
            Some(Outcome {
                path: candidate.path.clone(),
                language: candidate.profile.language,
                cohort: candidate.profile.cohort.label(),
                provenance: candidate.provenance,
                counts,
                sections,
            })
        })
        .collect();

    let failed = outcomes.iter().filter(|o| o.is_none()).count() as u64;
    let outcomes: Vec<Outcome> = outcomes.into_iter().flatten().collect();

    // BTreeMap keeps cohort, provenance and language ordering stable, so
    // snapshots diff cleanly instead of churning on hash order.
    let mut by_cohort: BTreeMap<&'static str, Rows> = BTreeMap::new();
    for outcome in &outcomes {
        let entry = by_cohort
            .entry(outcome.cohort)
            .or_default()
            .entry((outcome.provenance, outcome.language))
            .or_insert((Counts::default(), 0));
        entry.0.add(&outcome.counts);
        entry.1 += 1;
    }

    let mut cohorts: Vec<CohortReport> = by_cohort
        .into_iter()
        .map(|(cohort, rows)| {
            let mut totals = Counts::default();
            let mut files = 0u64;
            let languages: Vec<LanguageReport> = rows
                .into_iter()
                .map(|((provenance, language), (counts, count))| {
                    totals.add(&counts);
                    files += count;
                    LanguageReport {
                        language: language.to_string(),
                        provenance,
                        density: counts.density(unit),
                        files: count,
                        counts,
                    }
                })
                .collect();
            CohortReport {
                cohort: cohort.to_string(),
                density: totals.density(unit),
                files,
                counts: totals,
                languages,
            }
        })
        .collect();
    // The headline cohort leads; the rest keep their stable order behind it.
    cohorts.sort_by_key(|c| (c.cohort != HEADLINE_COHORT, c.cohort.clone()));

    let files = views.by_file.then(|| {
        let mut rows: Vec<FileReport> = outcomes
            .iter()
            .map(|o| FileReport {
                path: o.path.display().to_string(),
                language: o.language.to_string(),
                cohort: o.cohort.to_string(),
                provenance: o.provenance,
                density: o.counts.density(unit),
                counts: o.counts,
            })
            .collect();
        // Most prose first: the rows worth acting on come first, and the path
        // tiebreak keeps the order stable across runs.
        rows.sort_by(|a, b| {
            b.counts
                .prose(unit)
                .cmp(&a.counts.prose(unit))
                .then_with(|| a.path.cmp(&b.path))
        });
        rows
    });

    let sections = views.by_section.then(|| {
        let mut rows: Vec<SectionReport> = outcomes
            .iter()
            .flat_map(|o| {
                o.sections.iter().map(|(section, counts)| SectionReport {
                    path: o.path.display().to_string(),
                    section: section.clone(),
                    cohort: o.cohort.to_string(),
                    density: counts.density(unit),
                    counts: *counts,
                })
            })
            .collect();
        rows.sort_by(|a, b| {
            b.counts
                .prose(unit)
                .cmp(&a.counts.prose(unit))
                .then_with(|| a.path.cmp(&b.path))
                .then_with(|| a.section.cmp(&b.section))
        });
        rows
    });

    Report {
        schema_version: SCHEMA_VERSION,
        tool: "ernest".to_string(),
        unit,
        files_scanned: outcomes.len() as u64,
        files_skipped: skipped,
        files_failed: failed,
        cohorts,
        files,
        sections,
    }
}
