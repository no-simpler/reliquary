//! Rolling per-file results up into the report the CLI prints or serialises.

use std::collections::BTreeMap;
use std::path::PathBuf;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::analyze::profiles::Cohort;
use crate::analyze::{Health, analyze};
use crate::report::Diagnostics;
use crate::span::{Counts, Unit};
use crate::walk::{Provenance, Survey};

pub const SCHEMA_VERSION: u32 = 4;

/// The cohort holding the code that documentation documents. It leads the
/// breakdown and is the base the docs comparator reads against — but it is no
/// longer the headline, which sums every cohort.
pub const SOURCE_COHORT: &str = "source";

/// The cohort documentation formats land in. Its density sits near 100% in any
/// real project, so volume is what carries its own signal — but its prose still
/// counts toward the headline.
pub const DOCS_COHORT: &str = "docs";

/// The snapshot, and everything a reader needs to know what produced it.
///
/// Every field here is an aggregate or a bounded list, and none of them varies
/// with a presentation flag: `--json -vvv` and `--json` write the same bytes.
/// An unbounded per-file list — every unsupported path in a `--scope all` run is
/// sixty thousand of them — belongs in `report::Diagnostics`, which is text-only,
/// so a snapshot cannot grow with the size of the tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub tool: String,
    pub unit: Unit,
    /// How far the walk reached, and what it narrowed to. A figure that does not
    /// name its scope cannot be reasoned about, and two snapshots taken at
    /// different ones used to compare as though they were one measurement.
    #[serde(default = "default_scope")]
    pub scope: String,
    #[serde(default)]
    pub lang: Option<String>,
    /// What was measured, in the order it was named.
    #[serde(default)]
    pub roots: Vec<String>,
    pub files_scanned: u64,
    pub files_skipped: u64,
    pub files_failed: u64,
    /// Files no profile claimed, by extension. The headline sums every cohort,
    /// so an unwritten profile skews it; this is what makes that visible.
    #[serde(default)]
    pub unsupported: BTreeMap<String, u64>,
    /// The paths behind `files_failed`, and why each one stopped. A count says a
    /// file was lost; this says which, so a file that quietly stops parsing is
    /// visible in a diff rather than only in a number that moved.
    #[serde(default)]
    pub failed: Vec<Failure>,
    /// `.ernestignore` files that were in effect.
    #[serde(default)]
    pub ernestignore: Vec<String>,
    /// How many files those declarations removed. Excluded paths are never
    /// measured, so their prose is unknown — but the count separates a declared
    /// test fixture from half a repository, which is what decides whether the
    /// exclusion is worth saying out loud.
    #[serde(default)]
    pub ernestignore_excluded: u64,
    /// Languages whose grammar could not read every file it was handed, by
    /// language. A grammar that fails still returns a tree and the rules still
    /// classify it, so without this a borrowed grammar's confusion reports as an
    /// ordinary row.
    #[serde(default)]
    pub grammar: BTreeMap<String, GrammarHealth>,
    /// Whether the ranked views below cover the whole measurement. Absent scope,
    /// `files` and `sections` are indistinguishable from a smaller repository —
    /// and a diff of a scoped snapshot against an unscoped one bills every
    /// out-of-scope file as a deletion.
    #[serde(default)]
    pub ranking: RankingScope,
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

/// A snapshot written before `scope` existed was taken at the default.
fn default_scope() -> String {
    "local".to_string()
}

/// One file the run could not measure, and what stopped it. Three failures used
/// to collapse into one count, which named none of them.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Failure {
    pub path: String,
    pub reason: String,
}

/// What one language's grammar made of the files it was handed. `files` is the
/// count that failed, of `measured` that were tried — the shape the hand sweeps
/// in `TODO.md` reported, because it is the one that reads as a proportion.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct GrammarHealth {
    pub files: u64,
    pub measured: u64,
    pub error_nodes: u64,
    pub missing_nodes: u64,
}

/// What the ranked views cover. `asked` is `None` when they cover the whole
/// measurement, and otherwise the scope as the caller spelled it — canonical, so
/// two snapshots can be told apart by a reader and by `ernest diff`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RankingScope {
    pub asked: Option<String>,
    pub ranked: u64,
    pub measured: u64,
}

impl RankingScope {
    /// How to name this scope in a refusal, so the two sides of a mismatch read
    /// as the different questions they are.
    pub fn label(&self) -> String {
        match &self.asked {
            Some(asked) => asked.clone(),
            None => "the whole measurement".to_string(),
        }
    }
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
    /// A report of nothing, for a caller that wants to name two fields and mean
    /// the defaults for the rest. Every field added here would otherwise be a
    /// compile error at each literal construction, which is a tax on adding one.
    pub fn empty(unit: Unit) -> Self {
        Report {
            schema_version: SCHEMA_VERSION,
            tool: "ernest".to_string(),
            unit,
            scope: default_scope(),
            lang: None,
            roots: Vec::new(),
            files_scanned: 0,
            files_skipped: 0,
            files_failed: 0,
            unsupported: BTreeMap::new(),
            failed: Vec::new(),
            ernestignore: Vec::new(),
            ernestignore_excluded: 0,
            grammar: BTreeMap::new(),
            ranking: RankingScope::default(),
            total: Totals::default(),
            cohorts: Vec::new(),
            files: None,
            sections: None,
        }
    }

    pub fn headline(&self) -> &Totals {
        &self.total
    }

    pub fn cohort(&self, name: &str) -> Option<&CohortReport> {
        self.cohorts.iter().find(|c| c.cohort == name)
    }
}

/// Which breakdowns to show. All off by default: a bare run is the figure, and
/// every breakdown of it is a question a caller asks on purpose.
///
/// A repository-wide breakdown is stationary — it describes the tree rather than
/// the change just made to it — so showing one unasked spends a reader's
/// attention on rows that read the same before and after the work.
#[derive(Debug, Clone, Copy, Default)]
pub struct Views {
    pub by_cohort: bool,
    pub by_language: bool,
    pub by_file: bool,
    pub by_section: bool,
}

impl Views {
    /// Whether any breakdown was asked for. What the affordance note keys on.
    pub fn any(&self) -> bool {
        self.by_cohort || self.by_language || self.by_file || self.by_section
    }
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
    health: Health,
}

/// Analyze every candidate in parallel, then fold the results deterministically.
pub fn run(survey: &Survey, unit: Unit, views: Views) -> (Report, Diagnostics) {
    let attempts: Vec<Result<Outcome, Failure>> = survey
        .candidates
        .par_iter()
        .map(|candidate| {
            let fail = |reason: &str| Failure {
                path: candidate.path.display().to_string(),
                reason: reason.to_string(),
            };
            let bytes = std::fs::read(&candidate.path).map_err(|_| fail("unreadable"))?;
            // Byte offsets from tree-sitter are only char boundaries in valid
            // UTF-8, and a file that is not text is not prose either.
            let src = String::from_utf8(bytes).map_err(|_| fail("not utf-8"))?;
            // Sections describe a document; a source file has none worth naming.
            let wants_sections = views.by_section && candidate.profile.cohort == Cohort::Docs;
            let analysis = analyze(&src, candidate.profile, wants_sections)
                .map_err(|_| fail("parse failed"))?;
            Ok(Outcome {
                path: candidate.path.clone(),
                language: candidate.profile.language,
                cohort: candidate.profile.cohort.label(),
                provenance: candidate.provenance,
                counts: analysis.counts,
                sections: analysis.sections,
                health: analysis.health,
            })
        })
        .collect();

    let mut failed: Vec<Failure> = Vec::new();
    let mut outcomes: Vec<Outcome> = Vec::new();
    for attempt in attempts {
        match attempt {
            Ok(outcome) => outcomes.push(outcome),
            Err(failure) => failed.push(failure),
        }
    }
    // Candidates arrive sorted, so this already is — asserted rather than
    // assumed, because a snapshot that reorders on a rerun diffs as noise.
    failed.sort_by(|a, b| a.path.cmp(&b.path));

    // Every language gets a row, so a clean grammar is visible as clean rather
    // than absent — `0 of 50` and "no entry" read very differently to someone
    // asking whether a borrowed grammar is coping.
    let mut grammar: BTreeMap<String, GrammarHealth> = BTreeMap::new();
    let mut unread: Vec<String> = Vec::new();
    for outcome in &outcomes {
        let entry = grammar.entry(outcome.language.to_string()).or_default();
        entry.measured += 1;
        if outcome.health.clean() {
            continue;
        }
        entry.files += 1;
        entry.error_nodes += outcome.health.errors;
        entry.missing_nodes += outcome.health.missing;
        unread.push(outcome.path.display().to_string());
    }

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

    let report = Report {
        scope: survey.scope.label().to_string(),
        lang: survey.lang.clone(),
        roots: survey
            .roots
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        files_scanned: outcomes.len() as u64,
        files_skipped: survey.unsupported.values().sum(),
        files_failed: failed.len() as u64,
        unsupported: survey.unsupported.clone(),
        failed,
        ernestignore: survey
            .ernestignore
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        ernestignore_excluded: survey.excluded.len() as u64,
        grammar,
        total,
        cohorts,
        files,
        sections,
        ..Report::empty(unit)
    };

    // Whatever the walk kept. Empty unless `keep_paths` asked for it, which is
    // the one thing allowed to key on verbosity — none of this reaches `Report`.
    let diagnostics = Diagnostics {
        excluded: survey
            .excluded
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        unsupported: survey
            .unsupported_paths
            .iter()
            .map(|p| p.display().to_string())
            .collect(),
        // Always collected: in the sweeps behind this feature it was 0 files of
        // 7,048, so there is nothing to gate.
        unread,
    };

    (report, diagnostics)
}
