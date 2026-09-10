//! The drill log: one JSON object per line, appended and never rewritten.
//!
//! JSON Lines rather than the frontmatter documents and whole-file JSON the rest
//! of the lane uses, because this file is meant to outlive the machine. An
//! append is one `O_APPEND` line write, so no later write can rewrite an earlier
//! record; a whole-file replacement re-serialises the past on every save, and a
//! bug in that path rewrites history rather than failing.
//!
//! Every record carries `prev`, the SHA-256 of the preceding line. That is
//! **tamper evidence and corruption detection, not a control** — anyone who can
//! edit the file can recompute the chain. It is worth carrying because it turns
//! "has this decade-long record been edited or truncated" from unknowable into
//! checkable.
//!
//! Nothing here holds secret material. The verifiers live elsewhere, and no
//! field records the input, its length, or any prefix.

use std::fmt;
use std::str::FromStr;

use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::ladder::Class;
use crate::slug::Slug;

/// The schema this binary writes and understands.
pub const SCHEMA: u32 = 2;

/// A SHA-256 over one log line, rendered as lowercase hex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest([u8; 32]);

impl Digest {
    /// What the first record's `prev` holds.
    pub const GENESIS: Self = Self([0; 32]);

    /// The digest of one log line, excluding its terminating newline.
    pub fn of(line: &str) -> Self {
        let mut out = [0u8; 32];
        out.copy_from_slice(&Sha256::digest(line.as_bytes()));
        Self(out)
    }
}

/// A `prev` field that is not 64 hex characters.
#[derive(Debug, thiserror::Error)]
#[error("a chain digest is 64 hex characters")]
pub struct BadDigest;

impl FromStr for Digest {
    type Err = BadDigest;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bytes = text.as_bytes();
        if bytes.len() != 64 {
            return Err(BadDigest);
        }
        let mut out = [0u8; 32];
        for (slot, pair) in out.iter_mut().zip(bytes.as_chunks::<2>().0) {
            let [hi, lo] = *pair;
            *slot = (hex_nibble(hi)? << 4) | hex_nibble(lo)?;
        }
        Ok(Self(out))
    }
}

fn hex_nibble(byte: u8) -> Result<u8, BadDigest> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        _ => Err(BadDigest),
    }
}

impl TryFrom<String> for Digest {
    type Error = BadDigest;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Digest> for String {
    fn from(digest: Digest) -> Self {
        digest.to_string()
    }
}

impl fmt::Display for Digest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Identifies one sitting, so the entries typed in it can be told apart from a
/// second visit the same day.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SessionId(String);

impl SessionId {
    /// A fresh identifier, from the given randomness.
    pub fn from_bytes(bytes: [u8; 8]) -> Self {
        let mut text = String::with_capacity(16);
        for byte in bytes {
            let _ = fmt::Write::write_fmt(&mut text, format_args!("{byte:02x}"));
        }
        Self(text)
    }

    /// The identifier as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A session identifier that is not 16 hex characters.
#[derive(Debug, thiserror::Error)]
#[error("a session identifier is 16 hex characters")]
pub struct BadSessionId;

impl TryFrom<String> for SessionId {
    type Error = BadSessionId;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        if text.len() == 16 && text.bytes().all(|b| hex_nibble(b).is_ok()) {
            Ok(Self(text))
        } else {
            Err(BadSessionId)
        }
    }
}

impl From<SessionId> for String {
    fn from(id: SessionId) -> Self {
        id.0
    }
}

impl fmt::Display for SessionId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// One line of the log.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Record {
    /// Schema version. A record from a newer schema stops the run rather than
    /// being half-read.
    pub v: u32,
    /// When it was written.
    pub at: Timestamp,
    /// The drill day it was credited to, frozen at write time so a later change
    /// of timezone cannot re-date history.
    pub day: Date,
    /// The machine that wrote it. The log has one writer by design.
    pub host: String,
    /// SHA-256 of the preceding line.
    pub prev: Digest,
    /// What happened.
    pub event: Event,
}

/// What a record records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Event {
    /// A slug was enrolled.
    Add(Added),
    /// A slug's verifier was replaced.
    Rekey(Rekeyed),
    /// A slug left the schedule. Its history stays.
    Retire(Retired),
    /// A stretch horizon was set.
    Probe(Probed),
    /// Something was typed.
    Attempt(Attempted),
}

impl Event {
    /// The slug this event is about.
    pub fn slug(&self) -> &Slug {
        match self {
            Self::Add(e) => &e.slug,
            Self::Rekey(e) => &e.slug,
            Self::Retire(e) => &e.slug,
            Self::Probe(e) => &e.slug,
            Self::Attempt(e) => &e.slug,
        }
    }

    /// The kind, as it is spelled on the wire.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Add(_) => "add",
            Self::Rekey(_) => "rekey",
            Self::Retire(_) => "retire",
            Self::Probe(_) => "probe",
            Self::Attempt(_) => "attempt",
        }
    }
}

/// A slug was enrolled.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Added {
    /// The slug.
    pub slug: Slug,
    /// Which verifier generation this is. Enrolment is 1.
    pub version: u32,
    /// Whether losing this one is unrecoverable rather than inconvenient.
    pub critical: bool,
}

/// A slug's verifier was replaced.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rekeyed {
    /// The slug.
    pub slug: Slug,
    /// The new generation.
    pub version: u32,
    /// Whether the current secret was proved before the replacement, or whether
    /// this was a re-enrolment by someone who had lost it.
    pub proved: bool,
}

/// A slug left the schedule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retired {
    /// The slug.
    pub slug: Slug,
}

/// A stretch horizon was set, or cleared.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Probed {
    /// The slug.
    pub slug: Slug,
    /// The day the horizon ends and the entry becomes a probe. Absent clears a
    /// horizon that was set and is no longer wanted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<Date>,
}

/// How an attempt ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// The verifier accepted it.
    Pass,
    /// The verifier refused it.
    Fail,
    /// Nothing was offered. A failure of recall like any other, kept apart from
    /// [`Outcome::Fail`] because producing a wrong answer and having none are
    /// different readings, and the week-one curve runs through both.
    Blank,
    /// Passed over deliberately.
    Skip,
    /// The session was abandoned at this prompt.
    Abort,
}

impl Outcome {
    /// Whether recall was reached for. A blank is a failure, not an absence.
    pub fn lapsed(self) -> bool {
        matches!(self, Self::Fail | Self::Blank)
    }
}

/// One typed entry.
///
/// Nothing here is derived from the input's content: not its length, not a
/// prefix, not a character class. `corrections` counts backspaces and
/// `paste_refused` counts refused pastes, which are facts about the session
/// rather than about the secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attempted {
    /// The slug.
    pub slug: Slug,
    /// The sitting this belongs to.
    pub session: SessionId,
    /// Which verifier generation was tested.
    pub version: u32,
    /// What the entry counted as.
    pub class: Class,
    /// Which try this was within the sitting. Only the first scores.
    pub attempt: u8,
    /// How it ended.
    pub outcome: Outcome,
    /// Milliseconds from the prompt appearing to the first keystroke. The
    /// leading indicator of decay: it rises before the first failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttfk_ms: Option<u64>,
    /// Milliseconds from the first keystroke to submission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
    /// Backspaces.
    pub corrections: u32,
    /// Pastes refused at this prompt.
    pub paste_refused: u32,
    /// What the ladder said the interval was.
    pub scheduled_interval_days: u32,
    /// Days since the previous scheduled review.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual_interval_days: Option<u32>,
    /// Days since the previous entry of any kind. The honest retention
    /// interval, and what every statistic and the cutover gate read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_interval_days: Option<u32>,
    /// Whether the effective interval reached long-horizon evidence.
    pub stretch: bool,
    /// The ladder position before this entry.
    pub step_before: u8,
    /// The ladder position after it. Authoritative: replay reads this rather
    /// than recomputing, so a later change to the ladder cannot rewrite the
    /// past.
    pub step_after: u8,
}

/// A line as read back: parsed, or kept verbatim with the reason it would not
/// parse. A malformed line is never dropped — a record that vanishes from a
/// listing is worse than one that is visibly wrong.
#[derive(Clone, Debug)]
pub enum Line {
    /// It parsed.
    Parsed(Box<Record>),
    /// It did not.
    Malformed {
        /// The line as it stands on disk.
        raw: String,
        /// What the parser said.
        why: String,
    },
}

impl Line {
    /// Parse one line.
    pub fn read(raw: &str) -> Self {
        match serde_json::from_str::<Record>(raw) {
            Ok(record) => Self::Parsed(Box::new(record)),
            Err(error) => Self::Malformed {
                raw: raw.to_owned(),
                why: error.to_string(),
            },
        }
    }

    /// The record, when there is one.
    pub fn record(&self) -> Option<&Record> {
        match self {
            Self::Parsed(record) => Some(record),
            Self::Malformed { .. } => None,
        }
    }
}

/// Render a record as the single line that goes on disk.
///
/// # Errors
///
/// When the record will not serialise, which means a type in it has a broken
/// `Serialize`.
pub fn render(record: &Record) -> Result<String, serde_json::Error> {
    serde_json::to_string(record)
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::{
        Added, Attempted, Digest, Event, Line, Outcome, Record, SCHEMA, SessionId, render,
    };
    use crate::ladder::Class;

    fn session() -> SessionId {
        SessionId::from_bytes([1, 2, 3, 4, 5, 6, 7, 8])
    }

    fn attempt() -> Record {
        Record {
            v: SCHEMA,
            at: "2026-09-10T07:12:03Z".parse().unwrap(),
            day: date(2026, 9, 10),
            host: "Mac".to_owned(),
            prev: Digest::GENESIS,
            event: Event::Attempt(Attempted {
                slug: "escrow-p".parse().unwrap(),
                session: session(),
                version: 1,
                class: Class::Review,
                attempt: 1,
                outcome: Outcome::Pass,
                ttfk_ms: Some(1_200),
                total_ms: Some(4_100),
                corrections: 0,
                paste_refused: 0,
                scheduled_interval_days: 7,
                actual_interval_days: Some(7),
                effective_interval_days: Some(7),
                stretch: false,
                step_before: 4,
                step_after: 4,
            }),
        }
    }

    #[test]
    fn a_record_round_trips_through_one_line() {
        let record = attempt();
        let line = render(&record).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(serde_json::from_str::<Record>(&line).unwrap(), record);
    }

    #[test]
    fn the_wire_form_is_flat_enough_to_grep() {
        let line = render(&attempt()).unwrap();
        assert!(line.contains("\"kind\":\"attempt\""));
        assert!(line.contains("\"slug\":\"escrow-p\""));
        assert!(line.contains("\"class\":\"review\""));
        assert!(line.contains("\"outcome\":\"pass\""));
    }

    #[test]
    fn an_absent_optional_interval_is_omitted_rather_than_written_as_null() {
        let mut record = attempt();
        if let Event::Attempt(ref mut entry) = record.event {
            entry.actual_interval_days = None;
            entry.effective_interval_days = None;
            entry.ttfk_ms = None;
            entry.total_ms = None;
        }
        let line = render(&record).unwrap();
        assert!(!line.contains("null"));
        assert_eq!(serde_json::from_str::<Record>(&line).unwrap(), record);
    }

    #[test]
    fn an_unreadable_line_is_kept_rather_than_dropped() {
        let line = Line::read("{ this is not json");
        assert!(line.record().is_none());
        match line {
            Line::Malformed { raw, why } => {
                assert_eq!(raw, "{ this is not json");
                assert!(!why.is_empty());
            }
            Line::Parsed(_) => panic!("should not have parsed"),
        }
    }

    #[test]
    fn an_unknown_top_level_key_is_refused() {
        let mut value: serde_json::Value =
            serde_json::from_str(&render(&attempt()).unwrap()).unwrap();
        value["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_str::<Record>(&value.to_string()).is_err());
    }

    #[test]
    fn an_unknown_key_inside_an_event_is_refused_too() {
        let mut value: serde_json::Value =
            serde_json::from_str(&render(&attempt()).unwrap()).unwrap();
        value["event"]["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_str::<Record>(&value.to_string()).is_err());
    }

    #[test]
    fn the_chain_digest_round_trips_and_refuses_a_bad_one() {
        let digest = Digest::of("a line");
        let text = digest.to_string();
        assert_eq!(text.len(), 64);
        assert_eq!(text.parse::<Digest>().unwrap(), digest);
        assert!("zz".parse::<Digest>().is_err());
        assert_eq!(Digest::GENESIS.to_string(), "0".repeat(64));
    }

    #[test]
    fn a_different_line_gives_a_different_digest() {
        assert_ne!(Digest::of("a"), Digest::of("b"));
    }

    #[test]
    fn a_session_identifier_round_trips_and_refuses_a_bad_one() {
        let id = session();
        assert_eq!(id.as_str(), "0102030405060708");
        assert_eq!(
            serde_json::from_str::<SessionId>(&serde_json::to_string(&id).unwrap()).unwrap(),
            id
        );
        assert!(serde_json::from_str::<SessionId>("\"short\"").is_err());
    }

    #[test]
    fn every_event_names_its_slug_and_its_kind() {
        let event = Event::Add(Added {
            slug: "op-master".parse().unwrap(),
            version: 1,
            critical: false,
        });
        assert_eq!(event.slug().as_str(), "op-master");
        assert_eq!(event.kind(), "add");
    }
}
