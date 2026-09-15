//! What `rote` knows about its own health, in the vocabulary `assay` collects.
//!
//! `assay`'s registry station puts `doctor --format json` to every registered
//! binary on a two-second budget, so nothing here hashes anything: the report is
//! a reading of the corpus and of the parameters the verifiers already carry.
//!
//! **Defects only.** A fact that `status` or `stats` already states does not
//! also belong here — one home per reading, or two of them go stale apart.

use std::collections::{BTreeMap, BTreeSet};

use jiff::civil::Date;
use relic_core::finding::{Detail, Finding, FixHint, Report, Severity, StationId, Summary};
use relic_core::style::{Style, Tint};

use crate::corpus::Corpus;
use crate::corpus::chain::{Chains, Issue};
use crate::corpus::record::Event;
use crate::ladder::{Ladder, Standing};
use crate::machine::{Flagship, MachineId};
use crate::store::Paths;
use crate::verifier::M_COST_KIB;
use crate::verifier::file::Verifiers;

/// Days past due before an outstanding review is worth reporting. A drill taken
/// a day late is a drill taken.
pub const GRACE_DAYS: u32 = 2;

fn station() -> StationId {
    StationId::from_static("rote")
}

fn verifiers_station() -> StationId {
    StationId::from_static("rote-verifiers")
}

fn corpus_station() -> StationId {
    StationId::from_static("rote-corpus")
}

fn drill_station() -> StationId {
    StationId::from_static("rote-drill")
}

fn machine_station() -> StationId {
    StationId::from_static("rote-machine")
}

fn summary(text: &str) -> Summary {
    Summary::lossy(text)
}

/// Everything a report needs, gathered by the caller so this never reads the
/// world itself.
pub struct Health<'a> {
    /// The chains, as read.
    pub chains: &'a Chains,
    /// What they add up to.
    pub corpus: &'a Corpus,
    /// What this machine holds.
    pub verifiers: &'a Verifiers,
    /// Where everything lives.
    pub paths: &'a Paths,
    /// This machine.
    pub machine: &'a MachineId,
    /// What it calls itself.
    pub host: &'a str,
    /// Whether it may write.
    pub flagship: &'a Flagship,
    /// The drill day.
    pub today: Date,
    /// The schedule.
    pub ladder: &'a Ladder,
}

/// Read the state of the drill.
pub fn report(health: &Health<'_>) -> Report {
    let mut findings = Vec::new();
    findings.extend(placement(health));
    findings.extend(verifier_findings(health));
    findings.extend(chain_findings(health));
    findings.extend(divergence(health));
    findings.extend(concurrency(health));
    findings.extend(drill_findings(health));
    findings.extend(machine_findings(health));
    Report::ran(station(), findings)
}

/// The placement rule, enforced rather than trusted: a verifier is an
/// offline-attackable oracle, and the corpus tree replicates to two providers
/// and keeps prior versions.
fn placement(health: &Health<'_>) -> Vec<Finding> {
    if !health.paths.verifiers().starts_with(&health.paths.log_dir) {
        return Vec::new();
    }
    vec![
        verifiers_station()
            .broken(summary(
                "the verifier file sits inside the corpus tree, which replicates offsite and keeps prior versions",
            ))
            .detailed_with(Detail::new(format!(
                "{}\nunder {}",
                health.paths.verifiers(),
                health.paths.log_dir
            )))
            .fixed_by(FixHint::lossy(
                "point state elsewhere in the config, then rote attach each lineage",
            )),
    ]
}

/// What this machine holds that it should not, and what it holds badly.
fn verifier_findings(health: &Health<'_>) -> Vec<Finding> {
    let mut weak = Vec::new();
    let mut unreadable = Vec::new();
    let mut superseded = Vec::new();
    let mut orphan = Vec::new();
    let mut mislabelled = Vec::new();

    for (engram, held) in health.verifiers.held() {
        match health.verifiers.get(engram) {
            Ok(Some(verifier)) => {
                if verifier.meets_floor().is_ok_and(|ok| !ok) {
                    weak.push(format!(
                        "{}  m={} against a floor of {M_COST_KIB}",
                        health.corpus.label(engram),
                        verifier.m_cost_kib().unwrap_or_default()
                    ));
                }
            }
            Ok(None) | Err(_) => unreadable.push(health.corpus.label(engram)),
        }
        match health.corpus.owner(engram) {
            None => orphan.push(format!("{}  {}", held.slug, engram)),
            Some(lineage) => {
                if held.slug != lineage.slug {
                    mislabelled.push(format!(
                        "{engram}  filed under {} and the corpus says {}",
                        held.slug, lineage.slug
                    ));
                }
                let current = lineage.current().is_some_and(|d| d.engram == *engram);
                if !current {
                    superseded.push(health.corpus.label(engram));
                } else if lineage.retired {
                    orphan.push(format!("{}  retired", lineage.slug));
                }
            }
        }
    }

    let mut findings = Vec::new();
    if !weak.is_empty() {
        findings.push(
            verifiers_station()
                .broken(summary(
                    "a verifier is weaker than the artifact it verifies, which makes it the cheaper attack path",
                ))
                .detailed_with(Detail::new(weak.join("\n")))
                .fixed_by(FixHint::lossy(
                    "rote attach the lineage named below, or pass one drill on it",
                )),
        );
    }
    if !unreadable.is_empty() {
        findings.push(
            verifiers_station()
                .broken(summary(
                    "a verifier will not parse, so the drill cannot judge that engram",
                ))
                .detailed_with(Detail::new(unreadable.join("\n")))
                .fixed_by(FixHint::lossy("rote attach the lineage named below")),
        );
    }
    if !superseded.is_empty() {
        findings.push(
            verifiers_station()
                .broken(summary(
                    "a verifier is held for an engram the corpus has superseded, which is a live oracle for a secret that was rotated away",
                ))
                .detailed_with(Detail::new(superseded.join("\n")))
                .fixed_by(FixHint::lossy(&format!(
                    "remove the entry from {}",
                    health.paths.verifiers()
                ))),
        );
    }
    if !mislabelled.is_empty() {
        findings.push(
            verifiers_station()
                .soft(summary(
                    "a verifier names a lineage the corpus does not, so the file has been edited by hand",
                ))
                .detailed_with(Detail::new(mislabelled.join("\n"))),
        );
    }
    if !orphan.is_empty() {
        findings.push(
            verifiers_station()
                .soft(summary("a verifier is held for an engram nothing drills"))
                .detailed_with(Detail::new(orphan.join("\n")))
                .fixed_by(FixHint::lossy(&format!(
                    "remove the entry from {}",
                    health.paths.verifiers()
                ))),
        );
    }
    findings
}

/// What is wrong with the files themselves.
fn chain_findings(health: &Health<'_>) -> Vec<Finding> {
    let mut future = Vec::new();
    let mut malformed = Vec::new();
    let mut broken = Vec::new();
    let mut gaps = Vec::new();
    let mut foreign = Vec::new();

    for issue in &health.chains.issues {
        match issue {
            Issue::FromTheFuture { machine, line, v } => {
                future.push(format!("{machine} line {line}  schema {v}"));
            }
            Issue::Malformed { machine, line, why } => {
                malformed.push(format!("{machine} line {line}  {why}"));
            }
            Issue::ChainBreak { machine, line } => broken.push(format!("{machine} line {line}")),
            Issue::SeqBreak {
                machine,
                line,
                expected,
                found,
            } => gaps.push(format!(
                "{machine} line {line}  expected {expected}, found {found}"
            )),
            Issue::MachineMismatch { file, claimed } => {
                foreign.push(format!("{file}  claims {claimed}"));
            }
            Issue::Foreign { file } => foreign.push(file.clone()),
        }
    }

    let mut findings = Vec::new();
    if !future.is_empty() {
        findings.push(
            corpus_station()
                .broken(summary(
                    "a chain holds records written by a newer rote, so this one will not add to it",
                ))
                .detailed_with(Detail::new(future.join("\n")))
                .fixed_by(FixHint::lossy("upgrade rote on this machine")),
        );
    }
    if !malformed.is_empty() {
        findings.push(
            corpus_station()
                .broken(summary("a chain holds lines that will not parse"))
                .detailed_with(Detail::new(malformed.join("\n"))),
        );
    }
    if !broken.is_empty() {
        findings.push(
            corpus_station()
                .broken(summary(
                    "a chain's digests do not chain, so it has been edited or cut",
                ))
                .detailed_with(Detail::new(broken.join("\n"))),
        );
    }
    if !foreign.is_empty() {
        findings.push(
            corpus_station()
                .broken(summary(
                    "something in the chains directory is not a chain, and its records are excluded",
                ))
                .detailed_with(Detail::new(foreign.join("\n")))
                .fixed_by(FixHint::lossy(
                    "move it out of the chains directory, or delete it if it is a copy",
                )),
        );
    }
    if !gaps.is_empty() {
        findings.push(
            corpus_station()
                .soft(summary("a chain's positions do not run in order"))
                .detailed_with(Detail::new(gaps.join("\n"))),
        );
    }
    findings
}

/// A capture whose recorded predecessor is not the one merged order gives it.
///
/// The witness earning its keep: two machines that wrote without having seen
/// each other each believed a different history, and this is where that shows.
fn divergence(health: &Health<'_>) -> Vec<Finding> {
    let mut rungs: BTreeMap<crate::corpus::record::EngramId, u8> = BTreeMap::new();
    let mut found = Vec::new();
    for record in health.chains.records() {
        let Event::Capture(capture) = &record.event else {
            continue;
        };
        if !capture.scores() {
            continue;
        }
        let seen = rungs.get(&capture.engram).copied().unwrap_or(0);
        if capture.rung_before != seen {
            found.push(format!(
                "{}  {}  says it followed rung {} and merged order gives rung {seen}",
                health.corpus.label(&capture.engram),
                record.day,
                capture.rung_before
            ));
        }
        rungs.insert(capture.engram, capture.rung_after);
    }
    if found.is_empty() {
        return Vec::new();
    }
    vec![
        corpus_station()
            .soft(summary(
                "a capture does not follow the one before it, so two chains were written against different histories",
            ))
            .detailed_with(Detail::new(found.join("\n")))
            .fixed_by(FixHint::lossy("drill on one machine · rote machines")),
    ]
}

/// Two machines writing on one drill day, which double-counts retention.
fn concurrency(health: &Health<'_>) -> Vec<Finding> {
    let mut days: BTreeMap<Date, BTreeSet<&MachineId>> = BTreeMap::new();
    for record in health.chains.records() {
        days.entry(record.day).or_default().insert(&record.machine);
    }
    let shared: Vec<String> = days
        .into_iter()
        .filter(|(_, machines)| machines.len() > 1)
        .map(|(day, machines)| {
            let names: Vec<String> = machines.into_iter().map(ToString::to_string).collect();
            format!("{day}  {}", names.join(", "))
        })
        .collect();
    if shared.is_empty() {
        return Vec::new();
    }
    vec![
        corpus_station()
            .soft(summary(
                "two machines wrote on one drill day, so a reading may be counted twice",
            ))
            .detailed_with(Detail::new(shared.join("\n")))
            .fixed_by(FixHint::lossy("drill on the flagship · rote machines")),
    ]
}

/// What is wrong with the drill itself.
fn drill_findings(health: &Health<'_>) -> Vec<Finding> {
    let mut overdue = Vec::new();
    let mut dormant = Vec::new();
    let mut mismatched = Vec::new();

    for lineage in health.corpus.active() {
        let Some(dossier) = lineage.current() else {
            continue;
        };
        let label = health.corpus.label(&dossier.engram);
        if !health.verifiers.holds(&dossier.engram) {
            dormant.push(label.clone());
        }
        if matches!(dossier.standing(health.today, health.ladder), Standing::Due) {
            let late = dossier.days_overdue(health.today, health.ladder);
            if late > GRACE_DAYS {
                overdue.push(format!(
                    "{label}  {}",
                    relic_core::fmt::plural(
                        usize::try_from(late).unwrap_or(usize::MAX),
                        "day late",
                        "days late"
                    )
                ));
            }
        }
        if dossier.aided_mismatch {
            mismatched.push(label);
        }
    }

    let mut findings = Vec::new();
    if !dormant.is_empty() {
        findings.push(
            drill_station()
                .soft(summary(
                    "a lineage on the schedule has no verifier on this machine, so it cannot be drilled",
                ))
                .detailed_with(Detail::new(dormant.join("\n")))
                .fixed_by(FixHint::lossy("rote")),
        );
    }
    if !overdue.is_empty() {
        findings.push(
            drill_station()
                .soft(summary("a drill is overdue"))
                .detailed_with(Detail::new(overdue.join("\n")))
                .fixed_by(FixHint::lossy("rote")),
        );
    }
    if !mismatched.is_empty() {
        findings.push(
            drill_station()
                .soft(summary(
                    "an aided capture was refused, so the vault and the verifier hold different secrets",
                ))
                .detailed_with(Detail::new(mismatched.join("\n")))
                .fixed_by(FixHint::lossy(
                    "confirm which one is current, then rote rotate",
                )),
        );
    }
    findings
}

/// What this machine is.
fn machine_findings(health: &Health<'_>) -> Vec<Finding> {
    match health.flagship {
        Flagship::Here => Vec::new(),
        Flagship::Absent => vec![
            machine_station()
                .note(summary(
                    "this machine is not the flagship, so rote will not write here",
                ))
                .detailed_with(Detail::new(format!(
                    "{} is absent\nthis machine is {}",
                    health.paths.marker,
                    crate::machine::label(health.machine, health.host)
                ))),
        ],
        Flagship::Elsewhere { named } => vec![
            machine_station()
                .soft(summary(
                    "the flagship marker names another machine, so rote will not write here",
                ))
                .detailed_with(Detail::new(format!(
                    "{} names {named}\nthis machine is {}",
                    health.paths.marker,
                    crate::machine::label(health.machine, health.host)
                ))),
        ],
    }
}

/// A report for the case where the corpus could not be read at all. Not knowing
/// is not a clean bill of health.
pub fn unreadable(why: &str) -> Report {
    Report::ran(
        station(),
        vec![
            corpus_station()
                .finds(Severity::Broken, summary("the corpus could not be read"))
                .detailed_with(Detail::new(why)),
        ],
    )
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
            style.red(&format!("!!> rote broken — {} to look at", findings.len()))
        }
    };
    lines.push(verdict);
    lines.join("\n")
}
