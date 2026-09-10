//! What `rote` knows about its own health, in the vocabulary `assay` collects.
//!
//! `assay`'s registry station puts `doctor --format json` to every registered
//! binary on a two-second budget, so nothing here hashes anything: the report is
//! a reading of the log and of the parameters the verifiers already carry.

use std::collections::BTreeSet;

use jiff::civil::Date;
use relic_core::finding::{Detail, Finding, FixHint, Report, Severity, StationId, Summary};
use relic_core::style::{Style, Tint};

use crate::ladder::{Gate, Standing};
use crate::model::State;
use crate::store::{Issue, Journal, Paths, Verifiers};
use crate::verifier::M_COST_KIB;

/// Days past due before an outstanding review is worth reporting. A drill taken
/// a day late is a drill taken.
pub const GRACE_DAYS: u32 = 2;

fn station() -> StationId {
    StationId::from_static("rote")
}

fn summary(text: &str) -> Summary {
    Summary::lossy(text)
}

/// Read the state of the drill.
pub fn report(
    journal: &Journal,
    state: &State,
    verifiers: &Verifiers,
    paths: &Paths,
    today: Date,
    host: &str,
) -> Report {
    let mut findings = Vec::new();
    findings.extend(placement_findings(paths));
    findings.extend(log_findings(journal, host));
    findings.extend(slug_findings(state, verifiers, today));
    findings.extend(orphan_findings(state, verifiers, paths));
    Report::ran(station(), findings)
}

/// The placement rule, enforced rather than trusted: a verifier is an
/// offline-attackable oracle, and the log tree replicates to two providers and
/// keeps prior versions. A config that puts one inside the other is broken
/// however healthy everything else is.
fn placement_findings(paths: &Paths) -> Vec<Finding> {
    if !paths.verifiers().starts_with(&paths.log_dir) {
        return Vec::new();
    }
    vec![
        station()
            .broken(summary(
                "the verifier file sits inside the log tree, which replicates offsite and keeps prior versions",
            ))
            .detailed_with(Detail::new(format!(
                "{}\nunder {}",
                paths.verifiers(),
                paths.log_dir
            )))
            .fixed_by(FixHint::lossy(
                "point state elsewhere in the config, then rote rekey --force each slug",
            )),
    ]
}

/// A verifier the schedule has no use for: its slug is retired, or the log has
/// never heard of it. An oracle nobody drills is cost with no benefit, and a
/// verifier that outlived its retirement is one a removal missed.
fn orphan_findings(state: &State, verifiers: &Verifiers, paths: &Paths) -> Vec<Finding> {
    let scheduled: BTreeSet<&str> = state
        .scheduled()
        .into_iter()
        .map(|slug| slug.slug.as_str())
        .collect();
    let orphans: Vec<String> = verifiers
        .names()
        .filter(|name| !scheduled.contains(name))
        .map(str::to_owned)
        .collect();
    if orphans.is_empty() {
        return Vec::new();
    }
    vec![
        station()
            .soft(summary(
                "a verifier is held for a slug that is not on the schedule",
            ))
            .detailed_with(Detail::new(orphans.join("\n")))
            .fixed_by(FixHint::lossy(&format!(
                "remove the entry from {}",
                paths.verifiers()
            ))),
    ]
}

/// A report for the case where the log could not be read at all. Not knowing is
/// not a clean bill of health.
pub fn unreadable(why: &str) -> Report {
    Report::ran(
        station(),
        vec![
            station()
                .finds(Severity::Broken, summary("the drill log could not be read"))
                .detailed_with(Detail::new(why)),
        ],
    )
}

fn log_findings(journal: &Journal, host: &str) -> Vec<Finding> {
    let mut findings = Vec::new();
    let mut malformed = Vec::new();
    let mut chain = Vec::new();
    let mut future = Vec::new();
    for issue in &journal.issues {
        match issue {
            Issue::Malformed { line, why } => malformed.push(format!("line {line}: {why}")),
            Issue::ChainBreak { line } => chain.push(format!("line {line}")),
            Issue::FromTheFuture { line, v } => future.push(format!("line {line}: schema {v}")),
        }
    }
    if !future.is_empty() {
        findings.push(
            station()
                .broken(summary(
                    "the log holds records from a newer schema, so nothing here can be trusted",
                ))
                .detailed_with(Detail::new(future.join("\n")))
                .fixed_by(FixHint::lossy("upgrade rote on this machine")),
        );
    }
    if !malformed.is_empty() {
        findings.push(
            station()
                .broken(summary("the log holds lines that will not parse"))
                .detailed_with(Detail::new(malformed.join("\n"))),
        );
    }
    if !chain.is_empty() {
        findings.push(
            station()
                .broken(summary(
                    "the log's hash chain is broken, so it has been edited or truncated",
                ))
                .detailed_with(Detail::new(chain.join("\n"))),
        );
    }
    let strangers: Vec<&String> = journal
        .hosts
        .iter()
        .filter(|written| written.as_str() != host)
        .collect();
    if !strangers.is_empty() {
        let names: Vec<String> = strangers.into_iter().cloned().collect();
        findings.push(
            station()
                .soft(summary(
                    "the log has been written by another machine, and it is not a mergeable structure",
                ))
                .detailed_with(Detail::new(names.join("\n")))
                .fixed_by(FixHint::lossy("drill on one machine, and copy rather than sync")),
        );
    }
    findings
}

fn slug_findings(state: &State, verifiers: &Verifiers, today: Date) -> Vec<Finding> {
    let scheduled = state.scheduled();
    if scheduled.is_empty() {
        return vec![station().note(summary("no slugs are being drilled"))];
    }
    let mut found = Found::default();

    for slug in scheduled {
        match slug.standing(today) {
            Standing::Due => {
                let late = slug.days_overdue(today);
                if late > GRACE_DAYS {
                    found
                        .overdue
                        .push(format!("{}: {late} days past due", slug.slug));
                }
            }
            Standing::Probe | Standing::Held { .. } | Standing::Waiting { .. } => {}
        }
        match verifiers.get(&slug.slug) {
            Ok(Some(verifier)) => {
                if verifier.meets_floor().is_ok_and(|meets| !meets) {
                    let cost = verifier.m_cost_kib().unwrap_or(0);
                    found.weak.push(format!(
                        "{}: {cost} KiB, against a floor of {M_COST_KIB} KiB",
                        slug.slug
                    ));
                }
            }
            Ok(None) => found.missing.push(slug.slug.to_string()),
            Err(error) => found.weak.push(format!("{}: {error}", slug.slug)),
        }
        if let Gate::Ready { passes } = slug.gate() {
            found
                .ready
                .push(format!("{}: {passes} cold passes at the cap", slug.slug));
        }
        if slug.critical && slug.first_unaided.is_none() {
            found.unproven.push(slug.slug.to_string());
        }
        if slug.aided_mismatch {
            found.mismatched.push(slug.slug.to_string());
        }
    }

    found.into_findings()
}

/// What the scan collected, one list per finding it can produce.
#[derive(Default)]
struct Found {
    overdue: Vec<String>,
    missing: Vec<String>,
    weak: Vec<String>,
    ready: Vec<String>,
    unproven: Vec<String>,
    mismatched: Vec<String>,
}

impl Found {
    fn into_findings(self) -> Vec<Finding> {
        let mut findings = Vec::new();
        if !self.weak.is_empty() {
            findings.push(
                station()
                    .broken(summary(
                        "a verifier is weaker than the artifact it verifies, which makes it the cheaper attack path",
                    ))
                    .detailed_with(Detail::new(self.weak.join("\n")))
                    .fixed_by(FixHint::lossy("rote rekey the slug named below")),
            );
        }
        if !self.missing.is_empty() {
            findings.push(
                station()
                    .soft(summary(
                        "a slug on the schedule has no verifier on this machine",
                    ))
                    .detailed_with(Detail::new(self.missing.join("\n")))
                    .fixed_by(FixHint::lossy("rote rekey --force the slug named below")),
            );
        }
        if !self.overdue.is_empty() {
            findings.push(
                station()
                    .soft(summary("a drill is overdue"))
                    .detailed_with(Detail::new(self.overdue.join("\n")))
                    .fixed_by(FixHint::lossy("rote")),
            );
        }
        if !self.mismatched.is_empty() {
            findings.push(
                station()
                    .soft(summary(
                        "an aided entry was refused, so the vault and the verifier hold different secrets",
                    ))
                    .detailed_with(Detail::new(self.mismatched.join("\n")))
                    .fixed_by(FixHint::lossy(
                        "confirm which one is current, then rote rekey",
                    )),
            );
        }
        if !self.unproven.is_empty() {
            findings.push(
                station()
                    .note(summary(
                        "a critical slug has never been recalled without the answer in front of it",
                    ))
                    .detailed_with(Detail::new(self.unproven.join("\n"))),
            );
        }
        if !self.ready.is_empty() {
            findings.push(
                station()
                    .note(summary("a slug has cleared the cutover gate"))
                    .detailed_with(Detail::new(self.ready.join("\n"))),
            );
        }
        findings
    }
}

/// The human shape: one line per finding, with the verdict last.
///
/// A terminal block is read from the bottom, so the grade goes there.
pub fn render(report: &Report, style: Style) -> String {
    let mut lines = Vec::new();
    let findings = match &report.outcome {
        relic_core::finding::Outcome::Ran(findings) => findings.as_slice(),
        relic_core::finding::Outcome::Skipped(reason) => {
            return format!("rote: skipped — {reason}");
        }
    };
    for finding in findings {
        let tint = match finding.severity {
            Severity::Broken => Tint::Red,
            Severity::Soft => Tint::Yellow,
            Severity::Note => Tint::Dim,
        };
        lines.push(format!(
            "{}  {}",
            style.paint(tint, &finding.severity.to_string()),
            finding.summary
        ));
        if let Some(detail) = &finding.detail {
            for line in detail.as_str().lines() {
                lines.push(style.dim(&format!("        {line}")));
            }
        }
        if let Some(fix) = &finding.fix {
            lines.push(style.dim(&format!("        fix: {fix}")));
        }
    }
    let grade = report.grade();
    let verdict = match grade {
        relic_core::finding::Grade::Ok => style.green("==> rote ok"),
        relic_core::finding::Grade::Soft => style.yellow(&format!(
            "!!> rote degraded — {} to look at",
            findings.len()
        )),
        relic_core::finding::Grade::Broken => {
            style.bold(&format!("!!> rote BROKEN — {} to look at", findings.len()))
        }
    };
    lines.push(verdict);
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};
    use relic_core::finding::{Grade, Outcome, Severity};

    use super::{GRACE_DAYS, unreadable};
    use crate::ladder::Class;
    use crate::log::{Added, Attempted, Digest, Event, Line, Record, Retired, SCHEMA, SessionId};
    use crate::model::State;
    use crate::store::{Journal, Paths, Verifiers};
    use crate::verifier::Verifier;

    fn paths() -> Paths {
        Paths {
            log_dir: "/x/ark/rote".into(),
            state_dir: "/x/state/rote".into(),
        }
    }

    fn report(
        journal: &Journal,
        state: &State,
        verifiers: &Verifiers,
        today: Date,
        host: &str,
    ) -> relic_core::finding::Report {
        super::report(journal, state, verifiers, &paths(), today, host)
    }

    const PHC: &str = "$argon2id$v=19$m=262144,t=4,p=1$CQkJCQkJCQkJCQkJCQkJCQ$\
                       PdWLZDvNGYPMYWPfbSZq7yZO0eFRHmiGnLTuNBAxUC0";
    const WEAK: &str = "$argon2id$v=19$m=64,t=1,p=1$CQkJCQkJCQkJCQkJCQkJCQ$\
                        PdWLZDvNGYPMYWPfbSZq7yZO0eFRHmiGnLTuNBAxUC0";

    fn line(day: Date, event: Event) -> Line {
        Line::Parsed(Box::new(Record {
            v: SCHEMA,
            at: day.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp(),
            day,
            host: "Mac".to_owned(),
            prev: Digest::GENESIS,
            event,
        }))
    }

    fn added(day: Date, name: &str) -> Line {
        line(
            day,
            Event::Add(Added {
                slug: name.parse().unwrap(),
                version: 1,
                critical: false,
            }),
        )
    }

    fn review(day: Date, name: &str, effective: u32, before: u8, after: u8) -> Line {
        line(
            day,
            Event::Attempt(Attempted {
                slug: name.parse().unwrap(),
                session: SessionId::from_bytes([0; 8]),
                version: 1,
                class: Class::Review,
                attempt: 1,
                outcome: crate::log::Outcome::Pass,
                ttfk_ms: Some(900),
                total_ms: Some(3_000),
                corrections: 0,
                paste_refused: 0,
                scheduled_interval_days: 7,
                actual_interval_days: Some(effective),
                effective_interval_days: Some(effective),
                stretch: false,
                step_before: before,
                step_after: after,
            }),
        )
    }

    fn verifiers(names: &[(&str, &str)]) -> Verifiers {
        let mut out = Verifiers::default();
        for (name, phc) in names {
            out.set(&name.parse().unwrap(), &Verifier::parse(phc).unwrap());
        }
        out
    }

    fn findings(report: &relic_core::finding::Report) -> Vec<(Severity, String)> {
        match &report.outcome {
            Outcome::Ran(findings) => findings
                .iter()
                .map(|f| (f.severity, f.summary.as_str().to_owned()))
                .collect(),
            Outcome::Skipped(_) => Vec::new(),
        }
    }

    #[test]
    fn a_healthy_drill_reports_nothing_and_grades_clean() {
        let lines = vec![added(date(2026, 9, 10), "a")];
        let report = report(
            &Journal::default(),
            &State::replay(&lines),
            &verifiers(&[("a", PHC)]),
            date(2026, 9, 10),
            "Mac",
        );
        assert!(findings(&report).is_empty(), "{:?}", findings(&report));
        assert_eq!(report.grade(), Grade::Ok);
    }

    #[test]
    fn an_empty_roster_is_a_note_rather_than_a_complaint() {
        let report = report(
            &Journal::default(),
            &State::default(),
            &Verifiers::default(),
            date(2026, 9, 10),
            "Mac",
        );
        assert_eq!(findings(&report).len(), 1);
        assert_eq!(report.grade(), Grade::Ok, "a note is read, never counted");
    }

    #[test]
    fn a_slug_never_reviewed_still_falls_overdue() {
        let lines = vec![added(date(2026, 9, 1), "a")];
        let late = report(
            &Journal::default(),
            &State::replay(&lines),
            &verifiers(&[("a", PHC)]),
            date(2026, 9, 30),
            "Mac",
        );
        assert!(
            findings(&late).iter().any(|f| f.1.contains("overdue")),
            "{:?}",
            findings(&late)
        );
        assert_eq!(late.grade(), Grade::Soft);
    }

    #[test]
    fn a_verifier_inside_the_log_tree_is_broken() {
        let inside = Paths {
            log_dir: "/x/ark/rote".into(),
            state_dir: "/x/ark/rote/state".into(),
        };
        let report = super::report(
            &Journal::default(),
            &State::default(),
            &Verifiers::default(),
            &inside,
            date(2026, 9, 10),
            "Mac",
        );
        assert_eq!(report.grade(), Grade::Broken);
        assert!(
            findings(&report)
                .iter()
                .any(|f| f.1.contains("inside the log tree")),
            "{:?}",
            findings(&report)
        );
    }

    #[test]
    fn a_verifier_for_a_retired_or_unknown_slug_is_soft() {
        let lines = vec![
            added(date(2026, 9, 1), "a"),
            added(date(2026, 9, 1), "b"),
            line(
                date(2026, 9, 2),
                Event::Retire(Retired {
                    slug: "b".parse().unwrap(),
                }),
            ),
        ];
        let report = report(
            &Journal::default(),
            &State::replay(&lines),
            &verifiers(&[("a", PHC), ("b", PHC), ("ghost", PHC)]),
            date(2026, 9, 2),
            "Mac",
        );
        let found = findings(&report);
        let orphan = found
            .iter()
            .find(|f| f.1.contains("not on the schedule"))
            .expect("an orphan finding");
        assert_eq!(orphan.0, Severity::Soft);
        assert_eq!(report.grade(), Grade::Soft);
    }

    #[test]
    fn a_drill_past_the_grace_is_soft_and_one_inside_it_is_nothing() {
        let lines = vec![
            added(date(2026, 9, 1), "a"),
            review(date(2026, 9, 2), "a", 1, 3, 4),
        ];
        let state = State::replay(&lines);
        let inside = date(2026, 9, 9)
            .checked_add(jiff::Span::new().days(i64::from(GRACE_DAYS)))
            .unwrap();
        let quiet = report(
            &Journal::default(),
            &state,
            &verifiers(&[("a", PHC)]),
            inside,
            "Mac",
        );
        assert!(findings(&quiet).is_empty(), "{:?}", findings(&quiet));

        let late = report(
            &Journal::default(),
            &state,
            &verifiers(&[("a", PHC)]),
            date(2026, 9, 30),
            "Mac",
        );
        assert_eq!(findings(&late).first().map(|f| f.0), Some(Severity::Soft));
        assert_eq!(late.grade(), Grade::Soft);
    }

    #[test]
    fn a_missing_verifier_is_soft_and_says_how_to_get_one_back() {
        let lines = vec![added(date(2026, 9, 10), "a")];
        let report = report(
            &Journal::default(),
            &State::replay(&lines),
            &Verifiers::default(),
            date(2026, 9, 10),
            "Mac",
        );
        let found = findings(&report);
        assert_eq!(found.len(), 1);
        assert_eq!(found.first().unwrap().0, Severity::Soft);
        assert!(found.first().unwrap().1.contains("no verifier"));
    }

    #[test]
    fn a_verifier_below_the_floor_is_broken() {
        let lines = vec![added(date(2026, 9, 10), "a")];
        let report = report(
            &Journal::default(),
            &State::replay(&lines),
            &verifiers(&[("a", WEAK)]),
            date(2026, 9, 10),
            "Mac",
        );
        assert_eq!(report.grade(), Grade::Broken);
    }

    #[test]
    fn an_edited_log_is_broken_and_a_second_machine_is_soft() {
        let mut journal = Journal::parse("{ not json\n");
        journal.hosts.insert("Other".to_owned());
        let report = report(
            &journal,
            &State::default(),
            &Verifiers::default(),
            date(2026, 9, 10),
            "Mac",
        );
        let severities: Vec<Severity> = findings(&report).into_iter().map(|f| f.0).collect();
        assert!(severities.contains(&Severity::Broken));
        assert!(severities.contains(&Severity::Soft));
        assert_eq!(report.grade(), Grade::Broken);
    }

    #[test]
    fn a_log_that_could_not_be_read_is_broken_rather_than_silent() {
        let report = unreadable("permission denied");
        assert_eq!(report.grade(), Grade::Broken);
    }

    #[test]
    fn the_wire_form_is_a_single_report_the_registry_can_read() {
        let report = report(
            &Journal::default(),
            &State::default(),
            &Verifiers::default(),
            date(2026, 9, 10),
            "Mac",
        );
        let text = serde_json::to_string(&report).unwrap();
        let back: relic_core::finding::Report = serde_json::from_str(&text).unwrap();
        assert_eq!(back.station.as_str(), "rote");
    }
}
