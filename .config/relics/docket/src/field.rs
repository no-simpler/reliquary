//! Every text field a person or an agent hands the CLI passes through here, so
//! one contract covers `create`, `set` and `relay` alike.
//!
//! Reading stays lenient on purpose: a file already on disk is never rejected
//! for length, only reported by `doctor`. The limits bind what is written.

use anyhow::{Result, bail};

/// A symbol, not a sentence: what a session says out loud, and what a listing
/// shows in a column narrow enough to scan. The sentence is the tagline's job.
pub const NAME_MAX: usize = 20;

/// Three is where a name stops being one and starts being a description.
pub const NAME_WORDS_MAX: usize = 3;

/// One terminal row — the same bound Debian puts on a package synopsis and
/// Homebrew on a formula description. Anything longer is body material.
pub const TAGLINE_MAX: usize = 80;

/// A block reason shares the tagline's column, so it wraps the same way.
pub const BLOCKED_MAX: usize = 80;

pub const TAG_MAX: usize = 32;

/// A quoted body line shares the tagline's column, so it wraps the same way a
/// block reason does.
pub const EXCERPT_MAX: usize = 80;

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

/// The canonical name, or the reason it will not do. Case and separator style
/// are normalised rather than refused — hyphen, space and underscore all mean
/// the same word break, which is how a corpus that drifted between them arrives
/// at one spelling. Everything else is a refusal naming the rule it broke.
pub fn name(label: &str, raw: &str) -> Result<String> {
    let trimmed = raw.trim();
    if trimmed.to_ascii_lowercase().ends_with(".md") {
        bail!("{label} is a name, not a filename — drop the .md");
    }

    let mut value = String::new();
    let mut pending = false;
    for ch in trimmed.chars() {
        if ch == '_' || ch == '-' || ch.is_whitespace() {
            pending = !value.is_empty();
            continue;
        }
        if pending {
            value.push('_');
            pending = false;
        }
        value.push(ch.to_ascii_uppercase());
    }

    if value.is_empty() {
        bail!("{label} is required");
    }
    if let Some(stray) = value
        .chars()
        .find(|c| !(c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_'))
    {
        bail!("{label} holds {stray:?}; a name is A-Z, 0-9 and underscore, like DREAM_RESIDUE");
    }
    if !value.chars().any(|c| c.is_ascii_uppercase()) {
        bail!("{label} is all digits; a name carries at least one letter");
    }
    let words = value.split('_').count();
    if words > NAME_WORDS_MAX {
        bail!("{label} is {words} words; a name is at most {NAME_WORDS_MAX}");
    }
    let length = width(&value);
    if length > NAME_MAX {
        bail!(
            "{label} is {length} characters; the limit is {NAME_MAX}. \
             It is the handle a session says out loud — what it means belongs \
             in the tagline"
        );
    }
    Ok(value)
}

/// The name if it still is one, else one minted from the id, which is always
/// well formed. For the repair path, where refusing is not an option.
pub fn recovered_name(raw: Option<&str>, id: &str) -> String {
    raw.and_then(|value| name("--name", value).ok())
        .unwrap_or_else(|| format!("RECOVERED_{}", id.to_ascii_uppercase()))
}

/// Whether a value already on disk is the canonical shape a name would be
/// accepted in now.
pub fn is_name(value: &str) -> bool {
    name("--name", value).is_ok_and(|canonical| canonical == value)
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

/// The stretch of a line around a match, for quoting what a search found. The
/// window is placed rather than taken from the head, because a line long enough
/// to need cutting is one whose match is usually not at its front, and an
/// elision stands for either end that was dropped.
///
/// `needle` is already lowered, as the caller matched with it.
pub fn excerpt(raw: &str, needle: &str, max: usize) -> String {
    let line = normalise(raw);
    let total = width(&line);
    if total <= max {
        return line;
    }
    let lowered = line.to_lowercase();
    // Case folding can change a character count, so a needle found in the
    // lowered line places the window approximately. The excerpt is a pointer at
    // the match, not a substitute for reading the item.
    let Some(byte) = lowered.find(needle) else {
        return clamp(&line, max);
    };
    let at = lowered[..byte].chars().count().min(total);

    // A quarter of the room goes to what led up to the match, so the match sits
    // where the eye lands rather than against the left edge.
    let start = at.saturating_sub(max / 4);
    let mut room = max.saturating_sub(usize::from(start > 0));
    if total - start > room {
        room = room.saturating_sub(1);
    }
    let end = (start + room).min(total);

    let mut out = String::new();
    if start > 0 {
        out.push('…');
    }
    out.extend(line.chars().skip(start).take(end - start));
    if end < total {
        out.push('…');
    }
    out
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
    fn a_name_normalises_case_and_either_separator() {
        for raw in [
            "  Rosetta migration ",
            "rosetta-migration",
            "ROSETTA_MIGRATION",
            "--rosetta -- migration--",
        ] {
            assert_eq!(name("--name", raw).unwrap(), "ROSETTA_MIGRATION", "{raw:?}");
        }
        assert_eq!(name("--name", "refs").unwrap(), "REFS");
        assert_eq!(name("--name", "phase2 rollout").unwrap(), "PHASE2_ROLLOUT");
    }

    #[test]
    fn a_name_refuses_every_shape_that_is_not_one() {
        for bad in [
            "",
            "   ",
            "ROSETTA.md",
            "rosetta.MD",
            "one two three four",
            "SOMETHING_RATHER_LONGER",
            "point.break",
            "quoted'name",
            "RÉSUMÉ",
            "2026",
        ] {
            assert!(name("--name", bad).is_err(), "{bad:?} should not pass");
        }
        assert!(
            name("--name", "ROSETTA.md")
                .unwrap_err()
                .to_string()
                .contains(".md")
        );
        assert_eq!(width(&"X".repeat(NAME_MAX)), NAME_MAX);
        assert!(name("--name", &"X".repeat(NAME_MAX)).is_ok());
        assert!(name("--name", &"X".repeat(NAME_MAX + 1)).is_err());
    }

    #[test]
    fn a_name_is_recovered_from_the_id_when_it_cannot_be_salvaged() {
        assert_eq!(
            recovered_name(Some("dream residue"), "nh7d"),
            "DREAM_RESIDUE"
        );
        assert_eq!(
            recovered_name(Some("a sentence that was once a title"), "nh7d"),
            "RECOVERED_NH7D"
        );
        assert_eq!(recovered_name(None, "4mve"), "RECOVERED_4MVE");
        assert!(is_name(&recovered_name(None, "4mve")));
    }

    #[test]
    fn only_the_canonical_spelling_counts_as_a_name() {
        assert!(is_name("DREAM_RESIDUE"));
        assert!(!is_name("dream_residue"));
        assert!(!is_name("DREAM-RESIDUE"));
        assert!(!is_name("Migrate the four legacy conventions"));
    }

    #[test]
    fn emptiness_and_overlength_are_both_refused() {
        assert!(one_line("--tagline", " \n ", TAGLINE_MAX).is_err());
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
    fn an_excerpt_is_placed_around_its_match() {
        let line = format!("{} needle {}", "a".repeat(60), "b".repeat(60));
        let quoted = excerpt(&line, "needle", EXCERPT_MAX);
        assert!(quoted.contains("needle"), "{quoted}");
        assert!(quoted.starts_with('…') && quoted.ends_with('…'), "{quoted}");
        assert!(width(&quoted) <= EXCERPT_MAX, "{quoted}");
    }

    #[test]
    fn a_line_that_fits_is_quoted_whole() {
        assert_eq!(
            excerpt("  short   line ", "line", EXCERPT_MAX),
            "short line"
        );
    }

    #[test]
    fn an_excerpt_keeps_the_head_when_the_match_is_there() {
        let line = format!("needle {}", "b".repeat(200));
        let quoted = excerpt(&line, "needle", EXCERPT_MAX);
        assert!(quoted.starts_with("needle"), "{quoted}");
        assert!(quoted.ends_with('…'), "{quoted}");
        assert!(width(&quoted) <= EXCERPT_MAX, "{quoted}");
    }

    #[test]
    fn an_excerpt_falls_back_to_clamping_when_the_needle_is_absent() {
        let line = "c".repeat(200);
        let quoted = excerpt(&line, "needle", EXCERPT_MAX);
        assert_eq!(quoted, clamp(&line, EXCERPT_MAX));
    }

    #[test]
    fn tags_are_single_tokens_within_a_bound() {
        assert_eq!(tag("  ci  ").unwrap().as_deref(), Some("ci"));
        assert_eq!(tag("   ").unwrap(), None);
        assert!(tag("two words").is_err());
        assert!(tag(&"x".repeat(TAG_MAX + 1)).is_err());
    }
}
