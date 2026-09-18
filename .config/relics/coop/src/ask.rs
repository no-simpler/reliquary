//! The asked and read tiers: a producer's answer, taken fresh at every prompt.
//!
//! coop holds no answer of its own. An asked producer runs under a hard budget
//! and is killed past it; a read producer wrote its report on its own cadence
//! and coop only reads the file. Whatever caching either needs is theirs,
//! because only the producer knows when its truth changes.

use std::time::Duration;

use anyhow::{Context, Result};
use jiff::Timestamp;
use relic_core::finding::{Finding, Outcome, Report, StationId, Summary};
use relic_core::tool::{self, Tool};

use crate::source::{Ask, Kind, Read, Source};

/// The hard cap on an asked producer, per prompt.
pub const ASK_BUDGET: Duration = Duration::from_millis(50);

/// What a producer is doing right now, measured at the moment of asking.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Standing {
    /// Answered.
    Live,
    /// Did not answer within the budget and was killed.
    Slow,
    /// Could not be run, or answered with something that is not an answer.
    Failing,
    /// Its program, or its report, is not on this machine.
    Dormant,
}

/// One producer's answer this prompt.
#[derive(Clone, Debug)]
pub struct Answer {
    /// What it said. Empty unless [`Standing::Live`].
    pub findings: Vec<Finding>,
    /// How it went.
    pub standing: Standing,
    /// What went wrong, when something did.
    pub why: Option<String>,
}

impl Answer {
    fn live(findings: Vec<Finding>) -> Self {
        Self {
            findings,
            standing: Standing::Live,
            why: None,
        }
    }

    fn silent(standing: Standing, why: Option<String>) -> Self {
        Self {
            findings: Vec::new(),
            standing,
            why,
        }
    }
}

/// Run an asked producer under the budget.
pub fn run(source: &Source, ask: &Ask) -> Answer {
    let Some(tool) = Tool::find(ask.program()) else {
        return Answer::silent(Standing::Dormant, None);
    };
    let mut command = tool.command();
    command.args(ask.args());
    let exit = match tool.run_within(&mut command, ASK_BUDGET) {
        Ok(exit) => exit,
        Err(error @ tool::Error::TimedOut { .. }) => {
            return Answer::silent(Standing::Slow, Some(format!("{error:#}")));
        }
        Err(error) => return Answer::silent(Standing::Failing, Some(format!("{error:#}"))),
    };
    // A findings producer reports its grade through its exit status, so a
    // non-zero one there is the answer rather than a failure. A text producer
    // has no such channel, so for it the status means what it usually means.
    if ask.kind == Kind::Text && !exit.ok() {
        let code = exit
            .code
            .map_or_else(|| "a signal".to_owned(), |code| code.to_string());
        let stderr = exit.stderr.trim();
        let why = if stderr.is_empty() {
            format!("{} exited {code}", ask.program())
        } else {
            format!("{} exited {code}: {stderr}", ask.program())
        };
        return Answer::silent(Standing::Failing, Some(why));
    }
    answer(source, ask.kind, &exit.stdout)
}

/// Read a producer's report.
pub fn read(source: &Source, read: &Read) -> Answer {
    match fs_err::read_to_string(&read.path) {
        Ok(text) => answer(source, read.kind, &text),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            Answer::silent(Standing::Dormant, None)
        }
        Err(error) => Answer::silent(Standing::Failing, Some(format!("{error:#}"))),
    }
}

/// How long past its promise a read report is, when it is.
pub fn overdue(read: &Read, now: Timestamp) -> Option<String> {
    let promised = read.expect_every?;
    let modified = fs_err::metadata(&read.path).ok()?.modified().ok()?;
    let since = modified.duration_since(std::time::UNIX_EPOCH).ok()?;
    let at = Timestamp::from_second(i64::try_from(since.as_secs()).ok()?).ok()?;
    let elapsed = now.as_second().saturating_sub(at.as_second());
    (u64::try_from(elapsed).unwrap_or(0) > promised.as_secs())
        .then(|| relic_core::fmt::age(at, now))
}

fn answer(source: &Source, kind: Kind, text: &str) -> Answer {
    match parse(source, kind, text) {
        Ok(report) => match report.outcome {
            Outcome::Ran(findings) => Answer::live(findings),
            Outcome::Skipped(reason) => {
                Answer::silent(Standing::Failing, Some(reason.as_str().to_owned()))
            }
        },
        Err(error) => Answer::silent(Standing::Failing, Some(format!("{error:#}"))),
    }
}

/// Turn a producer's text into a report.
///
/// # Errors
///
/// When a findings source did not answer with a report.
pub fn parse(source: &Source, kind: Kind, text: &str) -> Result<Report> {
    match kind {
        Kind::Findings => {
            let trimmed = text.trim();
            if trimmed.is_empty() {
                return Ok(Report::ran(source.id.clone(), Vec::new()));
            }
            let report: Report = serde_json::from_str(trimmed)
                .with_context(|| format!("{} did not answer with a report", source.id))?;
            Ok(relabel(source, report))
        }
        Kind::Text => Ok(Report::ran(source.id.clone(), text_findings(source, text))),
    }
}

/// Keep a producer's own station namespace, but never let it answer under
/// somebody else's name.
///
/// `assay` enforces the same rule on the binaries it collects. A source that
/// declares itself `rote` and reports as `git` would put its finding in a card
/// row nobody could trace back to a declaration.
fn relabel(source: &Source, report: Report) -> Report {
    let station = &source.id;
    let owned = |id: &StationId| {
        id == station || id.as_str().starts_with(&format!("{}-", station.as_str()))
    };
    match report.outcome {
        Outcome::Ran(findings) => Report::ran(
            station.clone(),
            findings
                .into_iter()
                .map(|finding| {
                    if owned(&finding.station) {
                        finding
                    } else {
                        let mut moved = station.finds(finding.severity, finding.summary.clone());
                        moved.detail = finding.detail;
                        moved.fix = finding.fix;
                        moved.location = finding.location;
                        moved
                    }
                })
                .collect(),
        ),
        Outcome::Skipped(reason) => Report::skipped(station.clone(), reason),
    }
}

fn text_findings(source: &Source, text: &str) -> Vec<Finding> {
    text.lines()
        .map(strip_ansi)
        .map(|line| line.trim().to_owned())
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut finding = source.id.finds(source.severity, Summary::lossy(&line));
            if let Some(fix) = source.fix.clone() {
                finding = finding.fixed_by(fix);
            }
            finding
        })
        .collect()
}

/// Remove escape sequences from a producer's line.
///
/// A text producer was written to be read by a person, so it colours itself. The
/// card owns its own colour, and a smuggled escape would also let a producer
/// move the cursor out of the box it was drawn in.
pub fn strip_ansi(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut chars = line.chars();
    while let Some(c) = chars.next() {
        if c != '\u{1b}' {
            if !c.is_control() || c == '\t' {
                out.push(c);
            }
            continue;
        }
        // CSI and OSC are the two a terminal writer reaches for; both end at a
        // byte this scan can recognise without decoding the sequence.
        match chars.next() {
            Some('[') => {
                for next in chars.by_ref() {
                    if matches!(next, '@'..='~') {
                        break;
                    }
                }
            }
            Some(']') => {
                for next in chars.by_ref() {
                    if next == '\u{7}' || next == '\u{1b}' {
                        break;
                    }
                }
            }
            _ => {}
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Standing, parse, run, strip_ansi};
    use crate::predicate::Predicate;
    use crate::source::{Ask, Kind, Source, Tier};
    use relic_core::finding::{FixHint, Severity, StationId};

    fn source(id: &'static str) -> Source {
        Source {
            id: StationId::from_static(id),
            summary: String::new(),
            summary_missing: None,
            fix: Some(FixHint::lossy("do the thing")),
            severity: Severity::Soft,
            tier: Tier::When(Predicate::Exists {
                path: "/tmp".into(),
            }),
        }
    }

    #[test]
    fn a_findings_producer_is_read_verbatim() {
        let json = r#"{"station":"rote","outcome":{"ran":[
            {"station":"rote","severity":"soft","summary":"2 drills due","fix":"rote"}]}}"#;
        let report = parse(&source("rote"), Kind::Findings, json).unwrap();
        let finding = report.findings().first().unwrap();
        assert_eq!(finding.summary.as_str(), "2 drills due");
        assert_eq!(finding.fix.as_ref().unwrap().as_str(), "rote");
    }

    #[test]
    fn a_producers_own_namespace_survives_but_a_strangers_does_not() {
        let json = r#"{"station":"rote","outcome":{"ran":[
            {"station":"rote-log","severity":"soft","summary":"a"},
            {"station":"git","severity":"soft","summary":"b"}]}}"#;
        let report = parse(&source("rote"), Kind::Findings, json).unwrap();
        let stations: Vec<&str> = report
            .findings()
            .iter()
            .map(|f| f.station.as_str())
            .collect();
        assert_eq!(stations, ["rote-log", "rote"]);
    }

    #[test]
    fn a_findings_producer_that_said_nothing_is_an_empty_report_not_an_error() {
        let report = parse(&source("x"), Kind::Findings, "   \n").unwrap();
        assert!(report.findings().is_empty());
    }

    #[test]
    fn a_findings_producer_that_answered_prose_is_an_error() {
        assert!(parse(&source("x"), Kind::Findings, "everything is fine").is_err());
    }

    #[test]
    fn a_text_producer_becomes_one_finding_per_line() {
        let report = parse(&source("x"), Kind::Text, "first\n\nsecond\n").unwrap();
        assert_eq!(report.findings().len(), 2);
        let first = report.findings().first().unwrap();
        assert_eq!(first.summary.as_str(), "first");
        assert_eq!(first.fix.as_ref().unwrap().as_str(), "do the thing");
    }

    #[test]
    fn a_text_producer_cannot_smuggle_colour_into_the_card() {
        let report = parse(&source("x"), Kind::Text, "\x1b[1;33mwarning\x1b[0m\n").unwrap();
        assert_eq!(
            report.findings().first().unwrap().summary.as_str(),
            "warning"
        );
    }

    #[test]
    fn a_text_producer_cannot_move_the_cursor_out_of_the_box() {
        assert_eq!(strip_ansi("a\x1b[2Ab"), "ab");
        assert_eq!(strip_ansi("a\x1b]0;title\x07b"), "ab");
        assert_eq!(strip_ansi("a\rb"), "ab");
    }

    #[test]
    fn ordinary_text_is_left_exactly_alone() {
        assert_eq!(strip_ansi("2 drills due — rote"), "2 drills due — rote");
    }

    #[test]
    fn a_program_that_is_not_here_is_dormant() {
        let ask = Ask {
            run: vec!["/definitely/not/here".to_owned()],
            kind: Kind::Text,
        };
        assert_eq!(run(&source("x"), &ask).standing, Standing::Dormant);
    }

    #[test]
    fn a_producer_past_the_budget_is_slow_and_says_nothing() {
        let ask = Ask {
            run: vec!["sleep".to_owned(), "2".to_owned()],
            kind: Kind::Text,
        };
        let answer = run(&source("x"), &ask);
        assert_eq!(answer.standing, Standing::Slow);
        assert!(answer.findings.is_empty());
    }
}
