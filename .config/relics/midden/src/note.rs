use std::fmt;
use std::path::PathBuf;

use anyhow::{Result, bail};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::field;
use crate::id::{Id, digest};

/// Whole seconds. Nothing here turns on sub-second ordering, and a metadata
/// block that a person may read should not carry noise.
pub fn now() -> Timestamp {
    Timestamp::now()
        .round(jiff::Unit::Second)
        .unwrap_or_else(|_| Timestamp::now())
}

/// How many recurrences keep their own timestamp. The count is the signal;
/// the dates only have to show whether it is still happening, and an unbounded
/// list would be the one field that grows without end.
pub const SEEN_MAX: usize = 10;

pub const FINGERPRINT_WIDTH: usize = 8;

/// The closed taxonomy. A kind is chosen before a title is written, which is
/// what stops a note from being a feeling — each one implies where its fix
/// lives, and a cause that fits none of them is usually not yet understood.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
#[clap(rename_all = "lowercase")]
pub enum Kind {
    Gap,
    Conflict,
    Stale,
    Hunt,
    Rebuff,
    Friction,
    Rework,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Gap => "gap",
            Kind::Conflict => "conflict",
            Kind::Stale => "stale",
            Kind::Hunt => "hunt",
            Kind::Rebuff => "rebuff",
            Kind::Friction => "friction",
            Kind::Rework => "rework",
        }
    }
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Where a note is in its life. Open is the only state that asks anything of
/// the reader; the other two are how a note earns its way out of the corpus.
#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
#[clap(rename_all = "lowercase")]
pub enum Status {
    Open,
    Actioned,
    Dismissed,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Open => "open",
            Status::Actioned => "actioned",
            Status::Dismissed => "dismissed",
        }
    }
}

impl fmt::Display for Status {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One observation, as it sits on disk. Field order here is the key order in
/// the file: what the note says, then how often, then where it came from, then
/// when.
///
/// There is no separate wire projection — unlike a docket item, a note has no
/// shape that could be invalid-but-representable, so validation is a pass over
/// the same struct rather than a type that excludes it.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Note {
    pub id: Id,
    pub kind: Kind,
    pub title: String,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,

    pub status: Status,
    pub occurrences: u32,

    pub project: PathBuf,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cwd: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session: Option<String>,

    pub created: Timestamp,
    pub updated: Timestamp,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub seen: Vec<Timestamp>,
    pub fingerprint: String,
}

impl Note {
    /// What the metadata has to satisfy to be acted on. Reported by `doctor`
    /// and repaired by `set`, never enforced at read time — a note that fails
    /// this still lists, so it cannot disappear by being wrong.
    pub fn validate(&self) -> Result<()> {
        if self.title.trim().is_empty() {
            bail!("title is empty");
        }
        if self.occurrences == 0 {
            bail!("occurrences is 0; a note exists because it happened at least once");
        }
        if self.fingerprint.len() != FINGERPRINT_WIDTH {
            bail!(
                "fingerprint {:?} is not {FINGERPRINT_WIDTH} characters",
                self.fingerprint
            );
        }
        if self.updated < self.created {
            bail!("updated precedes created");
        }
        Ok(())
    }

    /// Recomputed from what the note claims, so a `set` that changes any part
    /// of the claim re-files it against the right neighbours.
    pub fn refingerprint(&mut self) {
        self.fingerprint = fingerprint(self.kind, self.target.as_deref(), &self.title);
    }

    /// Records another sighting. An actioned note that recurs reopens: the fix
    /// that was filed against it did not hold, and that is the single most
    /// useful thing the corpus can tell anyone.
    pub fn saw(&mut self, at: Timestamp) {
        self.occurrences = self.occurrences.saturating_add(1);
        self.seen.push(at);
        if self.seen.len() > SEEN_MAX {
            let excess = self.seen.len() - SEEN_MAX;
            self.seen.drain(..excess);
        }
        self.updated = at;
        if self.status == Status::Actioned {
            self.status = Status::Open;
        }
    }

    pub fn overlong(&self) -> Option<(&'static str, usize, usize)> {
        for (label, value, max) in [
            ("title", Some(self.title.as_str()), field::TITLE_MAX),
            ("detail", self.detail.as_deref(), field::DETAIL_MAX),
            ("target", self.target.as_deref(), field::TARGET_MAX),
        ] {
            let Some(value) = value else { continue };
            if field::is_overlong(value, max) {
                return Some((label, value.chars().count(), max));
            }
        }
        None
    }
}

/// The dedup key: same kind, same place to fix it, same claim. Deliberately
/// blind to project, session and wording — one ambiguous directive met from
/// three repositories is one finding, not three.
pub fn fingerprint(kind: Kind, target: Option<&str>, title: &str) -> String {
    digest(
        &[
            kind.as_str(),
            &normalise_target(target.unwrap_or_default()),
            &normalise_claim(title),
        ],
        FINGERPRINT_WIDTH,
    )
}

/// Case, punctuation and filler dropped, so two sessions phrasing the same
/// cause differently still land on one note.
fn normalise_claim(value: &str) -> String {
    const FILLER: [&str; 8] = ["the", "a", "an", "is", "was", "to", "of", "in"];
    value
        .split_whitespace()
        .map(|word| {
            word.chars()
                .filter(|c| c.is_alphanumeric())
                .flat_map(char::to_lowercase)
                .collect::<String>()
        })
        .filter(|word| !word.is_empty() && !FILLER.contains(&word.as_str()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// A locator as it is stored: home collapsed to a tilde so the same file reads
/// the same on every machine, and no trailing separator.
pub fn tidy_target(value: &str) -> String {
    let trimmed = value.trim().trim_end_matches('/');
    match std::env::var("HOME") {
        Ok(home) if !home.is_empty() => match trimmed.strip_prefix(&home) {
            Some(rest) => format!("~{rest}"),
            None => trimmed.to_owned(),
        },
        _ => trimmed.to_owned(),
    }
}

/// The same, case folded, for the fingerprint alone.
pub fn normalise_target(value: &str) -> String {
    tidy_target(value).to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(kind: Kind, target: Option<&str>, title: &str) -> Note {
        let at = now();
        let mut note = Note {
            id: Id::mint(),
            kind,
            title: title.to_owned(),
            detail: None,
            target: target.map(str::to_owned),
            status: Status::Open,
            occurrences: 1,
            project: PathBuf::from("/tmp/project"),
            cwd: None,
            branch: None,
            session: None,
            created: at,
            updated: at,
            seen: vec![at],
            fingerprint: String::new(),
        };
        note.refingerprint();
        note
    }

    #[test]
    fn the_same_cause_phrased_differently_fingerprints_alike() {
        let one = fingerprint(
            Kind::Gap,
            Some("~/.config/CLAUDE.md"),
            "The yadm wrapper is unstated",
        );
        let two = fingerprint(
            Kind::Gap,
            Some("~/.config/CLAUDE.md/"),
            "yadm wrapper unstated.",
        );
        assert_eq!(one, two);
    }

    #[test]
    fn kind_and_target_both_separate_causes() {
        let base = fingerprint(Kind::Gap, Some("a.md"), "same claim");
        assert_ne!(base, fingerprint(Kind::Stale, Some("a.md"), "same claim"));
        assert_ne!(base, fingerprint(Kind::Gap, Some("b.md"), "same claim"));
        assert_ne!(base, fingerprint(Kind::Gap, None, "same claim"));
    }

    #[test]
    fn a_sighting_bumps_the_count_and_bounds_the_dates() {
        let mut note = note(Kind::Hunt, None, "where the brewfiles live");
        for _ in 0..(SEEN_MAX + 5) {
            note.saw(now());
        }
        assert_eq!(note.occurrences as usize, SEEN_MAX + 6);
        assert_eq!(note.seen.len(), SEEN_MAX);
    }

    #[test]
    fn recurrence_reopens_an_actioned_note() {
        let mut note = note(Kind::Gap, None, "unstated convention");
        note.status = Status::Actioned;
        note.saw(now());
        assert_eq!(note.status, Status::Open);
    }

    #[test]
    fn recurrence_leaves_a_dismissal_standing() {
        let mut note = note(Kind::Friction, None, "a prompt that is simply the cost");
        note.status = Status::Dismissed;
        note.saw(now());
        assert_eq!(note.status, Status::Dismissed);
    }

    #[test]
    fn validation_names_what_is_wrong() {
        let mut note = note(Kind::Gap, None, "fine");
        assert!(note.validate().is_ok());
        note.occurrences = 0;
        assert!(
            note.validate()
                .unwrap_err()
                .to_string()
                .contains("occurrences")
        );
    }
}
