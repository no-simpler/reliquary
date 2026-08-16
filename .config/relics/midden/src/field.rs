//! Every text a person or an agent hands the CLI passes through here, so one
//! contract covers `note` and `set` alike.
//!
//! The limits are the corpus's first line of size policing, and they bind at
//! write time rather than at `gc`: a heap only stays worth excavating while
//! each note is still one glance. Reading stays lenient — a file already on
//! disk is never rejected for length, only reported by `doctor`.

use anyhow::{Result, bail};

/// A name, not a sentence. Git's hard subject bound, and where a listing stops
/// being scannable.
pub const TITLE_MAX: usize = 72;

/// The cause in a sentence or two. Longer than a tagline because a title names
/// the friction and this has to say why it happened, but short enough that a
/// digest of forty notes still reads in one sitting.
pub const DETAIL_MAX: usize = 200;

/// A path, a mode name, or a section heading. Bounded so it stays a locator and
/// does not drift into being the proposal itself.
pub const TARGET_MAX: usize = 120;

/// The evidence. Bytes, not characters, because this is the only unbounded
/// field and what it must not become is a transcript excerpt — roughly a screen
/// of terminal, which is as much as a quotation needs to be convincing.
pub const BODY_MAX: usize = 1200;

/// Trimmed, with every internal run of whitespace collapsed to one space, so a
/// value pasted out of a wrapped paragraph stores as the single line it is.
fn normalise(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Characters, not display width: the failure it would guard against — a CJK
/// detail occupying twice its count in columns — is not one this tool meets,
/// and counting scalar values keeps the rule one a reader can apply by eye.
fn width(value: &str) -> usize {
    value.chars().count()
}

/// The normalised value, or the reason it will not do. `label` is the flag a
/// caller would retype, so the message points at the fix.
pub fn one_line(label: &str, raw: &str, max: usize) -> Result<String> {
    let value = normalise(raw);
    if value.is_empty() {
        bail!("{label} is required");
    }
    let length = width(&value);
    if length > max {
        bail!(
            "{label} is {length} characters; the limit is {max}. \
             A note is read in a digest of many — the detail that will not fit \
             belongs in the body, as evidence"
        );
    }
    Ok(value)
}

/// The same, for a value that may legitimately be absent. Blank means absent.
pub fn optional(label: &str, raw: &str, max: usize) -> Result<Option<String>> {
    if normalise(raw).is_empty() {
        return Ok(None);
    }
    one_line(label, raw, max).map(Some)
}

/// The body, with its trailing whitespace settled and its size bound enforced.
pub fn body(raw: &str) -> Result<String> {
    let trimmed = raw.trim_end();
    if trimmed.len() > BODY_MAX {
        bail!(
            "--body is {} bytes; the limit is {BODY_MAX}. \
             The body is the evidence, not the transcript: quote the turn that \
             shows the friction and cut the rest",
            trimmed.len()
        );
    }
    if trimmed.is_empty() {
        return Ok(String::new());
    }
    Ok(format!("{trimmed}\n"))
}

/// Whether a value already on disk is longer than what would be accepted now.
pub fn is_overlong(value: &str, max: usize) -> bool {
    width(value) > max
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wrapped_paragraph_stores_as_one_line() {
        assert_eq!(
            one_line("--detail", "  two   lines\nof it  ", DETAIL_MAX).unwrap(),
            "two lines of it"
        );
    }

    #[test]
    fn emptiness_and_overlength_are_both_refused() {
        assert!(one_line("--title", " \n ", TITLE_MAX).is_err());
        let long = "x".repeat(TITLE_MAX + 1);
        let error = one_line("--title", &long, TITLE_MAX)
            .unwrap_err()
            .to_string();
        assert!(error.contains("--title"), "{error}");
        assert!(error.contains("73 characters"), "{error}");
    }

    #[test]
    fn an_absent_optional_is_not_an_error() {
        assert_eq!(optional("--target", "  ", TARGET_MAX).unwrap(), None);
        assert_eq!(
            optional("--target", " ~/.config/CLAUDE.md ", TARGET_MAX).unwrap(),
            Some("~/.config/CLAUDE.md".to_owned())
        );
    }

    #[test]
    fn counting_is_characters_not_bytes() {
        let value = "é".repeat(TITLE_MAX);
        assert!(one_line("--title", &value, TITLE_MAX).is_ok());
    }

    #[test]
    fn a_body_is_bounded_and_newline_terminated() {
        assert_eq!(body("  ").unwrap(), "");
        assert_eq!(body("evidence\n\n\n").unwrap(), "evidence\n");
        assert!(body(&"x".repeat(BODY_MAX + 1)).is_err());
    }
}
