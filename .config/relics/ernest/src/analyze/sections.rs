//! Splitting a document into the units a de-prosing pass acts on.
//!
//! A file is too coarse for a long document: knowing that a 90 KB spec is
//! mostly prose says nothing about where to cut. Every byte is attributed to
//! its innermost enclosing `section`, so the per-character invariants the
//! golden tests assert still hold when the rows are summed.

use tree_sitter::Node;

/// A heading's own content, ending where its first subsection begins.
pub struct Section {
    /// The heading path that reaches it, `Heading > Subheading`.
    pub label: String,
    pub start: usize,
    pub end: usize,
}

/// Text before the first heading still belongs somewhere.
const PREAMBLE: &str = "(preamble)";

/// Innermost sections of `root`, in document order.
pub fn of(root: Node, src: &str) -> Vec<Section> {
    let mut out = Vec::new();
    walk(root, "", src, &mut out);
    out
}

fn walk(node: Node, prefix: &str, src: &str, out: &mut Vec<Section>) {
    if node.kind() != "section" {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(child, prefix, src, out);
        }
        return;
    }

    let label = match prefix {
        "" => heading(node, src),
        _ => format!("{prefix} > {}", heading(node, src)),
    };

    let mut cursor = node.walk();
    let nested: Vec<Node> = node
        .children(&mut cursor)
        .filter(|child| child.kind() == "section")
        .collect();

    out.push(Section {
        label: label.clone(),
        start: node.start_byte(),
        end: nested
            .first()
            .map_or(node.end_byte(), tree_sitter::Node::start_byte),
    });

    for child in nested {
        walk(child, &label, src, out);
    }
}

/// The heading text of a section, or `(preamble)` when it opens the document
/// without one.
fn heading(node: Node, src: &str) -> String {
    let mut cursor = node.walk();
    let text = node
        .children(&mut cursor)
        .find(|child| matches!(child.kind(), "atx_heading" | "setext_heading"))
        .and_then(|head| {
            let mut cursor = head.walk();
            head.children(&mut cursor)
                .find(|child| child.kind() == "inline" || child.kind() == "paragraph")
                .map(|inline| {
                    src.get(inline.start_byte()..inline.end_byte())
                        .unwrap_or("")
                        .trim()
                        .to_owned()
                })
        });
    match text {
        Some(text) if !text.is_empty() => text,
        _ => PREAMBLE.to_owned(),
    }
}
