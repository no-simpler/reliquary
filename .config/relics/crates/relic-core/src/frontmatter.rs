//! A markdown document that opens with a YAML block: the metadata, and the body
//! it introduces.
//!
//! `docket` and `midden` each wrote [`split`] and the parser's line fixup by
//! hand, byte for byte, and `mantra` is the third to need them. That is the
//! platform bar met twice over, so the split lives here and the copies are gone.
//!
//! The shape is one every consumer already agreed on. The document opens with a
//! `---` line; the next `---` line on its own closes the metadata; everything
//! after it is the body, **returned untouched**. That last property is the one
//! callers build on — a rewrite renders the metadata again and hands the body
//! straight back, so a round trip cannot reflow prose, normalise a line ending,
//! or drop a trailing newline.
//!
//! What is deliberately *not* here: reading a file, and typing the result. Both
//! belong to the store that owns the document. This module is the parse and
//! nothing else.

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Why a document could not be read as metadata plus body.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The document does not open with a `---` line.
    #[error("no metadata: the file must open with a --- line")]
    Missing,
    /// It opened but never closed.
    #[error("unterminated metadata: no closing --- line")]
    Unterminated,
    /// Unreachable by construction; see [`split`].
    #[error("the metadata does not end on a character boundary")]
    Boundary,
    /// The YAML did not describe what the caller asked for, at a line and column
    /// counted from the top of the *file*.
    #[error("line {line}, column {column}: {message}")]
    Parse {
        /// One-based, and offset past the opening `---`.
        line: usize,
        /// As the YAML parser reported it.
        column: usize,
        /// The parser's complaint, with its own coordinates removed.
        message: String,
    },
    /// The YAML did not parse and the parser named no position.
    #[error("{0}")]
    Yaml(String),
    /// A value that cannot be written back out as YAML.
    #[error("rendering the metadata")]
    Render(#[source] serde_yaml_ng::Error),
}

/// Splits a document into its metadata and its body. The body is returned
/// untouched, so every rewrite preserves it exactly.
///
/// # Errors
///
/// [`Error::Missing`] when the document does not open with `---`,
/// [`Error::Unterminated`] when nothing closes it.
pub fn split(text: &str) -> Result<(&str, &str), Error> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .ok_or(Error::Missing)?;

    // `get` rather than a slice: the offsets come from `split_inclusive`, so they
    // are character boundaries, and saying so with a total operation costs one
    // error arm that cannot fire.
    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            let front = rest.get(..offset).ok_or(Error::Boundary)?;
            let body = rest.get(offset + line.len()..).ok_or(Error::Boundary)?;
            return Ok((front, body));
        }
        offset += line.len();
    }
    Err(Error::Unterminated)
}

/// Reads the metadata of a document already [`split`] from its body.
///
/// # Errors
///
/// [`Error::Parse`] or [`Error::Yaml`] when the YAML does not describe `T`.
pub fn parse<T: DeserializeOwned>(front: &str) -> Result<T, Error> {
    serde_yaml_ng::from_str(front).map_err(|e| match e.location() {
        // The opening `---` occupies line one, so the parser's line number is
        // one short of the line a reader would count in the file.
        Some(at) => Error::Parse {
            line: at.line() + 1,
            column: at.column(),
            message: without_location(&e.to_string()).to_owned(),
        },
        None => Error::Yaml(e.to_string()),
    })
}

/// Renders `value` as the metadata of a document, above `body`.
///
/// # Errors
///
/// [`Error::Render`] when `value` has no YAML form.
pub fn render<T: Serialize>(value: &T, body: &str) -> Result<String, Error> {
    let front = serde_yaml_ng::to_string(value).map_err(Error::Render)?;
    Ok(format!("---\n{front}---\n{body}"))
}

/// The parser appends its own coordinates, which are relative to the metadata
/// rather than to the file. One location per message, and it should be the one
/// a reader can act on.
fn without_location(message: &str) -> &str {
    match message.rfind(" at line ") {
        Some(cut) => message.get(..cut).unwrap_or(message).trim_end(),
        None => message,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
    #[serde(deny_unknown_fields)]
    struct Meta {
        id: String,
    }

    #[test]
    fn splitting_finds_the_body_untouched() {
        let text = "---\nid: b71c\n---\n# Title\n\nbody\n";
        let (front, body) = split(text).unwrap();
        assert_eq!(front, "id: b71c\n");
        assert_eq!(body, "# Title\n\nbody\n");
    }

    #[test]
    fn splitting_rejects_documents_without_metadata() {
        assert!(matches!(split("# no metadata\n"), Err(Error::Missing)));
        assert!(matches!(split("---\nid: b71c\n"), Err(Error::Unterminated)));
    }

    #[test]
    fn a_carriage_return_still_opens_a_document() {
        let (front, body) = split("---\r\nid: b71c\n---\nbody\n").unwrap();
        assert_eq!(front, "id: b71c\n");
        assert_eq!(body, "body\n");
    }

    #[test]
    fn empty_metadata_is_metadata() {
        let (front, body) = split("---\n---\nbody\n").unwrap();
        assert_eq!(front, "");
        assert_eq!(body, "body\n");
    }

    #[test]
    fn a_later_rule_does_not_close_the_metadata_twice() {
        // The body's own `---` is body, because the first closing line wins.
        let (front, body) = split("---\nid: b71c\n---\nabove\n---\nbelow\n").unwrap();
        assert_eq!(front, "id: b71c\n");
        assert_eq!(body, "above\n---\nbelow\n");
    }

    #[test]
    fn a_parse_error_counts_lines_from_the_top_of_the_file() {
        let text = "---\nid: b71c\nunknown: 1\n---\n";
        let (front, _) = split(text).unwrap();
        let error = parse::<Meta>(front).unwrap_err();
        let Error::Parse { line, message, .. } = &error else {
            panic!("expected a located parse error, got {error:?}");
        };
        // `unknown` is on line 3 of the file and line 2 of the metadata.
        assert_eq!(*line, 3);
        assert!(!message.contains(" at line "), "{message}");
    }

    #[test]
    fn rendering_round_trips_through_splitting() {
        let meta = Meta {
            id: "b71c".to_owned(),
        };
        let text = render(&meta, "# Title\n\nbody\n").unwrap();
        let (front, body) = split(&text).unwrap();
        assert_eq!(parse::<Meta>(front).unwrap(), meta);
        assert_eq!(body, "# Title\n\nbody\n");
    }

    proptest::proptest! {
        /// The body comes back byte for byte, which is what every rewrite of a
        /// document relies on: only the metadata is ever rendered again.
        #[test]
        fn splitting_preserves_the_body(body in "(?s).*") {
            let text = format!("---\nid: b71c\n---\n{body}");
            let (front, found) = split(&text).expect("the document opens with metadata");
            proptest::prop_assert_eq!(front, "id: b71c\n");
            proptest::prop_assert_eq!(found, body.as_str());
        }
    }
}
