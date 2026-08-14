//! The conditional register that closes a text report. Every note states
//! something that qualifies the figure above it, and every note fires only when
//! what it states is true — so a clean run says nothing, and a run whose number
//! deserves a caveat carries exactly that caveat.
//!
//! Shared by `human` and `diff`. The footer used to exist only in the former,
//! which left a diff silently free of the `.ernestignore` and unit caveats that
//! govern both of the snapshots it compares.

use crate::aggregate::Report;
use crate::span::Unit;
use crate::walk::Provenance;

use super::{Verbosity, count, plural, thousands};

/// Extensions named before the rest are summarised away at the default level. A
/// note is not a view, so `--top` does not govern it — `-v` uncaps it, and
/// `--json` carries the whole histogram at every level.
const GAPS: usize = 4;

/// Every `--by` value, in the order they are declared: what a caller most often
/// wants first.
const VIEWS: &str = "--by file|section|cohort|language, --top N";

#[derive(Debug, Default)]
pub struct Notes(Vec<String>);

impl Notes {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn push(&mut self, note: impl Into<String>) {
        self.0.push(note.into());
    }

    /// What was measured and what was not. The headline sums every cohort, so a
    /// format ernest cannot read skews it rather than abstaining from it — this
    /// is the line that makes that visible, and names the profile to write next.
    pub fn census(&mut self, report: &Report, level: Verbosity) {
        let mut line = count(report.files_scanned, "file") + " measured";
        if report.files_skipped > 0 {
            line.push_str(&format!(
                ", {} unsupported",
                thousands(report.files_skipped)
            ));
            // The histogram is summarised at the default level and whole at `-v`:
            // "+2 more" is the right answer to a question nobody asked, and the
            // wrong one to a caller who asked for provenance.
            let gaps = if level >= Verbosity::Verbose {
                usize::MAX
            } else {
                GAPS
            };
            if let Some(named) = unsupported(report, gaps) {
                line.push_str(&format!(" ({named})"));
            }
        }
        if report.files_failed > 0 {
            line.push_str(&format!(", {} unreadable", thousands(report.files_failed)));
        }
        self.push(line);
    }

    /// What the walk was asked for and what it reached. Provenance rather than
    /// caveat: none of it qualifies the figure, and all of it answers "why is
    /// this number over these files" — which is only a question once someone is
    /// surprised by the answer.
    pub fn provenance(&mut self, report: &Report, level: Verbosity) {
        if level < Verbosity::Verbose {
            return;
        }

        if !report.roots.is_empty() {
            self.push(format!("measuring {}", report.roots.join(", ")));
        }

        self.push(format!(
            "--scope {} — {}",
            report.scope,
            match report.scope.as_str() {
                "shared" => "only what a fresh clone would see",
                "all" => "gitignored files included",
                _ => "locally-excluded files included, gitignored files not",
            }
        ));

        // Tracked against local is the second brain made visible: prose in a
        // committed document is paid for by a reader, prose in `.claude/` by
        // every context load.
        let (tracked, local) = provenance_split(report);
        if local > 0 {
            self.push(format!(
                "{} tracked, {} local to this machine",
                thousands(tracked),
                thousands(local)
            ));
        }

        // A narrowed run passes over supported files without counting them as a
        // coverage gap — correct, and silent, so the figure looks repository-wide
        // when it is not.
        if let Some(lang) = &report.lang {
            self.push(format!(
                "--lang {lang} — supported files in other languages were not measured"
            ));
        }
    }

    /// A declared corpus is prose that is the product rather than prose about
    /// the code. Excluded silently it would read as a repository with less prose
    /// in it, so the exclusion is said out loud wherever it applied.
    ///
    /// Materiality decides *when*, not verbosity. Behind `-v` a default run
    /// would under-report in silence, which is the failure this line exists to
    /// prevent — but a corpus that removed one test fixture has not moved the
    /// figure and does not need announcing in every measurement of the tree
    /// above it. So: named by default when it removed enough to matter, and at
    /// `-v` either way.
    ///
    /// The count is corpus-wide rather than per rule file. Attributing an
    /// exclusion to one of several `.ernestignore` files would mean matching
    /// each rule set separately, and the number that decides materiality is the
    /// total.
    pub fn corpora(&mut self, report: &Report, level: Verbosity) {
        if !material(report) && level < Verbosity::Verbose {
            return;
        }
        for path in &report.ernestignore {
            let mut line = format!("{path} applied");
            if report.ernestignore_excluded > 0 {
                line.push_str(&format!(
                    " — {} excluded as a declared corpus",
                    count(report.ernestignore_excluded, "file")
                ));
            } else {
                line.push_str(" — a declared corpus is not measured");
            }
            self.push(line);
        }
    }

    /// A clean corpus is a result, not an absence. Without this line, `-vvv` on a
    /// repository whose grammars all coped would look like a rung that failed to
    /// print rather than one that found nothing to report.
    pub fn grammar(&mut self, report: &Report, level: Verbosity) {
        if level < Verbosity::Trace {
            return;
        }
        if report.grammar.values().all(|health| health.files == 0) {
            self.push("no file defeated its grammar");
        }
    }

    /// Lines cannot split a mixed line, so the unit that resolves one by
    /// dominance says so rather than letting the figure pass as the canonical
    /// one.
    pub fn unit(&mut self, unit: Unit) {
        if unit == Unit::Lines {
            self.push("counting lines; a line belongs to whichever class holds most of it");
        }
    }

    /// A truncated list that looks complete is worse than no list.
    pub fn truncated(&mut self, shown: usize, total: usize, noun: &str) {
        if total > shown {
            let withheld = (total - shown) as u64;
            self.push(format!(
                "… {} more {} — --top 0 shows every row",
                thousands(withheld),
                plural(withheld, noun)
            ));
        }
    }

    /// The one standing affordance, and it stands only while nothing has been
    /// asked for: once a caller names a view, the menu has done its job and
    /// repeating it is prose the tool emits about itself.
    ///
    /// `--json` is offered only where there is a measurement to snapshot — a
    /// diff reads snapshots rather than writing one.
    pub fn views(&mut self, snapshot: bool) {
        let mut line = VIEWS.to_string();
        if snapshot {
            line.push_str(", --json");
        }
        self.push(line);
    }

    pub fn render(&self) -> String {
        self.0
            .iter()
            .map(|note| format!("  {note}\n"))
            .collect::<String>()
    }
}

/// Whether a declared corpus removed enough to have moved the figure.
///
/// A ratio rather than a ceiling, because materiality is proportional: the same
/// corpus is trivia in a monorepo and the whole story in a four-file tree. A tenth
/// of what would have been measured is the line — below it, the exclusion is the
/// test fixture case and belongs behind `-v`.
fn material(report: &Report) -> bool {
    let excluded = report.ernestignore_excluded;
    excluded > 0 && excluded * 10 >= excluded + report.files_scanned
}

/// Which extensions the skipped files were, heaviest first, naming at most
/// `limit` of them.
fn unsupported(report: &Report, limit: usize) -> Option<String> {
    if report.unsupported.is_empty() {
        return None;
    }
    let mut gaps: Vec<(&String, &u64)> = report.unsupported.iter().collect();
    gaps.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    let mut named: Vec<String> = gaps
        .iter()
        .take(limit)
        .map(|(ext, n)| format!("{ext} {}", thousands(**n)))
        .collect();
    if gaps.len() > limit {
        named.push(format!("+{} more", thousands((gaps.len() - limit) as u64)));
    }
    Some(named.join(", "))
}

/// Files on each side of the share, summed from the breakdown that is built on
/// every run — so this costs nothing and needs no new field.
fn provenance_split(report: &Report) -> (u64, u64) {
    let mut tracked = 0;
    let mut local = 0;
    for cohort in &report.cohorts {
        for language in &cohort.languages {
            match language.provenance {
                Provenance::Tracked => tracked += language.files,
                Provenance::Local => local += language.files,
            }
        }
    }
    (tracked, local)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn report() -> Report {
        Report {
            files_scanned: 1,
            ..Report::empty(Unit::Chars)
        }
    }

    #[test]
    fn a_clean_census_carries_only_what_it_measured() {
        let mut notes = Notes::default();
        notes.census(&report(), Verbosity::Normal);
        assert_eq!(notes.render(), "  1 file measured\n");
    }

    #[test]
    fn an_unsupported_tally_names_the_heaviest_and_counts_the_rest() {
        let mut report = report();
        report.files_scanned = 40;
        report.files_skipped = 17;
        report.files_failed = 2;
        report.unsupported = [
            ("json".to_string(), 14),
            ("lock".to_string(), 1),
            ("map".to_string(), 9),
            ("vim".to_string(), 20),
            ("log".to_string(), 3),
            ("bin".to_string(), 2),
        ]
        .into_iter()
        .collect();

        let mut notes = Notes::default();
        notes.census(&report, Verbosity::Normal);
        assert_eq!(
            notes.render(),
            "  40 files measured, 17 unsupported (vim 20, json 14, map 9, log 3, +2 more), 2 unreadable\n"
        );

        // The same tally at `-v`: every extension named, nothing summarised
        // away, and the heaviest-first order unchanged.
        let mut notes = Notes::default();
        notes.census(&report, Verbosity::Verbose);
        assert_eq!(
            notes.render(),
            "  40 files measured, 17 unsupported (vim 20, json 14, map 9, log 3, bin 2, lock 1), 2 unreadable\n"
        );
    }

    #[test]
    fn a_complete_list_says_nothing_about_being_complete() {
        let mut notes = Notes::default();
        notes.truncated(20, 20, "file");
        assert!(notes.is_empty());
        notes.truncated(20, 21, "file");
        assert_eq!(
            notes.render(),
            "  … 1 more file — --top 0 shows every row\n"
        );
    }
}
