//! When a mode is said, as a closed vocabulary.
//!
//! Three forms and a fixed key set, deliberately with no expression language.
//! The moment this needs a parser, nobody remembers the syntax and the runtime
//! is a cron daemon that happens to hold prose.
//!
//! ```yaml
//! triggers:
//!   - on: activate                       # immediate, on +token
//!   - every: { tokens: 25000 }           # periodic, mark-gated
//!   - when: { context_tokens: 500000 }   # deferred, edge-triggered, once
//! ```
//!
//! Composition is list membership and nothing more, so combinations fall out for
//! free: `terse` is `activate` plus `every`, `paced` is `when` alone, `robust` is
//! `activate` alone.
//!
//! **Compaction is not in here.** Re-stating every active mode after the context
//! was summarised away is a property of the runtime, not a choice a mode gets to
//! make — a mode that opted out would simply stop existing at the first
//! `/compact`. Keeping it out of the vocabulary is also what keeps the vocabulary
//! honest: every entry below is a decision a mode author actually has.

use serde::Deserialize;
use serde::de::{Deserializer, Error as _};

/// One clause of a mode's schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Trigger {
    /// A moment to say it at.
    On(Moment),
    /// Say it again every so many tokens.
    Every(Every),
    /// Say it once, when the session has grown past a mark.
    When(When),
}

/// The moments a mode can ask for. One today; the enum is the vocabulary.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Moment {
    /// The `+token` that switched the mode on.
    Activate,
}

/// Periodic re-delivery, measured in context tokens rather than turns: salience
/// decays with what fills the window, and a turn can be a sentence or a hundred
/// thousand tokens of tool output.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Every {
    /// Tokens between one delivery and the next.
    pub tokens: u64,
}

/// Deferred delivery. The directive is wrong until the session is big enough for
/// it to be true, and a directive that is wrong is not merely inert — it is a
/// distractor sitting in context for every turn before it applies.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct When {
    /// The mark to cross.
    pub context_tokens: u64,
}

/// A clause as a mode file spells it: a one-key mapping.
///
/// Written out rather than derived as an externally tagged enum, because YAML
/// spells one of those with a `!Tag` and the corpus is written as mappings. The
/// pay-off is the error: `deny_unknown_fields` answers a misspelt clause by
/// naming the three that exist, which is the whole reason the vocabulary is
/// closed.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    #[serde(default)]
    on: Option<Moment>,
    #[serde(default)]
    every: Option<Every>,
    #[serde(default)]
    when: Option<When>,
}

impl<'de> Deserialize<'de> for Trigger {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = Wire::deserialize(deserializer)?;
        match (wire.on, wire.every, wire.when) {
            (Some(moment), None, None) => Ok(Self::On(moment)),
            (None, Some(every), None) => Ok(Self::Every(every)),
            (None, None, Some(when)) => Ok(Self::When(when)),
            (None, None, None) => Err(D::Error::custom("a trigger needs one of: on, every, when")),
            _ => Err(D::Error::custom(
                "a trigger takes exactly one of: on, every, when",
            )),
        }
    }
}

/// What a mode does when it declares no triggers at all: the behaviour the whole
/// corpus had before there was a schedule, so eleven of thirteen files migrate
/// without gaining a line.
pub fn default() -> Vec<Trigger> {
    vec![Trigger::On(Moment::Activate)]
}

impl Trigger {
    /// How this clause reads in a listing. Short enough for a table cell.
    pub fn label(self) -> String {
        match self {
            Self::On(Moment::Activate) => "on activate".to_owned(),
            Self::Every(Every { tokens }) => format!("every {tokens} tokens"),
            Self::When(When { context_tokens }) => format!("at {context_tokens} tokens"),
        }
    }

    /// Whether evaluating this clause needs to know how full the window is.
    /// A session whose modes all answer `false` never reads a transcript.
    pub fn reads_tokens(self) -> bool {
        matches!(self, Self::Every(_) | Self::When(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(yaml: &str) -> Result<Vec<Trigger>, String> {
        relic_core::frontmatter::parse::<Vec<Trigger>>(yaml).map_err(|e| e.to_string())
    }

    #[test]
    fn the_three_forms_parse() {
        let found = parse(
            "- on: activate\n- every: { tokens: 25000 }\n- when: { context_tokens: 500000 }\n",
        )
        .unwrap();
        assert_eq!(
            found,
            [
                Trigger::On(Moment::Activate),
                Trigger::Every(Every { tokens: 25000 }),
                Trigger::When(When {
                    context_tokens: 500_000
                }),
            ]
        );
    }

    #[test]
    fn an_unknown_clause_names_the_ones_that_exist() {
        let error = parse("- whenever: { tokens: 1 }\n").unwrap_err();
        assert!(error.contains("unknown field"), "{error}");
        assert!(error.contains("every"), "{error}");
    }

    #[test]
    fn a_clause_takes_exactly_one_key() {
        assert!(parse("- {}\n").unwrap_err().contains("needs one of"));
        assert!(
            parse("- { on: activate, every: { tokens: 1 } }\n")
                .unwrap_err()
                .contains("exactly one")
        );
    }

    #[test]
    fn an_unknown_key_inside_a_clause_is_refused() {
        assert!(parse("- every: { turns: 5 }\n").is_err());
        assert!(parse("- when: { tokens: 5 }\n").is_err());
    }

    #[test]
    fn only_token_gated_clauses_need_a_transcript() {
        assert!(!Trigger::On(Moment::Activate).reads_tokens());
        assert!(Trigger::Every(Every { tokens: 1 }).reads_tokens());
        assert!(Trigger::When(When { context_tokens: 1 }).reads_tokens());
    }

    #[test]
    fn the_default_is_what_an_expander_did() {
        assert_eq!(default(), [Trigger::On(Moment::Activate)]);
    }
}
