//! One shape for what a check found, and one derivation for what it means.
//!
//! The machine's verification surface is many checks in many programs: stations
//! inside `assay`, and every registered binary that answers `doctor --format
//! json`. They agree on this module and on nothing else, which is what lets an
//! aggregator collect from a relic whose source it has never read.
//!
//! The shape is [SARIF]-derived — the OASIS format heterogeneous analyzers emit
//! so one collector can consume them all — reduced to what this machine uses.
//!
//! **Grading is derived, never tallied.** [`Grade::of`] reads a finding set;
//! nothing counts warnings into a mutable total on the side, which is the defect
//! the bash checkers carry ("they mutate FAILS/WARNS via the helpers, so never
//! call in `$(...)`").
//!
//! [SARIF]: https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html

use std::fmt;
use std::str::FromStr;

use camino::Utf8PathBuf;
use serde::{Deserialize, Serialize};

/// What a finding says about the machine.
///
/// The test between the two that grade, unchanged since it was written down:
/// **does this mean the machine is no longer reproducible from the repo, or that
/// something is silently disarmed?** If so it is [`Severity::Broken`]. A machine
/// that is merely degraded — a budget exceeded, two dialects drifted apart in
/// meaning — is [`Severity::Soft`].
///
/// There is no `Ok` variant. A finding that says nothing is wrong is not a
/// finding; the absence of findings is what [`Grade::Ok`] means.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Severity {
    /// Worth reading, and not a defect — most often a check that could not judge
    /// one item: a tap this machine has not installed, a roster not cached, a
    /// definition still encrypted.
    ///
    /// The per-item counterpart of [`Outcome::Skipped`], and it grades the same
    /// way: not at all. A gate that reddens on something the reader cannot fix
    /// where it fires teaches people to bypass the gate, which costs every other
    /// check too.
    Note,
    /// The machine is degraded, and still reproducible.
    Soft,
    /// The machine cannot be reproduced from the repo, or a guard is disarmed.
    Broken,
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Note => "note",
            Self::Soft => "soft",
            Self::Broken => "broken",
        })
    }
}

/// The verdict over a set of findings, and the process exit that carries it.
///
/// `0`/`1`/`2` is what `check-bedrock` and `check-brew-health` already agreed on
/// separately; it is the convention here because two of the surfaces being
/// subsumed had converged on it unprompted.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Grade {
    /// Nothing to report.
    Ok,
    /// At least one [`Severity::Soft`] finding, and no worse.
    Soft,
    /// At least one [`Severity::Broken`] finding.
    Broken,
}

impl Grade {
    /// The verdict a finding set carries.
    #[must_use]
    pub fn of<'a>(findings: impl IntoIterator<Item = &'a Finding>) -> Self {
        findings
            .into_iter()
            .map(|finding| match finding.severity {
                Severity::Note => Self::Ok,
                Severity::Soft => Self::Soft,
                Severity::Broken => Self::Broken,
            })
            .max()
            .unwrap_or(Self::Ok)
    }

    /// The verdict over a whole run.
    #[must_use]
    pub fn across<'a>(reports: impl IntoIterator<Item = &'a Report>) -> Self {
        reports
            .into_iter()
            .map(Report::grade)
            .max()
            .unwrap_or(Self::Ok)
    }

    /// The process exit status this verdict is reported as.
    #[must_use]
    pub fn exit_code(self) -> u8 {
        match self {
            Self::Ok => 0,
            Self::Soft => 1,
            Self::Broken => 2,
        }
    }
}

impl fmt::Display for Grade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Ok => "ok",
            Self::Soft => "soft",
            Self::Broken => "broken",
        })
    }
}

/// A name is not a station id until it has been parsed as one.
#[derive(Clone, PartialEq, Eq, Debug, thiserror::Error)]
pub enum BadStationId {
    /// Nothing, or only whitespace.
    #[error("a station id may not be empty")]
    Empty,
    /// A character outside the kebab-case alphabet.
    #[error("a station id is lowercase letters, digits and hyphens: {0:?}")]
    Alphabet(String),
}

/// Which station a finding came from.
///
/// Kebab-case, because it is also the token that selects a station on the
/// command line and the key it is written under in JSON.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct StationId(String);

impl StationId {
    /// An id from a literal in our own source, taken on trust.
    ///
    /// The strict [`FromStr`] is for a name that came from outside. This is for
    /// a roster entry, where a fallible constructor would make building the
    /// roster fallible for a property the source already fixes. The check is
    /// not skipped, only moved: a station suite is expected to re-parse every
    /// id it publishes, which is what `assay`'s roster test does.
    #[must_use]
    pub fn from_static(name: &'static str) -> Self {
        Self(name.to_owned())
    }

    /// The id as it is spelled.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A finding from this station, at this severity.
    ///
    /// Findings are minted through the id rather than assembled beside it, so a
    /// station cannot stamp another station's name on its own report.
    #[must_use]
    pub fn finds(&self, severity: Severity, summary: Summary) -> Finding {
        Finding {
            station: self.clone(),
            severity,
            summary,
            detail: None,
            fix: None,
            location: None,
        }
    }

    /// A [`Severity::Soft`] finding from this station.
    #[must_use]
    pub fn soft(&self, summary: Summary) -> Finding {
        self.finds(Severity::Soft, summary)
    }

    /// A [`Severity::Broken`] finding from this station.
    #[must_use]
    pub fn broken(&self, summary: Summary) -> Finding {
        self.finds(Severity::Broken, summary)
    }

    /// A [`Severity::Note`] from this station: read it, do not grade on it.
    #[must_use]
    pub fn note(&self, summary: Summary) -> Finding {
        self.finds(Severity::Note, summary)
    }
}

impl FromStr for StationId {
    type Err = BadStationId;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(BadStationId::Empty);
        }
        if !trimmed
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
        {
            return Err(BadStationId::Alphabet(trimmed.to_owned()));
        }
        Ok(Self(trimmed.to_owned()))
    }
}

impl TryFrom<String> for StationId {
    type Error = BadStationId;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<StationId> for String {
    fn from(id: StationId) -> Self {
        id.0
    }
}

impl fmt::Display for StationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// A line of prose that is not one line.
#[derive(Clone, Copy, PartialEq, Eq, Debug, thiserror::Error)]
pub enum NotOneLine {
    /// Nothing, or only whitespace.
    #[error("expected one line, and there was nothing")]
    Empty,
    /// More than one line.
    #[error("expected one line, and there was a line break")]
    Multiline,
}

/// One line saying what is wrong. Every finding has one.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Summary(String);

impl Summary {
    /// The longest a summary may be before [`Summary::lossy`] cuts it.
    pub const MAX: usize = 200;

    /// The line as it is spelled.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// One line out of text from outside, without the option of refusing.
    ///
    /// The strict [`FromStr`] is for prose we write. This is for what another
    /// program said — a brew error, a git message, a panic — where refusing to
    /// summarise loses the report entirely and truncating loses a tail. Line
    /// breaks and runs of whitespace collapse to single spaces; the full text
    /// belongs in a [`Detail`] beside it.
    #[must_use]
    pub fn lossy(text: &str) -> Self {
        let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if flat.is_empty() {
            return Self("(no message)".to_owned());
        }
        if flat.chars().count() > Self::MAX {
            let cut: String = flat.chars().take(Self::MAX - 1).collect();
            return Self(cut + "\u{2026}");
        }
        Self(flat)
    }
}

impl FromStr for Summary {
    type Err = NotOneLine;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if trimmed.is_empty() {
            return Err(NotOneLine::Empty);
        }
        if trimmed.contains('\n') {
            return Err(NotOneLine::Multiline);
        }
        Ok(Self(trimmed.to_owned()))
    }
}

impl TryFrom<String> for Summary {
    type Error = NotOneLine;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<Summary> for String {
    fn from(summary: Summary) -> Self {
        summary.0
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// The evidence under a summary. Free to run to several lines.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Detail(String);

impl Detail {
    /// Evidence, or nothing when there was none to give.
    #[must_use]
    pub fn new(text: impl Into<String>) -> Option<Self> {
        let text: String = text.into();
        let trimmed = text.trim_end();
        (!trimmed.trim().is_empty()).then(|| Self(trimmed.to_owned()))
    }

    /// The evidence as it is spelled.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Detail {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// What to do about it: one line, and an imperative one.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct FixHint(String);

impl FixHint {
    /// The hint as it is spelled.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// One line out of text that must not be refused. See [`Summary::lossy`].
    #[must_use]
    pub fn lossy(text: &str) -> Self {
        Self(Summary::lossy(text).0)
    }
}

impl FromStr for FixHint {
    type Err = NotOneLine;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Summary::from_str(s).map(|line| Self(line.0))
    }
}

impl TryFrom<String> for FixHint {
    type Error = NotOneLine;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<FixHint> for String {
    fn from(hint: FixHint) -> Self {
        hint.0
    }
}

impl fmt::Display for FixHint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where a finding is, when it is anywhere in particular.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Location {
    /// The file the finding is about.
    pub path: Utf8PathBuf,
    /// The line within it, 1-based, when the finding is that precise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<usize>,
}

impl Location {
    /// A whole file.
    #[must_use]
    pub fn file(path: impl Into<Utf8PathBuf>) -> Self {
        Self {
            path: path.into(),
            line: None,
        }
    }

    /// One line of a file, 1-based.
    #[must_use]
    pub fn at_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.line {
            Some(line) => write!(f, "{}:{line}", self.path),
            None => write!(f, "{}", self.path),
        }
    }
}

/// One thing a station found.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Finding {
    /// Which station found it.
    pub station: StationId,
    /// What it means for the machine.
    pub severity: Severity,
    /// One line saying what is wrong.
    pub summary: Summary,
    /// The evidence.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<Detail>,
    /// What to do about it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<FixHint>,
    /// Where it is.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
}

impl Finding {
    /// Attaches the evidence.
    #[must_use]
    pub fn detailed(mut self, detail: Detail) -> Self {
        self.detail = Some(detail);
        self
    }

    /// Attaches the evidence, when there is any.
    ///
    /// [`Detail::new`] already answers "was there anything to say?", and every
    /// caller that builds evidence from text it did not write has to ask. This
    /// is that answer applied, rather than the same `if let` written out at
    /// each site.
    #[must_use]
    pub fn detailed_with(mut self, detail: Option<Detail>) -> Self {
        self.detail = detail;
        self
    }

    /// Attaches what to do about it.
    #[must_use]
    pub fn fixed_by(mut self, fix: FixHint) -> Self {
        self.fix = Some(fix);
        self
    }

    /// Attaches where it is.
    #[must_use]
    pub fn at(mut self, location: Location) -> Self {
        self.location = Some(location);
        self
    }
}

/// What one station produced.
///
/// A station that could not run is [`Outcome::Skipped`] and grades [`Grade::Ok`]
/// — a skip is a fact, not a fault. A station that *should* have run and threw
/// instead is not represented here at all: the runner converts that into a
/// [`Severity::Broken`] finding, so a crashed check is never a quiet pass.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub enum Outcome {
    /// The station ran. An empty set means it found nothing.
    Ran(Vec<Finding>),
    /// The station declined to run, and said why.
    Skipped(Summary),
}

/// One station's contribution to a run.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Report {
    /// The station.
    pub station: StationId,
    /// What it produced.
    pub outcome: Outcome,
}

impl Report {
    /// A station that ran.
    #[must_use]
    pub fn ran(station: StationId, findings: Vec<Finding>) -> Self {
        Self {
            station,
            outcome: Outcome::Ran(findings),
        }
    }

    /// A station that declined, and why.
    #[must_use]
    pub fn skipped(station: StationId, reason: Summary) -> Self {
        Self {
            station,
            outcome: Outcome::Skipped(reason),
        }
    }

    /// What this station's findings amount to. A skip is [`Grade::Ok`].
    #[must_use]
    pub fn grade(&self) -> Grade {
        match &self.outcome {
            Outcome::Ran(findings) => Grade::of(findings),
            Outcome::Skipped(_) => Grade::Ok,
        }
    }

    /// What it found, or nothing when it was skipped.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        match &self.outcome {
            Outcome::Ran(findings) => findings,
            Outcome::Skipped(_) => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station() -> StationId {
        "bedrock".parse().expect("a valid id")
    }

    fn line(text: &str) -> Summary {
        text.parse().expect("one line")
    }

    #[test]
    fn a_station_id_is_kebab_case_or_nothing() {
        assert!("brew-health".parse::<StationId>().is_ok());
        assert_eq!("".parse::<StationId>(), Err(BadStationId::Empty));
        assert_eq!("   ".parse::<StationId>(), Err(BadStationId::Empty));
        assert!(matches!(
            "Brew_Health".parse::<StationId>(),
            Err(BadStationId::Alphabet(_))
        ));
    }

    #[test]
    fn a_summary_is_one_line() {
        assert_eq!(
            line("  something is wrong  ").as_str(),
            "something is wrong"
        );
        assert_eq!("".parse::<Summary>(), Err(NotOneLine::Empty));
        assert_eq!("two\nlines".parse::<Summary>(), Err(NotOneLine::Multiline));
    }

    #[test]
    fn a_lossy_summary_never_refuses() {
        assert_eq!(Summary::lossy("a\n  b\tc").as_str(), "a b c");
        assert_eq!(Summary::lossy("   ").as_str(), "(no message)");
        let long = Summary::lossy(&"x".repeat(500));
        assert_eq!(long.as_str().chars().count(), Summary::MAX);
        assert!(long.as_str().ends_with('\u{2026}'));
        assert!(long.as_str().parse::<Summary>().is_ok());
    }

    #[test]
    fn empty_evidence_is_no_evidence() {
        assert!(Detail::new("   \n\n").is_none());
        assert_eq!(
            Detail::new("one\ntwo\n").expect("evidence").as_str(),
            "one\ntwo"
        );
    }

    #[test]
    fn a_grade_is_the_worst_finding() {
        let id = station();
        assert_eq!(Grade::of(&[]), Grade::Ok);
        assert_eq!(Grade::of(&[id.soft(line("a"))]), Grade::Soft);
        assert_eq!(
            Grade::of(&[id.soft(line("a")), id.broken(line("b"))]),
            Grade::Broken
        );
    }

    #[test]
    fn a_note_is_read_and_not_graded() {
        let id = station();
        assert_eq!(
            Grade::of(&[id.note(line("a tap is not present here"))]),
            Grade::Ok
        );
        assert_eq!(
            Grade::of(&[id.note(line("unjudged")), id.soft(line("degraded"))]),
            Grade::Soft
        );
    }

    #[test]
    fn a_skip_is_not_a_failure() {
        let report = Report::skipped(station(), line("the archive is still encrypted"));
        assert_eq!(report.grade(), Grade::Ok);
        assert!(report.findings().is_empty());
    }

    #[test]
    fn grades_carry_the_exit_convention_the_scripts_agreed_on() {
        assert_eq!(Grade::Ok.exit_code(), 0);
        assert_eq!(Grade::Soft.exit_code(), 1);
        assert_eq!(Grade::Broken.exit_code(), 2);
    }

    #[test]
    fn a_run_grades_across_its_stations() {
        let bedrock = station();
        let brew: StationId = "brew-health".parse().expect("a valid id");
        let reports = vec![
            Report::ran(bedrock.clone(), vec![]),
            Report::skipped(brew.clone(), line("homebrew is not installed")),
        ];
        assert_eq!(Grade::across(&reports), Grade::Ok);

        let reports = vec![
            Report::ran(bedrock, vec![]),
            Report::ran(brew.clone(), vec![brew.soft(line("deprecated"))]),
        ];
        assert_eq!(Grade::across(&reports), Grade::Soft);
    }

    #[test]
    fn a_finding_round_trips_through_the_wire_format() {
        let id = station();
        let finding = id
            .broken(line("bash 5 is not on PATH"))
            .detailed(Detail::new("found /bin/bash 3.2").expect("evidence"))
            .fixed_by("brew install bash".parse().expect("one line"))
            .at(Location::file("/etc/paths").at_line(3));

        let json = serde_json::to_string(&finding).expect("serializable");
        let back: Finding = serde_json::from_str(&json).expect("deserializable");
        assert_eq!(back, finding);
    }

    #[test]
    fn the_wire_format_refuses_a_field_it_does_not_know() {
        let json = r#"{"station":"bedrock","severity":"soft","summary":"x","kind":"y"}"#;
        assert!(serde_json::from_str::<Finding>(json).is_err());
    }

    #[test]
    fn the_wire_format_refuses_a_summary_that_is_not_one_line() {
        let json = r#"{"station":"bedrock","severity":"soft","summary":"a\nb"}"#;
        assert!(serde_json::from_str::<Finding>(json).is_err());
    }

    #[test]
    fn an_outcome_round_trips_through_the_wire_format() {
        let report = Report::skipped(station(), line("nothing to do"));
        let json = serde_json::to_string(&report).expect("serializable");
        assert_eq!(
            json,
            r#"{"station":"bedrock","outcome":{"skipped":"nothing to do"}}"#
        );
        let back: Report = serde_json::from_str(&json).expect("deserializable");
        assert_eq!(back, report);
    }
}
