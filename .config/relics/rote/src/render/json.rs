//! Machine shape. One top-level key, and every row carrying its own context.

use anyhow::Result;
use serde::Serialize;

/// Render a document.
///
/// # Errors
///
/// When the value will not serialise.
pub fn document<T: Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string_pretty(value)?)
}

#[cfg(test)]
mod tests {
    use super::document;

    #[test]
    fn a_document_is_pretty_printed_json() {
        let text = document(&serde_json::json!({"slugs": []})).unwrap();
        assert_eq!(text, "{\n  \"slugs\": []\n}");
    }
}
