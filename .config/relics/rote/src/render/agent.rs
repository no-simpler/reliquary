//! Aligned, uncoloured, and never a box. What a non-terminal caller gets.

use super::Table;

/// A titled block: heading, table, then any closing lines.
pub fn block(heading: &str, table: &Table, notes: &[String]) -> String {
    let mut lines = Vec::new();
    if !heading.is_empty() {
        lines.push(heading.to_owned());
    }
    if table.is_empty() {
        lines.push("nothing".to_owned());
    } else {
        lines.extend(super::aligned(table));
    }
    lines.extend(notes.iter().cloned());
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::super::Table;
    use super::block;

    #[test]
    fn a_block_is_heading_then_rows_then_notes() {
        let mut table = Table::new(&["slug"]);
        table.push(vec!["a".to_owned()]);
        let text = block("drills", &table, &["one note".to_owned()]);
        assert_eq!(text, "drills\nslug\na\none note");
    }

    #[test]
    fn an_empty_table_renders_as_a_word_rather_than_a_blank() {
        let text = block("drills", &Table::new(&["slug"]), &[]);
        assert_eq!(text, "drills\nnothing");
    }
}
