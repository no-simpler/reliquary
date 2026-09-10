//! The asked tier: producers that have to be run, and never on the prompt path.
//!
//! Two things keep a subprocess out from under the prompt. A cached answer is
//! rendered while its replacement is fetched, and the fetch happens in a
//! detached child holding a per-source lock, so twenty shells waking at once
//! produce one run rather than twenty.
//!
//! The one exception is a genuinely cold cache, which blocks under a hard cap.
//! It happens once per source per machine, and the alternative is a first shell
//! that shows nothing at all.

use std::process::Stdio;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use jiff::Timestamp;
use relic_core::finding::{Finding, Outcome, Report, StationId, Summary};
use relic_core::tool::Tool;

use crate::cache::{self, Cached, Freshness};
use crate::paths::Paths;
use crate::source::{Ask, Kind, Source, Tier};

/// The hard cap on a cold, blocking fetch.
pub const COLD_BUDGET: Duration = Duration::from_millis(300);
/// The cap on a background fetch, which nobody is waiting for but which must
/// still not sit on the lock forever.
pub const BACKGROUND_BUDGET: Duration = Duration::from_secs(20);

/// What an asked source is doing right now, for `sources` to report.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Answered, and the answer is good.
    Fresh,
    /// Answered, and a replacement is on its way.
    Stale,
    /// Never answered on this machine.
    Cold,
    /// Its program is not on this machine.
    Dormant,
}

/// Read an asked source without running it, and say whether it needs running.
pub fn peek(paths: &Paths, source: &Source, ask: &Ask, now: Timestamp) -> (Vec<Finding>, State) {
    if Tool::find(ask.program()).is_none() {
        return (Vec::new(), State::Dormant);
    }
    let cached = Cached::load(&paths.cache(source.id.as_str()));
    let keys = cache::fingerprint(&ask.keys, now);
    let state = match cache::freshness(cached.as_ref(), ask, now, &keys) {
        Freshness::Fresh => State::Fresh,
        Freshness::Stale => State::Stale,
        Freshness::Cold => State::Cold,
    };
    let findings = cached.map(|record| record.report.findings().to_vec());
    (findings.unwrap_or_default(), state)
}

/// Run one asked source and store what it said.
///
/// # Errors
///
/// When the program is absent, could not be run, or answered with something
/// that is not a report.
pub fn refresh(
    paths: &Paths,
    source: &Source,
    ask: &Ask,
    now: Timestamp,
    budget: Duration,
) -> Result<Report> {
    match attempt(source, ask, budget) {
        Ok(report) => {
            store(paths, source, ask, report.clone(), now)?;
            Ok(report)
        }
        Err(error) => {
            // A failure is stamped too, so it ages out on the same clock a
            // success does. Without this the cache stays stale, and a producer
            // that is reliably broken forks a background refresh from every
            // prompt on the machine, forever.
            let why = format!("{error:#}");
            let skipped = Report::skipped(source.id.clone(), Summary::lossy(&why));
            store(paths, source, ask, skipped, now)?;
            Err(error)
        }
    }
}

fn attempt(source: &Source, ask: &Ask, budget: Duration) -> Result<Report> {
    let tool =
        Tool::find(ask.program()).ok_or_else(|| anyhow!("{} is not on PATH", ask.program()))?;
    let mut command = tool.command();
    command.args(ask.args());
    let exit = tool
        .run_within(&mut command, budget)
        .with_context(|| format!("running {}", ask.program()))?;
    // A findings producer reports its grade through its exit status, so a
    // non-zero one there is the answer rather than a failure. A text producer
    // has no such channel, so for it the status means what it usually means.
    if ask.kind == Kind::Text && !exit.ok() {
        let code = exit
            .code
            .map_or_else(|| "a signal".to_owned(), |code| code.to_string());
        let stderr = exit.stderr.trim();
        bail!(
            "{} exited {code}{}",
            ask.program(),
            if stderr.is_empty() {
                String::new()
            } else {
                format!(": {stderr}")
            }
        );
    }
    parse(source, ask.kind, &exit.stdout)
}

fn store(paths: &Paths, source: &Source, ask: &Ask, report: Report, now: Timestamp) -> Result<()> {
    let keys = cache::fingerprint(&ask.keys, now);
    cache::Cached::store(&paths.cache(source.id.as_str()), report, keys, now)
}

/// Why a source last answered with nothing, when that was a failure.
pub fn failure(paths: &Paths, id: &str) -> Option<String> {
    match Cached::load(&paths.cache(id))?.report.outcome {
        Outcome::Skipped(reason) => Some(reason.as_str().to_owned()),
        Outcome::Ran(_) => None,
    }
}

/// Turn a producer's stdout into a report.
///
/// # Errors
///
/// When a findings source did not answer with a report.
pub fn parse(source: &Source, kind: Kind, stdout: &str) -> Result<Report> {
    match kind {
        Kind::Findings => {
            let trimmed = stdout.trim();
            if trimmed.is_empty() {
                return Ok(Report::ran(source.id.clone(), Vec::new()));
            }
            let report: Report = serde_json::from_str(trimmed)
                .with_context(|| format!("{} did not answer with a report", source.id))?;
            Ok(relabel(source, report))
        }
        Kind::Text => Ok(Report::ran(
            source.id.clone(),
            text_findings(source, stdout),
        )),
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

fn text_findings(source: &Source, stdout: &str) -> Vec<Finding> {
    stdout
        .lines()
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

/// Ask for a refresh that this process will not wait for.
///
/// The child is put in its own process group so it survives the shell that
/// spawned it moving on, and its streams go nowhere so nothing it prints can
/// land in the middle of a prompt.
pub fn spawn_detached(id: &str) -> Result<()> {
    use std::os::unix::process::CommandExt;

    let exe = std::env::current_exe().context("locating coop")?;
    std::process::Command::new(exe)
        .arg("refresh")
        .arg("--source")
        .arg(id)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .context("spawning a background refresh")?;
    Ok(())
}

/// Hold the refresh lock for one source, or discover somebody else has it.
///
/// # Errors
///
/// When the lock file cannot be created.
pub fn claim(paths: &Paths, id: &str) -> Result<Option<relic_core::lock::Lock>> {
    let path = paths.refresh_lock(id);
    if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    Ok(relic_core::lock::Lock::try_acquire(&path)?)
}

/// Every asked source in a set, for the commands that walk them.
pub fn asked(sources: &[Source]) -> Vec<(&Source, &Ask)> {
    sources
        .iter()
        .filter_map(|source| match &source.tier {
            Tier::Ask(ask) => Some((source, ask)),
            Tier::When(_) => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{parse, strip_ansi};
    use crate::predicate::Predicate;
    use crate::source::{Kind, Source, Tier};
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
}
