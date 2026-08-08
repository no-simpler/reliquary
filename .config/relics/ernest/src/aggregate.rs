//! Rolling per-file results up into the report the CLI prints or serialises.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analyze::analyze_file;
use crate::span::{Counts, Unit};
use crate::walk::Candidate;

pub const SCHEMA_VERSION: u32 = 1;

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
    pub density: Option<f64>,
    #[serde(flatten)]
    pub counts: Counts,
}

impl Report {
    pub fn headline(&self) -> Option<&CohortReport> {
        self.cohorts.iter().find(|c| c.cohort == HEADLINE_COHORT)
    }
}

struct Outcome {
    path: PathBuf,
    language: &'static str,
    cohort: &'static str,
    counts: Counts,
}

/// Analyze every candidate in parallel, then fold the results deterministically.
pub fn run(candidates: &[Candidate], unit: Unit, skipped: u64, by_file: bool) -> Report {
    let outcomes: Vec<Option<Outcome>> = candidates
        .par_iter()
        .map(|candidate| {
            let bytes = std::fs::read(&candidate.path).ok()?;
            // Byte offsets from tree-sitter are only char boundaries in valid
            // UTF-8, and a file that is not text is not prose either.
            let src = String::from_utf8(bytes).ok()?;
            let counts = analyze_file(&src, candidate.profile).ok()?;
            Some(Outcome {
                path: candidate.path.clone(),
                language: candidate.profile.language,
                cohort: candidate.profile.cohort.label(),
                counts,
            })
        })
        .collect();

    let failed = outcomes.iter().filter(|o| o.is_none()).count() as u64;
    let outcomes: Vec<Outcome> = outcomes.into_iter().flatten().collect();

    // BTreeMap keeps cohort and language ordering stable, so snapshots diff
    // cleanly instead of churning on hash order.
    let mut by_cohort: BTreeMap<&'static str, BTreeMap<&'static str, (Counts, u64)>> =
        BTreeMap::new();
    for outcome in &outcomes {
        let entry = by_cohort
            .entry(outcome.cohort)
            .or_default()
            .entry(outcome.language)
            .or_insert((Counts::default(), 0));
        entry.0.add(&outcome.counts);
        entry.1 += 1;
    }

    let cohorts = by_cohort
        .into_iter()
        .map(|(cohort, languages)| {
            let mut totals = Counts::default();
            let mut files = 0u64;
            let languages: Vec<LanguageReport> = languages
                .into_iter()
                .map(|(language, (counts, count))| {
                    totals.add(&counts);
                    files += count;
                    LanguageReport {
                        language: language.to_string(),
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

    let files = by_file.then(|| {
        let mut rows: Vec<FileReport> = outcomes
            .iter()
            .map(|o| FileReport {
                path: o.path.display().to_string(),
                language: o.language.to_string(),
                cohort: o.cohort.to_string(),
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

    Report {
        schema_version: SCHEMA_VERSION,
        tool: "ernest".to_string(),
        unit,
        files_scanned: outcomes.len() as u64,
        files_skipped: skipped,
        files_failed: failed,
        cohorts,
        files,
    }
}
