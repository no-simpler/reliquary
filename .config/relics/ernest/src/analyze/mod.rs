//! Turning a file into classified byte spans.

pub mod profiles;

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser};

use crate::span::{Class, Counts, Span, measure};
use profiles::{PRAGMA_PREFIXES, Profile, comment_body};

/// Classify `src` under `profile` and roll the result up into counts.
pub fn analyze_file(src: &str, profile: &Profile) -> Result<Counts> {
    let spans = classify(src, profile)?;
    Ok(measure(src, &spans, profile.default_class))
}

/// Ordered, non-overlapping spans for everything the profile can name.
/// Bytes no span covers belong to `profile.default_class`.
pub fn classify(src: &str, profile: &Profile) -> Result<Vec<Span>> {
    let mut parser = Parser::new();
    parser
        .set_language(&profile.language_fn.into())
        .with_context(|| format!("loading the {} grammar", profile.language))?;
    let tree = parser
        .parse(src, None)
        .with_context(|| format!("parsing as {}", profile.language))?;

    let mut spans = Vec::new();
    collect(tree.root_node(), profile, src, &mut spans);

    // A shebang is unavoidable in an executable script. It is not part of any
    // comment node — tree-sitter-php lexes it as inline text — so the rule is
    // generic rather than per-profile.
    if src.starts_with("#!") {
        let eol = src.find('\n').unwrap_or(src.len());
        if spans.first().is_none_or(|s| s.start >= eol) {
            spans.insert(0, Span::new(0, eol, Class::Ignored));
        }
    }

    Ok(spans)
}

/// Walk the tree, taking the outermost node that matches a rule and never
/// descending into it — which is what keeps the spans non-overlapping.
fn collect(node: Node, profile: &Profile, src: &str, out: &mut Vec<Span>) {
    let kind = node.kind();

    if profile.ignored_nodes.contains(&kind) {
        out.push(Span::new(node.start_byte(), node.end_byte(), Class::Ignored));
        return;
    }

    if profile.prose_nodes.contains(&kind) {
        push_prose(node.start_byte(), node.end_byte(), profile, src, out);
        return;
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, profile, src, out);
    }
}

/// Split a recognised comment into what it actually contains: a tooling
/// directive is uninteresting whole, an annotation line is code, and the rest
/// is prose — delimiters included.
fn push_prose(start: usize, end: usize, profile: &Profile, src: &str, out: &mut Vec<Span>) {
    let text = &src[start..end];

    if is_pragma(text, profile) {
        out.push(Span::new(start, end, Class::Ignored));
        return;
    }

    if profile.annotation_line.is_empty() {
        out.push(Span::new(start, end, Class::Prose));
        return;
    }

    let mut pending: Option<Span> = None;
    for (offset, line) in line_offsets(text) {
        let ls = start + offset;
        let le = ls + line.len();
        let body = comment_body(line, profile.comment_frame);
        let class = if profile
            .annotation_line
            .iter()
            .any(|prefix| body.starts_with(prefix))
        {
            Class::Code
        } else {
            Class::Prose
        };

        match pending {
            // Coalesce runs of the same class so the span list stays short.
            Some(ref mut span) if span.class == class => span.end = le,
            Some(span) => {
                out.push(span);
                pending = Some(Span::new(ls, le, class));
            }
            None => pending = Some(Span::new(ls, le, class)),
        }
    }
    if let Some(span) = pending {
        out.push(span);
    }
}

/// True when the comment opens with a machine-consumed directive.
fn is_pragma(text: &str, profile: &Profile) -> bool {
    let Some(first) = line_offsets(text)
        .map(|(_, line)| comment_body(line, profile.comment_frame))
        .find(|body| !body.is_empty())
    else {
        return false;
    };
    PRAGMA_PREFIXES
        .iter()
        .any(|prefix| first.starts_with(prefix))
}

/// Lines of `text` with their byte offsets, newline excluded, final line kept
/// whether or not it is terminated.
fn line_offsets(text: &str) -> impl Iterator<Item = (usize, &str)> {
    let mut at = 0usize;
    std::iter::from_fn(move || {
        if at > text.len() {
            return None;
        }
        let rest = &text[at..];
        let (line, step) = match rest.find('\n') {
            Some(i) => (&rest[..i], i + 1),
            None => (rest, rest.len() + 1),
        };
        let start = at;
        at += step;
        Some((start, line))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use profiles::{PHP, YAML};

    fn counts(src: &str, profile: &Profile) -> Counts {
        analyze_file(src, profile).unwrap()
    }

    #[test]
    fn a_url_in_a_string_is_not_a_comment() {
        let c = counts("<?php\n$u = \"http://example.com\";\n", &PHP);
        assert_eq!(c.prose_chars, 0);
    }

    #[test]
    fn heredoc_content_is_not_a_comment() {
        let c = counts("<?php\n$s = <<<SQL\n  -- nope # nope\nSQL;\n", &PHP);
        assert_eq!(c.prose_chars, 0);
    }

    #[test]
    fn annotation_lines_are_code_and_description_lines_are_prose() {
        let src = "<?php\n/**\n * Resolves a tenant.\n * @param int $id\n */\n$x = 1;\n";
        let c = counts(src, &PHP);
        // "/**", "* Resolves a tenant." and "*/" stay prose, delimiters included.
        assert_eq!(c.prose_chars, "/**".len() as u64 + "*Resolvesatenant.".len() as u64 + 2);
        // The annotation line reclassifies whole, frame and all.
        assert_eq!(c.code_chars, "*@paramint$id".len() as u64 + "$x=1;".len() as u64);
    }

    #[test]
    fn the_php_open_tag_is_uninteresting() {
        let c = counts("<?php\n$x = 1;\n", &PHP);
        assert_eq!(c.ignored_chars, 5);
        assert_eq!(c.code_chars, 5);
        assert_eq!(c.prose_chars, 0);
    }

    #[test]
    fn a_shebang_is_uninteresting() {
        let c = counts("#!/usr/bin/env php\n<?php\n$x = 1;\n", &PHP);
        assert_eq!(c.ignored_chars, "#!/usr/bin/envphp".len() as u64 + 5);
        assert_eq!(c.prose_chars, 0);
    }

    #[test]
    fn a_decorative_banner_is_billed_as_prose() {
        let banner = "<?php\n// ====================\n$x = 1;\n";
        let c = counts(banner, &PHP);
        assert_eq!(c.prose_chars, 22);
    }

    #[test]
    fn hashes_inside_yaml_scalars_are_not_comments() {
        let src = "key: \"value # not a comment\"\nblock: |\n  # not a comment\n";
        let c = counts(src, &YAML);
        assert_eq!(c.prose_chars, 0);
    }

    #[test]
    fn yaml_document_markers_are_uninteresting() {
        let c = counts("---\nkey: 1\n...\n", &YAML);
        assert_eq!(c.ignored_chars, 6);
    }

    #[test]
    fn a_tooling_directive_is_uninteresting_not_prose() {
        let c = counts("# yaml-language-server: $schema=./x.json\nkey: 1\n", &YAML);
        assert_eq!(c.prose_chars, 0);
        assert_eq!(c.ignored_chars, 38);
    }

    #[test]
    fn a_trailing_comment_splits_its_line() {
        // The whole point of counting characters: a line-based counter would
        // call this line code and lose the comment entirely.
        let c = counts("key: 1  # why this value\n", &YAML);
        assert_eq!(c.code_chars, 5); // "key:1"
        assert_eq!(c.prose_chars, 13); // "#whythisvalue"
    }

    #[test]
    fn a_split_line_resolves_to_whichever_class_holds_most_of_it() {
        let mostly_prose = counts("key: 1  # why this value\n", &YAML);
        assert_eq!(mostly_prose.prose_lines, 1);
        assert_eq!(mostly_prose.code_lines, 0);

        let mostly_code = counts("some: mapping value here  # why\n", &YAML);
        assert_eq!(mostly_code.code_lines, 1);
        assert_eq!(mostly_code.prose_lines, 0);
    }
}
