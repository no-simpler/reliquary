//! Rolling per-file results up into the report the CLI prints or serialises.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analyze::profiles::Cohort;
use crate::analyze::{analyze_file, analyze_sections};
use crate::span::{Counts, Unit};
use crate::walk::{Provenance, Survey};

pub const SCHEMA_VERSION: u32 = 3;

/// The cohort holding the code that documentation documents. It leads the
/// breakdown and is the base the docs comparator reads against — but it is no
/// longer the headline, which sums every cohort.
pub const SOURCE_COHORT: &str = "source";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub tool: String,
    pub unit: Unit,
    pub files_scanned: u64,
    pub files_skipped: u64,
    pub files_failed: u64,
    /// Files no profile claimed, by extension. The headline sums every cohort,
    /// so an unwritten profile skews it; this is what makes that visible.
    #[serde(default)]
    pub unsupported: BTreeMap<String, u64>,
    /// `.ernestignore` files that were in effect.
    #[serde(default)]
    pub ernestignore: Vec<String>,
    /// Every cohort summed. The headline: prose is prose wherever it lives, so
    /// moving it between a comment and a document must not move the number.
    pub total: Totals,
    /// How the total decomposes. A breakdown, not a barrier — `docs` density
    /// alone sits near 100% in any real project and says little, but its prose
    /// is prose the reader pays for and belongs in the figure above.
    pub cohorts: Vec<CohortReport>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub files: Option<Vec<FileReport>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sections: Option<Vec<SectionReport>>,
}

/// The figure every cohort rolls up into.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct Totals {
    pub density: Option<f64>,
    pub files: u64,
    #[serde(flatten)]
    pub counts: Counts,
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
    pub fn headline(&self) -> &Totals {
        &self.total
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

/// Accumulated counts and file tally for one `(language, provenance)` row,
/// keyed in the order the report's columns read.
type Rows = BTreeMap<(&'static str, Provenance), (Counts, u64)>;

struct Outcome {
    path: PathBuf,
    language: &'static str,
    cohort: &'static str,
    provenance: Provenance,
    counts: Counts,
    sections: Vec<(String, Counts)>,
}

/// Analyze every candidate in parallel, then fold the results deterministically.
pub fn run(survey: &Survey, unit: Unit, views: Views) -> Report {
    let outcomes: Vec<Option<Outcome>> = survey
        .candidates
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
            .entry((outcome.language, outcome.provenance))
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
                .map(|((language, provenance), (counts, count))| {
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
    // Source leads; the rest keep their stable order behind it.
    cohorts.sort_by_key(|c| (c.cohort != SOURCE_COHORT, c.cohort.clone()));

    // Ratio of sums across every cohort, never a mean of their ratios — the
    // same rule that keeps a small file from dominating its language.
    let mut total = Totals::default();
    for cohort in &cohorts {
        total.counts.add(&cohort.counts);
        total.files += cohort.files;
    }
    total.density = total.counts.density(unit);

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
        files_skipped: survey.unsupported.values().sum(),
        files_failed: failed,
        unsupported: survey.unsupported.clone(),
        ernestignore: survey
            .ernestignore
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        total,
        cohorts,
        files,
        sections,
    }
}
