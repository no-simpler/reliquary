//! Turning a file into classified byte spans.

pub mod profiles;
pub mod sections;

use anyhow::{Context, Result};
use tree_sitter::{Node, Parser, Tree};

use crate::span::{Class, Counts, Span, measure, measure_range};
use profiles::{PRAGMA_PREFIXES, Profile, comment_body};

/// Classify `src` under `profile` and roll the result up into counts.
pub fn analyze_file(src: &str, profile: &Profile) -> Result<Counts> {
    let tree = parse(src, profile)?;
    let spans = classify(&tree, src, profile);
    Ok(measure(src, &spans, profile.default_class))
}

/// As `analyze_file`, plus one row per innermost section of the document.
/// Sections that measure to nothing are dropped rather than reported as `n/a`.
pub fn analyze_sections(src: &str, profile: &Profile) -> Result<(Counts, Vec<(String, Counts)>)> {
    let tree = parse(src, profile)?;
    let spans = classify(&tree, src, profile);
    let rows = sections::of(tree.root_node(), src)
        .into_iter()
        .map(|section| {
            let counts = measure_range(src, &spans, profile.default_class, section.start, section.end);
            (section.label, counts)
        })
        .filter(|(_, c)| c.prose_chars + c.code_chars + c.ignored_chars > 0)
        .collect();
    Ok((measure(src, &spans, profile.default_class), rows))
}

/// Public so a test can ask whether a grammar actually read a file, rather than
/// only what the rules made of what it returned.
pub fn parse(src: &str, profile: &Profile) -> Result<Tree> {
    let mut parser = Parser::new();
    parser
        .set_language(&profile.language_fn.into())
        .with_context(|| format!("loading the {} grammar", profile.language))?;
    parser
        .parse(src, None)
        .with_context(|| format!("parsing as {}", profile.language))
}

/// Ordered, non-overlapping spans for everything the profile can name.
/// Bytes no span covers belong to `profile.default_class`.
fn classify(tree: &Tree, src: &str, profile: &Profile) -> Vec<Span> {
    let mut spans = Vec::new();
    collect(tree.root_node(), profile, src, &mut spans);

    // A shebang is unavoidable in an executable script, and grammars disagree
    // on it: tree-sitter-bash lexes it as a comment, tree-sitter-php as inline
    // text, tree-sitter-rust as a node of its own. So the rule is generic, and
    // it overrides whatever the walk decided about the spans the line holds.
    if let Some(eol) = crate::detect::shebang_len(src) {
        let inside = spans.iter().take_while(|s| s.end <= eol).count();
        // A span straddling the line end is nothing this rule can split, so it
        // leaves the file alone rather than guess.
        if spans.get(inside).is_none_or(|s| s.start >= eol) {
            spans.splice(..inside, [Span::new(0, eol, Class::Ignored)]);
        }
    }

    fold_generated_regions(&mut spans, profile, src);
    spans
}

/// Walk the tree, taking the outermost node that matches a rule and never
/// descending into it — which is what keeps the spans non-overlapping.
fn collect(node: Node, profile: &Profile, src: &str, out: &mut Vec<Span>) {
    let kind = node.kind();

    for (class, kinds) in [
        (Class::Ignored, profile.ignored_nodes),
        (Class::Code, profile.code_nodes),
        (Class::Prose, profile.prose_nodes),
    ] {
        if kinds.contains(&kind) {
            push_recognised(class, node.start_byte(), node.end_byte(), profile, src, out);
            return;
        }
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect(child, profile, src, out);
    }
}

/// Split a recognised node into what it actually contains: a tooling directive
/// is uninteresting whole, an annotation line is code, and prose keeps its
/// delimiters.
fn push_recognised(
    class: Class,
    start: usize,
    end: usize,
    profile: &Profile,
    src: &str,
    out: &mut Vec<Span>,
) {
    let text = &src[start..end];

    // Ahead of the class branch, so a directive is uninteresting whether it
    // arrived as a comment or as raw markup.
    if class != Class::Ignored && is_pragma(text, profile) {
        out.push(Span::new(start, end, Class::Ignored));
        return;
    }

    if class != Class::Prose || profile.annotation_line.is_empty() {
        out.push(Span::new(start, end, class));
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

/// Collapse each opener/closer pair and everything between it into one
/// uninteresting span. Content a tool rewrites is not the author's to cut, and
/// the region's interior may hold no spans of its own — hence a post-pass over
/// the span list rather than a rule in the walk.
fn fold_generated_regions(spans: &mut Vec<Span>, profile: &Profile, src: &str) {
    if profile.generated_regions.is_empty() {
        return;
    }
    let mut at = 0usize;
    while at < spans.len() {
        let opened = profile
            .generated_regions
            .iter()
            .find(|(open, _)| marker(&spans[at], profile, src).starts_with(open));
        let Some((_, close)) = opened else {
            at += 1;
            continue;
        };
        // An unclosed opener leaves the rest of the file alone; a later pair
        // still folds.
        let closed = (at + 1..spans.len())
            .find(|&i| marker(&spans[i], profile, src).starts_with(close));
        let Some(end) = closed else {
            at += 1;
            continue;
        };
        let region = Span::new(spans[at].start, spans[end].end, Class::Ignored);
        spans.splice(at..=end, [region]);
        at += 1;
    }
}

/// A span's first line, stripped to its comment body, so region markers are
/// tested against real content.
fn marker<'a>(span: &Span, profile: &Profile, src: &'a str) -> &'a str {
    let text = &src[span.start..span.end];
    comment_body(text.lines().next().unwrap_or(""), profile.comment_frame)
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
    use profiles::{MARKDOWN, PHP, RUST, SHELL, TOML, YAML};

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

    /// tree-sitter-bash lexes the shebang as a comment, so the rule has to
    /// override a span rather than only fill a gap.
    #[test]
    fn a_shebang_the_grammar_claimed_as_a_comment_is_still_uninteresting() {
        let c = counts("#!/usr/bin/env bash\nx=1\n", &SHELL);
        assert_eq!(c.ignored_chars, "#!/usr/bin/envbash".len() as u64);
        assert_eq!(c.prose_chars, 0);
        assert_eq!(c.code_chars, "x=1".len() as u64);
    }

    #[test]
    fn hashes_that_are_shell_syntax_are_not_comments() {
        for src in [
            "p=\"${PWD##*/}\"\n",
            "n=${#REPOS[@]}\n",
            "while [[ $# -gt 0 ]]; do shift; done\n",
            "case $x in '#'*|'') echo a ;; \\#*) echo b ;; esac\n",
            "awk '/^[[:space:]]*#/ { next }' f\n",
            "echo \"value # not a comment\"\n",
        ] {
            assert_eq!(counts(src, &SHELL).prose_chars, 0, "{src}");
        }
    }

    #[test]
    fn heredoc_content_is_not_a_comment_in_shell() {
        let quoted = "cat <<'EOF'\n#!/bin/bash\n# nope\nEOF\n";
        assert_eq!(counts(quoted, &SHELL).prose_chars, 0);
        let expanded = "cat <<EOF\n#!/bin/bash\n# nope\nEOF\n";
        assert_eq!(counts(expanded, &SHELL).prose_chars, 0);
    }

    #[test]
    fn a_shellcheck_directive_is_uninteresting_not_prose() {
        let c = counts("# shellcheck disable=SC1091\nsource f\n", &SHELL);
        assert_eq!(c.prose_chars, 0);
        assert_eq!(c.ignored_chars, "#shellcheckdisable=SC1091".len() as u64);
    }

    /// `##`, `#>` and `#.` are house comment sigils, not a machine-consumed
    /// convention, so nothing about them reclassifies to code.
    #[test]
    fn the_house_comment_sigils_are_prose() {
        let c = counts("## Section\n#>  mcd PATH\n#.  -a  dotfiles\n", &SHELL);
        assert_eq!(c.code_chars, 0);
        assert_eq!(c.ignored_chars, 0);
        assert_eq!(c.prose_chars, "##Section#>mcdPATH#.-adotfiles".len() as u64);
    }

    #[test]
    fn doc_comments_and_ordinary_comments_are_prose_alike() {
        let src = "//! Module doc.\n/// Item doc.\n// Plain note.\nfn f() {}\n";
        let c = counts(src, &RUST);
        assert_eq!(
            c.prose_chars,
            "//!Moduledoc.///Itemdoc.//Plainnote.".len() as u64
        );
        assert_eq!(c.code_chars, "fnf(){}".len() as u64);
    }

    /// The one that made this profile worth writing carefully: the line is an
    /// attribute, not a shebang, and billing it as unavoidable would write off
    /// the most common first line in the language.
    #[test]
    fn an_inner_attribute_on_line_one_is_a_pragma_not_a_shebang() {
        let c = counts("#![deny(missing_docs)]\nfn f() {}\n", &RUST);
        assert_eq!(c.ignored_chars, "#![deny(missing_docs)]".len() as u64);
        assert_eq!(c.prose_chars, 0);
        assert_eq!(c.code_chars, "fnf(){}".len() as u64);
    }

    /// A lint directive is uninteresting whichever vehicle a language gives it;
    /// an attribute that carries meaning is code.
    #[test]
    fn lint_attributes_are_uninteresting_and_semantic_ones_are_code() {
        let lint = counts("#[allow(dead_code)]\nfn f() {}\n", &RUST);
        assert_eq!(lint.ignored_chars, "#[allow(dead_code)]".len() as u64);

        let semantic = counts("#[derive(Debug)]\nstruct S;\n", &RUST);
        assert_eq!(semantic.ignored_chars, 0);
        assert_eq!(semantic.code_chars, "#[derive(Debug)]structS;".len() as u64);
    }

    #[test]
    fn slashes_that_are_rust_syntax_are_not_comments() {
        for src in [
            "let u = \"http://example.com/#frag\";\n",
            "let r = r\"C:\\\\ // still a string\";\n",
            "let r = r#\"a \"quoted\" // thing\"#;\n",
            "let b = b\"bytes // here\";\n",
            "let q = a / b / c;\n",
            "fn f<'a>(x: &'a str) -> char { '/' }\n",
            "'outer: loop { break 'outer; }\n",
        ] {
            assert_eq!(counts(src, &RUST).prose_chars, 0, "{src}");
        }
    }

    /// Four slashes are not a doc comment, but they are still a comment.
    #[test]
    fn a_nested_block_comment_closes_once_and_four_slashes_are_prose() {
        let nested = counts("/* outer /* inner */ still outer */\nfn f() {}\n", &RUST);
        assert_eq!(nested.code_chars, "fnf(){}".len() as u64);

        let four = counts("//// not a doc comment\n", &RUST);
        assert_eq!(four.prose_chars, "////notadoccomment".len() as u64);
    }

    /// Comments reach inside macro token trees, so the coverage does not stop
    /// at a `println!`.
    #[test]
    fn a_comment_inside_a_macro_is_still_a_comment() {
        let c = counts("fn f() {\n    println!(\"{}\", /* note */ 1);\n}\n", &RUST);
        assert_eq!(c.prose_chars, "/*note*/".len() as u64);
    }

    #[test]
    fn hashes_inside_toml_strings_are_not_comments() {
        for src in [
            "url = \"https://e.com/#frag\"\n",
            "lit = '#raw'\n",
            "\"quoted#key\" = 2\n",
            "multi = \"\"\"\n# not a comment\n\"\"\"\n",
            "inline = { a = \"x#y\" }\n",
        ] {
            assert_eq!(counts(src, &TOML).prose_chars, 0, "{src}");
        }
    }

    #[test]
    fn a_toml_comment_is_prose_wherever_it_sits() {
        let c = counts("[a]\n# Why this table.\nkey = 1 # why\n", &TOML);
        assert_eq!(c.prose_chars, "#Whythistable.#why".len() as u64);
        assert_eq!(c.code_chars, "[a]key=1".len() as u64);
        assert_eq!(c.ignored_chars, 0);
    }

    #[test]
    fn a_toml_schema_directive_is_uninteresting_not_prose() {
        let c = counts("#:schema https://e.com/s.json\nkey = 1\n", &TOML);
        assert_eq!(c.prose_chars, 0);
        assert_eq!(c.ignored_chars, "#:schemahttps://e.com/s.json".len() as u64);
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
    fn a_fence_is_uninteresting_and_what_it_holds_is_code() {
        let c = counts("# H\n\n```php\n$x = 1;\n```\n", &MARKDOWN);
        assert_eq!(c.prose_chars, "#H".len() as u64);
        assert_eq!(c.code_chars, "$x=1;".len() as u64);
        // Both delimiters and the info string.
        assert_eq!(c.ignored_chars, "``````php".len() as u64);
    }

    #[test]
    fn a_hash_inside_a_code_span_is_not_a_heading() {
        let c = counts("A paragraph naming `# not a heading` in passing.\n", &MARKDOWN);
        assert_eq!(c.code_chars, 0);
        assert_eq!(c.ignored_chars, 0);
        assert!(c.prose_chars > 0);
    }

    #[test]
    fn front_matter_is_uninteresting() {
        let c = counts("---\nkey: value\n---\n\n# H\n", &MARKDOWN);
        assert_eq!(c.ignored_chars, "---key:value---".len() as u64);
        assert_eq!(c.prose_chars, "#H".len() as u64);
    }

    #[test]
    fn table_cells_are_prose_and_the_structure_holding_them_is_code() {
        let c = counts("| a | b |\n| - | - |\n| c | d |\n", &MARKDOWN);
        assert_eq!(c.prose_chars, "abcd".len() as u64);
        // Six pipes framing the two content rows, and the delimiter row whole.
        assert_eq!(c.code_chars, "||||||".len() as u64 + "|-|-|".len() as u64);
    }

    #[test]
    fn a_generated_region_is_uninteresting_whole() {
        let c = counts("<!-- TOC -->\n\n- [a](#a)\n\n<!-- /TOC -->\n", &MARKDOWN);
        assert_eq!(c.prose_chars, 0);
        assert_eq!(c.code_chars, 0);
    }

    #[test]
    fn raw_markup_is_code_but_a_directive_is_uninteresting() {
        let c = counts("<!-- rumdl-disable MD033 -->\n\n<div>x</div>\n", &MARKDOWN);
        assert_eq!(c.prose_chars, 0);
        assert_eq!(c.code_chars, "<div>x</div>".len() as u64);
        assert_eq!(c.ignored_chars, "<!--rumdl-disableMD033-->".len() as u64);
    }

    fn labels(src: &str) -> Vec<String> {
        analyze_sections(src, &MARKDOWN)
            .unwrap()
            .1
            .into_iter()
            .map(|(label, _)| label)
            .collect()
    }

    #[test]
    fn a_section_label_is_the_heading_path_that_reaches_it() {
        assert_eq!(
            labels("# One\n\ntext\n\n## Two\n\ntext\n\n### Three\n\ntext\n"),
            ["One", "One > Two", "One > Two > Three"]
        );
    }

    #[test]
    fn text_before_the_first_heading_is_its_own_section() {
        assert_eq!(labels("Opening text.\n\n# One\n\ntext\n"), ["(preamble)", "One"]);
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
