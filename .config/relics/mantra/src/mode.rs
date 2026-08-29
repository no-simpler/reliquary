//! A mode: a reusable paragraph of behavioural directives, and the schedule it
//! is said on.
//!
//! Markdown with optional metadata, because prose is the payload. Prose inside a
//! YAML string block is miserable to author and worse to diff, so the structure
//! goes in the metadata and the directives stay a document.
//!
//! ```text
//! ---
//! triggers:
//!   - on: activate
//!   - every: { tokens: 25000 }
//! refrain: Say **few** words. Cut filler.
//! ---
//!
//! # Terse mode
//! …
//! ```
//!
//! **Two payloads, split by the moment they are delivered at.** Activation emits
//! the body; a refresh emits the [`Mode::refrain`]. Never both, so nothing is
//! restated inside one injection.
//!
//! The refrain is not a summary and not a size optimisation — bodies are tiny by
//! policy and re-pasting one costs nothing. It exists because a refresh needs
//! *different emphasis*: the imperative without its qualifiers. `terse`'s
//! `## Suspend` section is the worked example. Re-delivering "drop terseness for
//! security warnings" every twenty-five thousand tokens is periodically
//! re-issuing permission to be verbose, which undoes the refresh it rode in on.
//! Exceptions are activation-only material; the standing directive is not.
//!
//! Single-line is a constraint rather than a limitation. A refrain that needs a
//! paragraph is a mode whose imperative is not yet sharp enough to survive
//! repetition.
//!
//! Parsing is **total**. A file that cannot be read still yields a [`Broken`],
//! because a mode that silently disappears is a mode whose absence is discovered
//! by the model behaving wrongly. The hook skips it; `mantra doctor` names it.

use camino::{Utf8Path, Utf8PathBuf};
use fs_err as fs;
use serde::Deserialize;

use crate::trigger::{self, Trigger};

/// The metadata a mode file may carry. Everything is optional; a file with no
/// metadata at all is a mode with the default schedule and no refrain.
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Wire {
    #[serde(default)]
    triggers: Option<Vec<Trigger>>,
    #[serde(default)]
    refrain: Option<String>,
}

/// A mode that reads.
#[derive(Clone, Debug)]
pub struct Mode {
    /// The `+token` that activates it, which is also its file stem.
    pub name: String,
    /// Where it was found.
    pub path: Utf8PathBuf,
    /// When it is said.
    pub triggers: Vec<Trigger>,
    /// What a refresh says. Absent falls back to the body, which is right for a
    /// mode carrying no exceptions to leave out.
    pub refrain: Option<String>,
    /// What an activation says.
    pub body: String,
}

/// A mode file that does not read, kept rather than dropped.
#[derive(Clone, Debug)]
pub struct Broken {
    /// The name it would have had.
    pub name: String,
    /// The file.
    pub path: Utf8PathBuf,
    /// What is wrong with it, in one line.
    pub why: String,
}

/// Either, so a listing can show both without two passes.
pub type Read = Result<Mode, Broken>;

impl Mode {
    /// What an activation, or a re-statement after compaction, delivers.
    pub fn full(&self) -> &str {
        &self.body
    }

    /// What a refresh delivers.
    pub fn short(&self) -> &str {
        self.refrain.as_deref().unwrap_or(&self.body)
    }

    /// Whether any clause of this mode's schedule needs the window size.
    pub fn reads_tokens(&self) -> bool {
        self.triggers.iter().any(|t| Trigger::reads_tokens(*t))
    }
}

/// Reads one mode file. Never fails — an unreadable mode is a [`Broken`].
pub fn read(name: &str, path: &Utf8Path) -> Read {
    let broken = |why: String| Broken {
        name: name.to_owned(),
        path: path.to_owned(),
        why,
    };
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) => return Err(broken(e.to_string())),
    };
    parse(name, path, &text).map_err(broken)
}

/// The parse, separated from the file so tests do not need one.
fn parse(name: &str, path: &Utf8Path, text: &str) -> Result<Mode, String> {
    use relic_core::frontmatter::{Error, split};

    // No metadata is a mode that wanted none: the default schedule, and the
    // whole file as its body. Metadata that opens and never closes is a typo,
    // and reporting it is the only way its author finds out.
    let (front, body) = match split(text) {
        Ok(halves) => halves,
        Err(Error::Missing) => ("", text),
        Err(e) => return Err(e.to_string()),
    };

    let wire: Wire = if front.trim().is_empty() {
        Wire::default()
    } else {
        relic_core::frontmatter::parse(front).map_err(|e| e.to_string())?
    };

    if body.trim().is_empty() {
        return Err("the mode says nothing: its body is empty".to_owned());
    }
    if let Some(refrain) = &wire.refrain {
        if refrain.trim().is_empty() {
            return Err("refrain is empty: remove the key or write the imperative".to_owned());
        }
        if refrain.contains('\n') {
            return Err(
                "refrain is more than one line: sharpen the imperative until it fits".to_owned(),
            );
        }
    }

    Ok(Mode {
        name: name.to_owned(),
        path: path.to_owned(),
        triggers: wire.triggers.unwrap_or_else(trigger::default),
        refrain: wire.refrain.map(|r| r.trim().to_owned()),
        body: body.trim().to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::{Every, Moment};

    fn read_text(text: &str) -> Result<Mode, String> {
        parse("terse", Utf8Path::new("/modes/terse.md"), text)
    }

    #[test]
    fn a_file_without_metadata_takes_the_default_schedule() {
        let mode = read_text("# Terse mode\n\nSay few words.\n").unwrap();
        assert_eq!(mode.triggers, trigger::default());
        assert_eq!(mode.refrain, None);
        assert_eq!(mode.body, "# Terse mode\n\nSay few words.");
    }

    #[test]
    fn metadata_carries_the_schedule_and_the_refrain() {
        let mode = read_text(
            "---\ntriggers:\n  - on: activate\n  - every: { tokens: 25000 }\nrefrain: Say few words.\n---\n\n# Terse mode\n\nBody.\n",
        )
        .unwrap();
        assert_eq!(
            mode.triggers,
            [
                Trigger::On(Moment::Activate),
                Trigger::Every(Every { tokens: 25000 })
            ]
        );
        assert_eq!(mode.refrain.as_deref(), Some("Say few words."));
        assert_eq!(mode.body, "# Terse mode\n\nBody.");
    }

    #[test]
    fn a_refresh_without_a_refrain_falls_back_to_the_body() {
        let mode = read_text("# Robust mode\n\nGeneralise.\n").unwrap();
        assert_eq!(mode.short(), mode.full());
    }

    #[test]
    fn a_refrain_replaces_the_body_on_a_refresh() {
        let mode = read_text("---\nrefrain: Cut filler.\n---\n\nLong body.\n").unwrap();
        assert_eq!(mode.short(), "Cut filler.");
        assert_eq!(mode.full(), "Long body.");
    }

    #[test]
    fn a_multi_line_refrain_is_refused() {
        let error = read_text("---\nrefrain: |\n  one\n  two\n---\n\nBody.\n").unwrap_err();
        assert!(error.contains("one line"), "{error}");
    }

    #[test]
    fn an_empty_body_is_refused() {
        assert!(read_text("---\nrefrain: x\n---\n\n\n").is_err());
    }

    #[test]
    fn unterminated_metadata_is_reported_rather_than_read_as_body() {
        let error = read_text("---\nrefrain: x\n\n# Terse\n").unwrap_err();
        assert!(error.contains("unterminated"), "{error}");
    }

    #[test]
    fn a_stale_key_from_the_command_lane_is_refused() {
        // `description` and `disable-model-invocation` meant something only
        // while modes lived under `commands/`. Refusing them is what makes a
        // half-migrated file loud instead of silently unscheduled.
        let error = read_text("---\ndescription: Terse mode\n---\n\nBody.\n").unwrap_err();
        assert!(error.contains("description"), "{error}");
    }

    #[test]
    fn only_a_token_gated_mode_reads_a_transcript() {
        assert!(!read_text("Body.\n").unwrap().reads_tokens());
        assert!(
            read_text("---\ntriggers:\n  - every: { tokens: 10 }\n---\n\nBody.\n")
                .unwrap()
                .reads_tokens()
        );
    }
}
