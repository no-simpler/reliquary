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

use super::{count, plural, thousands};

/// Extensions named before the rest are summarised away. A note is not a view,
/// so `--top` does not govern it — `--json` carries the whole histogram.
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
    pub fn census(&mut self, report: &Report) {
        let mut line = count(report.files_scanned, "file") + " measured";
        if report.files_skipped > 0 {
            line.push_str(&format!(
                ", {} unsupported",
                thousands(report.files_skipped)
            ));
            if let Some(gaps) = unsupported(report) {
                line.push_str(&format!(" ({gaps})"));
            }
        }
        if report.files_failed > 0 {
            line.push_str(&format!(", {} unreadable", thousands(report.files_failed)));
        }
        self.push(line);
    }

    /// A declared corpus is prose that is the product rather than prose about
    /// the code. Excluded silently it would read as a repository with less prose
    /// in it, so the exclusion is said out loud wherever it applied.
    pub fn corpora(&mut self, report: &Report) {
        for path in &report.ernestignore {
            self.push(format!(
                "{path} applied — a declared corpus is not measured"
            ));
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

/// Which extensions the skipped files were, heaviest first.
fn unsupported(report: &Report) -> Option<String> {
    if report.unsupported.is_empty() {
        return None;
    }
    let mut gaps: Vec<(&String, &u64)> = report.unsupported.iter().collect();
    gaps.sort_by(|a, b| b.1.cmp(a.1).then_with(|| a.0.cmp(b.0)));

    let mut named: Vec<String> = gaps
        .iter()
        .take(GAPS)
        .map(|(ext, n)| format!("{ext} {}", thousands(**n)))
        .collect();
    if gaps.len() > GAPS {
        named.push(format!("+{} more", thousands((gaps.len() - GAPS) as u64)));
    }
    Some(named.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aggregate::{SCHEMA_VERSION, Totals};
    use std::collections::BTreeMap;

    fn report() -> Report {
        Report {
            schema_version: SCHEMA_VERSION,
            tool: "ernest".to_string(),
            unit: Unit::Chars,
            files_scanned: 1,
            files_skipped: 0,
            files_failed: 0,
            unsupported: BTreeMap::new(),
            ernestignore: Vec::new(),
            total: Totals::default(),
            cohorts: Vec::new(),
            files: None,
            sections: None,
        }
    }

    #[test]
    fn a_clean_census_carries_only_what_it_measured() {
        let mut notes = Notes::default();
        notes.census(&report());
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
        notes.census(&report);
        assert_eq!(
            notes.render(),
            "  40 files measured, 17 unsupported (vim 20, json 14, map 9, log 3, +2 more), 2 unreadable\n"
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
