//! One line of the corpus: appended, and never rewritten.
//!
//! JSON Lines rather than the frontmatter documents and whole-file JSON the
//! rest of the lane uses, because this file is meant to outlive the machine. An
//! append is one `O_APPEND` line write, so no later write can rewrite an earlier
//! record; a whole-file replacement re-serializes the past on every save, and a
//! bug in that path rewrites history rather than failing.
//!
//! Every record carries `prev`, the SHA-256 of the preceding line in its own
//! chain. That is **tamper evidence and corruption detection, not a control** —
//! anyone who can edit the file can recompute the chain. It catches edits,
//! interior deletions and a cut head, because the first line's `prev` is the
//! genesis digest. It does **not** catch truncation of the tail, and with one
//! chain per machine it does not catch a whole chain being deleted either.
//!
//! The envelope carries no machine and no position: the file a line sits in
//! names the machine, and the line's index in that file is its position.
//!
//! **A record stores what its writer believed.** `scheduled_interval_days` and
//! `rung_after` are authoritative — they are what the scheduler decided at the
//! time, so replay reads them and history stays immune to a later change in the
//! ladder. The remaining measurements are *witnesses*: `stats` recomputes them
//! from the merged corpus, and a disagreement is how a two-machine divergence is
//! found.
//!
//! Nothing here holds secret material. The verifiers live elsewhere, and no
//! field records the input, its length, or any prefix.

use std::fmt;
use std::str::FromStr;

use anyhow::{Context as _, Result};
use jiff::Timestamp;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

use crate::ladder::Occasion;
use crate::slug::Slug;

/// The schema this binary writes and understands.
pub const SCHEMA: u32 = 5;

fn hex_nibble(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        _ => None,
    }
}

fn render_hex(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len().saturating_mul(2));
    for byte in bytes {
        let _ = fmt::Write::write_fmt(&mut text, format_args!("{byte:02x}"));
    }
    text
}

/// A SHA-256 over one line, rendered as lowercase hex.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Digest([u8; 32]);

impl Digest {
    /// What the first record in a chain carries.
    pub const GENESIS: Self = Self([0; 32]);

    /// The digest of one line, excluding its terminating newline.
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
            let hi = hex_nibble(hi).ok_or(BadDigest)?;
            let lo = hex_nibble(lo).ok_or(BadDigest)?;
            *slot = (hi << 4) | lo;
        }
        Ok(Self(out))
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
        f.write_str(&render_hex(&self.0))
    }
}

/// Sixteen random bytes: the shape every minted identity takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
struct Minted([u8; 16]);

impl Minted {
    fn mint() -> Result<Self> {
        let mut bytes = [0u8; 16];
        getrandom::fill(&mut bytes).context("drawing an identity")?;
        Ok(Self(bytes))
    }

    fn parse(text: &str) -> Option<Self> {
        let bytes = text.as_bytes();
        if bytes.len() != 32 {
            return None;
        }
        let mut out = [0u8; 16];
        for (slot, pair) in out.iter_mut().zip(bytes.as_chunks::<2>().0) {
            let [hi, lo] = *pair;
            *slot = (hex_nibble(hi)? << 4) | hex_nibble(lo)?;
        }
        Some(Self(out))
    }
}

impl fmt::Display for Minted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&render_hex(&self.0))
    }
}

/// A string that is not a minted identity.
#[derive(Debug, thiserror::Error)]
#[error("an identity is 32 lowercase hex characters")]
pub struct BadId;

/// One enrolled secret, for its whole life.
///
/// Minted at enrolment and at every rotation, and never derived from the secret
/// — a hash of the secret would be an unsalted oracle, and it would make two
/// lineages holding the same phrase indistinguishable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct EngramId(Minted);

impl EngramId {
    /// A fresh identity.
    ///
    /// # Errors
    ///
    /// When the system will not supply randomness.
    pub fn mint() -> Result<Self> {
        Ok(Self(Minted::mint()?))
    }
}

impl FromStr for EngramId {
    type Err = BadId;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Minted::parse(text).map(Self).ok_or(BadId)
    }
}

impl TryFrom<String> for EngramId {
    type Error = BadId;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<EngramId> for String {
    fn from(id: EngramId) -> Self {
        id.to_string()
    }
}

impl fmt::Display for EngramId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One invocation, so the captures typed in it can be told apart from a second
/// visit the same day.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct SittingId(Minted);

impl SittingId {
    /// A fresh identity.
    ///
    /// # Errors
    ///
    /// When the system will not supply randomness.
    pub fn mint() -> Result<Self> {
        Ok(Self(Minted::mint()?))
    }
}

impl FromStr for SittingId {
    type Err = BadId;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Minted::parse(text).map(Self).ok_or(BadId)
    }
}

impl TryFrom<String> for SittingId {
    type Error = BadId;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<SittingId> for String {
    fn from(id: SittingId) -> Self {
        id.to_string()
    }
}

impl fmt::Display for SittingId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

/// One line of a chain.
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
    /// The label the writing machine wore at the time. Never an identity: the
    /// machine is named by the file this line sits in.
    pub host: String,
    /// SHA-256 of the preceding line in this chain.
    pub prev: Digest,
    /// What happened.
    pub event: Event,
}

/// What a record records.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum Event {
    /// A lineage was opened, with its first engram.
    Enroll(Enrolled),
    /// One engram was superseded by the next.
    Rotate(Rotated),
    /// An engram was given a verifier on the machine that wrote this.
    Attach(Attached),
    /// A lineage left the schedule. Its history stays.
    Retire(Retired),
    /// Something was typed.
    Capture(Captured),
}

impl Event {
    /// The engram this event is about, where it is about one. A retirement
    /// names a lineage and no engram.
    pub fn engram(&self) -> Option<EngramId> {
        match self {
            Self::Enroll(e) => Some(e.engram),
            Self::Rotate(e) => Some(e.to),
            Self::Attach(e) => Some(e.engram),
            Self::Capture(e) => Some(e.engram),
            Self::Retire(_) => None,
        }
    }

    /// The kind, as it is spelled on the wire.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Enroll(_) => "enroll",
            Self::Rotate(_) => "rotate",
            Self::Attach(_) => "attach",
            Self::Retire(_) => "retire",
            Self::Capture(_) => "capture",
        }
    }
}

/// A lineage was opened.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Enrolled {
    /// The lineage.
    pub slug: Slug,
    /// Its first engram.
    pub engram: EngramId,
    /// Whether losing this one is unrecoverable rather than inconvenient.
    pub critical: bool,
}

/// One engram was superseded by the next.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rotated {
    /// The lineage.
    pub slug: Slug,
    /// The engram that takes over. What it supersedes is whatever the lineage
    /// held, which replay knows and the record need not repeat.
    pub to: EngramId,
}

/// An engram was given a verifier on the machine that wrote this.
///
/// Continuity is **asserted, never proved**: `rote` holds no recoverable form of
/// a secret and has nothing to check a typed one against. The claim is recorded
/// and nothing is verified.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Attached {
    /// The engram a verifier was minted for. It names its lineage.
    pub engram: EngramId,
}

/// A lineage left the schedule.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Retired {
    /// The lineage.
    pub slug: Slug,
}

/// How one capture ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Outcome {
    /// The verifier accepted it.
    Pass,
    /// The verifier refused it.
    Fail,
    /// Nothing was offered. A failure of recall like any other, kept apart from
    /// [`Outcome::Fail`] because producing a wrong answer and having none are
    /// different readings, and the first week runs through both.
    Blank,
    /// Passed over deliberately.
    Skip,
    /// The sitting was abandoned at this prompt.
    Abort,
}

impl Outcome {
    /// Whether recall was reached for and missed. A blank is a failure, not an
    /// absence.
    pub fn lapsed(self) -> bool {
        matches!(self, Self::Fail | Self::Blank)
    }

    /// Whether anything was put in front of a person.
    pub fn exposed(self) -> bool {
        matches!(self, Self::Pass | Self::Fail | Self::Blank)
    }
}

/// One typed sample.
///
/// Nothing here is derived from the input's content: not its length, not a
/// prefix, not a character class. `corrections` counts keystrokes that removed
/// something, one apiece and whatever each removed, and the two paste counts
/// partition the pastes the prompt saw — facts about the sitting rather than
/// about the secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Captured {
    /// The lineage. Determined by `engram`, and carried anyway so one line
    /// grepped out of context names something a person recognises.
    pub slug: Slug,
    /// The engram that was asked for.
    pub engram: EngramId,
    /// The sitting this belongs to.
    pub sitting: SittingId,
    /// Which sample within this drill. Only the first is measured.
    pub ordinal: u8,
    /// Whether the schedule asked for the drill this belongs to.
    pub occasion: Occasion,
    /// Whether the answer was consulted before it was typed. Not recall, so it
    /// is kept out of every figure that claims to measure one.
    pub aided: bool,
    /// How it ended.
    pub outcome: Outcome,
    /// Milliseconds from the prompt appearing to the first keystroke. The
    /// leading indicator of decay: it rises before the first failure.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ttfk_ms: Option<u64>,
    /// Milliseconds from the first keystroke to submission.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_ms: Option<u64>,
    /// Keystrokes that removed something, one apiece.
    pub corrections: u32,
    /// Pastes that reached the field. One is taken wherever nothing is being
    /// measured, which is every prompt but a cold try.
    pub paste_accepted: u32,
    /// Pastes that did not, whether the prompt refuses them or the field had no
    /// room. The two counts together are every paste the prompt saw, and
    /// neither is a count of lookups: a paste refused here is one steered onto
    /// a channel that arrives as ordinary typing.
    pub paste_refused: u32,
    /// What the ladder asked for. Authoritative.
    pub scheduled_interval_days: u32,
    /// Days since the schedule's anchor, as the writer saw it. A witness.
    pub actual_interval_days: u32,
    /// Days since the secret was last in front of a person, as the writer saw
    /// it. A witness.
    pub effective_interval_days: u32,
    /// The ladder position before this sample, as the writer saw it. A witness:
    /// after a merge it can name a predecessor that never existed in merged
    /// order, which is exactly how a divergence is found.
    pub rung_before: u8,
    /// The ladder position after it. Authoritative.
    pub rung_after: u8,
}

impl Captured {
    /// Whether this sample moves the ladder.
    ///
    /// The schedule must have asked, the memory must have been measured, and it
    /// must be the first sample of its drill — a second is primed by the first.
    pub fn scores(&self) -> bool {
        self.occasion == Occasion::Review && !self.aided && self.ordinal == 1
    }
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
/// When the record will not serialize, which means a type in it has a broken
/// `Serialize`.
pub fn render(record: &Record) -> Result<String, serde_json::Error> {
    serde_json::to_string(record)
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::{
        Captured, Digest, EngramId, Enrolled, Event, Line, Outcome, Record, SCHEMA, SittingId,
        render,
    };
    use crate::ladder::Occasion;

    fn capture() -> Record {
        Record {
            v: SCHEMA,
            at: "2026-09-10T07:12:03Z".parse().unwrap(),
            day: date(2026, 9, 10),
            host: "Mac".to_owned(),
            prev: Digest::GENESIS,
            event: Event::Capture(Captured {
                slug: "escrow-p".parse().unwrap(),
                engram: EngramId::mint().unwrap(),
                sitting: SittingId::mint().unwrap(),
                ordinal: 1,
                occasion: Occasion::Review,
                aided: false,
                outcome: Outcome::Pass,
                ttfk_ms: Some(1_200),
                total_ms: Some(4_100),
                corrections: 0,
                paste_accepted: 0,
                paste_refused: 0,
                scheduled_interval_days: 30,
                actual_interval_days: 30,
                effective_interval_days: 30,
                rung_before: 6,
                rung_after: 6,
            }),
        }
    }

    #[test]
    fn a_record_round_trips_through_one_line() {
        let record = capture();
        let line = render(&record).unwrap();
        assert!(!line.contains('\n'));
        assert_eq!(serde_json::from_str::<Record>(&line).unwrap(), record);
    }

    #[test]
    fn the_wire_form_is_flat_enough_to_grep() {
        let line = render(&capture()).unwrap();
        for expected in [
            "\"kind\":\"capture\"",
            "\"slug\":\"escrow-p\"",
            "\"occasion\":\"review\"",
            "\"aided\":false",
            "\"outcome\":\"pass\"",
        ] {
            assert!(line.contains(expected), "missing {expected} in {line}");
        }
    }

    #[test]
    fn every_event_but_a_retirement_names_an_engram() {
        let engram = EngramId::mint().unwrap();
        let event = Event::Enroll(Enrolled {
            slug: "op-master".parse().unwrap(),
            engram,
            critical: false,
        });
        assert_eq!(event.engram(), Some(engram));
        assert_eq!(event.kind(), "enroll");
        let retired = Event::Retire(super::Retired {
            slug: "op-master".parse().unwrap(),
        });
        assert_eq!(retired.engram(), None);
        assert_eq!(retired.kind(), "retire");
    }

    #[test]
    fn an_absent_optional_is_omitted_rather_than_written_as_null() {
        let mut record = capture();
        if let Event::Capture(ref mut entry) = record.event {
            entry.ttfk_ms = None;
            entry.total_ms = None;
        }
        let line = render(&record).unwrap();
        assert!(!line.contains("null"));
        assert_eq!(serde_json::from_str::<Record>(&line).unwrap(), record);
    }

    #[test]
    fn an_unknown_key_is_refused_at_either_level() {
        let mut value: serde_json::Value =
            serde_json::from_str(&render(&capture()).unwrap()).unwrap();
        value["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_str::<Record>(&value.to_string()).is_err());

        let mut value: serde_json::Value =
            serde_json::from_str(&render(&capture()).unwrap()).unwrap();
        value["event"]["surprise"] = serde_json::json!(true);
        assert!(serde_json::from_str::<Record>(&value.to_string()).is_err());
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
    fn the_chain_digest_round_trips_and_refuses_a_bad_one() {
        let digest = Digest::of("a line");
        let text = digest.to_string();
        assert_eq!(text.len(), 64);
        assert_eq!(text.parse::<Digest>().unwrap(), digest);
        assert!("zz".parse::<Digest>().is_err());
        assert_eq!(Digest::GENESIS.to_string(), "0".repeat(64));
        assert_ne!(Digest::of("a"), Digest::of("b"));
    }

    #[test]
    fn a_minted_identity_round_trips_and_refuses_a_bad_one() {
        let engram = EngramId::mint().unwrap();
        assert_eq!(engram.to_string().len(), 32);
        let json = serde_json::to_string(&engram).unwrap();
        assert_eq!(serde_json::from_str::<EngramId>(&json).unwrap(), engram);
        assert!(serde_json::from_str::<EngramId>("\"short\"").is_err());
        assert_ne!(
            EngramId::mint().unwrap(),
            EngramId::mint().unwrap(),
            "two mints never collide"
        );
        assert!(serde_json::from_str::<SittingId>("\"nope\"").is_err());
    }

    #[test]
    fn only_a_first_unaided_review_sample_moves_the_ladder() {
        let mut record = capture();
        let Event::Capture(ref mut sample) = record.event else {
            panic!("built a capture");
        };
        assert!(sample.scores());

        sample.aided = true;
        assert!(!sample.scores(), "an aided sample measures nothing");

        sample.aided = false;
        sample.ordinal = 2;
        assert!(!sample.scores(), "a second sample is primed by the first");

        sample.ordinal = 1;
        sample.occasion = Occasion::Practice;
        assert!(!sample.scores(), "nobody asked for practice");
    }

    #[test]
    fn only_something_typed_counts_as_exposure() {
        assert!(Outcome::Pass.exposed());
        assert!(Outcome::Fail.exposed());
        assert!(Outcome::Blank.exposed());
        assert!(!Outcome::Skip.exposed());
        assert!(!Outcome::Abort.exposed());
        assert!(Outcome::Fail.lapsed());
        assert!(Outcome::Blank.lapsed());
        assert!(!Outcome::Pass.lapsed());
    }
}
