//! Every text field a person or an agent hands the CLI passes through here, so
//! one contract covers `create`, `set` and `relay` alike.
//!
//! Reading stays lenient on purpose: a file already on disk is never rejected
//! for length, only reported by `doctor`. The limits bind what is written.

use anyhow::{Result, bail};

/// A name, not a sentence. Git's hard subject bound, and where a listing stops
/// being scannable.
pub const TITLE_MAX: usize = 72;

/// One terminal row — the same bound Debian puts on a package synopsis and
/// Homebrew on a formula description. Anything longer is body material.
pub const TAGLINE_MAX: usize = 80;

/// A block reason shares the tagline's column, so it wraps the same way.
pub const BLOCKED_MAX: usize = 80;

pub const TAG_MAX: usize = 32;

/// Trimmed, with every internal run of whitespace collapsed to one space, so a
/// value pasted out of a wrapped paragraph stores as the single line it is.
fn normalise(raw: &str) -> String {
    raw.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Characters, not display width: the failure it would guard against — a CJK
/// tagline occupying twice its count in columns — is not one this tool meets,
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
             It is the one line that says whether this is the item a session came \
             for — the detail belongs in the body"
        );
    }
    Ok(value)
}

/// Normalised and cut to fit, for the repair path: `set` rebuilds an item whose
/// frontmatter stopped parsing, and must not fail on a value it did not author.
/// The cut falls on a word boundary where there is one within reach.
pub fn clamp(raw: &str, max: usize) -> String {
    let value = normalise(raw);
    if width(&value) <= max {
        return value;
    }
    let keep = max.saturating_sub(1);
    let end = value
        .char_indices()
        .nth(keep)
        .map(|(at, _)| at)
        .unwrap_or(value.len());
    let head = &value[..end];
    let cut = match head.rfind(' ') {
        Some(space) if width(&head[..space]) * 2 >= keep => space,
        _ => end,
    };
    format!("{}…", head[..cut].trim_end())
}

/// One tag, or nothing when the entry was blank. Whitespace is rejected rather
/// than collapsed: a tag is a token, and a listing separates them by space.
pub fn tag(raw: &str) -> Result<Option<String>> {
    let value = raw.trim();
    if value.is_empty() {
        return Ok(None);
    }
    if value.split_whitespace().count() > 1 {
        bail!("tag {value:?} contains whitespace; tags are single tokens");
    }
    let length = width(value);
    if length > TAG_MAX {
        bail!("tag {value:?} is {length} characters; the limit is {TAG_MAX}");
    }
    Ok(Some(value.to_owned()))
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
            one_line("--tagline", "  two   lines\nof it  ", TAGLINE_MAX).unwrap(),
            "two lines of it"
        );
    }

    #[test]
    fn emptiness_and_overlength_are_both_refused() {
        assert!(one_line("--title", " \n ", TITLE_MAX).is_err());
        let long = "x".repeat(TAGLINE_MAX + 1);
        let error = one_line("--tagline", &long, TAGLINE_MAX)
            .unwrap_err()
            .to_string();
        assert!(error.contains("--tagline"), "{error}");
        assert!(error.contains("81 characters"), "{error}");
    }

    #[test]
    fn counting_is_characters_not_bytes() {
        let value = "é".repeat(TAGLINE_MAX);
        assert!(one_line("--tagline", &value, TAGLINE_MAX).is_ok());
    }

    #[test]
    fn clamping_fits_and_prefers_a_word_boundary() {
        assert_eq!(clamp("short enough", TAGLINE_MAX), "short enough");
        let clamped = clamp("alpha beta gamma delta", 12);
        assert_eq!(clamped, "alpha beta…");
        assert!(width(&clamped) <= 12);
    }

    #[test]
    fn clamping_cuts_mid_word_when_there_is_no_boundary_in_reach() {
        let clamped = clamp("a supercalifragilistic word", 12);
        assert_eq!(clamped, "a supercali…");
        assert!(width(&clamped) <= 12);
    }

    #[test]
    fn tags_are_single_tokens_within_a_bound() {
        assert_eq!(tag("  ci  ").unwrap().as_deref(), Some("ci"));
        assert_eq!(tag("   ").unwrap(), None);
        assert!(tag("two words").is_err());
        assert!(tag(&"x".repeat(TAG_MAX + 1)).is_err());
    }
}
