//! The format registry.
//!
//! Adding a language is adding one `Profile` constant here and listing it in
//! `PROFILES`. Write a bespoke `Analyzer` only when a profile cannot express
//! the format.

use tree_sitter_language::LanguageFn;

use crate::span::Class;

/// Which figure a language contributes to. Prose-by-nature formats such as
/// Markdown belong to `Docs` and are reported separately — folding them into
/// the source denominator would swamp it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Cohort {
    Source,
    // Unused until the first documentation profile lands; the split has to
    // exist before it is needed, or the headline silently absorbs prose.
    #[allow(dead_code)]
    Docs,
}

impl Cohort {
    pub fn label(self) -> &'static str {
        match self {
            Cohort::Source => "source",
            Cohort::Docs => "docs",
        }
    }
}

pub struct Profile {
    pub language: &'static str,
    pub language_fn: LanguageFn,
    pub extensions: &'static [&'static str],
    pub filenames: &'static [&'static str],
    pub cohort: Cohort,
    /// What bytes no rule claims are worth. `Code` for source languages, so an
    /// analyzer's whole job is finding prose; `Prose` for documentation
    /// formats, where the polarity inverts and code blocks are what get named.
    pub default_class: Class,
    /// tree-sitter node kinds holding prose.
    pub prose_nodes: &'static [&'static str],
    /// tree-sitter node kinds that are unavoidable given the file exists —
    /// counted toward neither side of the ratio.
    pub ignored_nodes: &'static [&'static str],
    /// Comment sigils, stripped only to read a line's body. They still count
    /// as prose; a decorative banner is mostly delimiter and should be billed
    /// as the ornament it is.
    pub comment_frame: &'static [&'static str],
    /// Body prefixes marking a machine-consumed annotation. Such a line is
    /// avoidable but is not prose, so it re-classifies to code.
    pub annotation_line: &'static [&'static str],
}

/// Comment bodies opening with one of these are tooling directives, not prose:
/// machine-consumed, and unavoidable once you want the tooling. The whole
/// comment becomes uninteresting.
pub const PRAGMA_PREFIXES: &[&str] = &[
    "yaml-language-server:",
    "yamllint ",
    "phpcs:",
    "phpstan-",
    "psalm-",
    "@phpcs",
    "@phpstan-",
    "@psalm-",
    "noqa",
    "shellcheck ",
    "SPDX-License-Identifier:",
    "vim:",
    "type:",
    "editorconfig-checker-",
    "prettier-ignore",
    "eslint-disable",
];

pub static PHP: Profile = Profile {
    language: "php",
    language_fn: tree_sitter_php::LANGUAGE_PHP,
    extensions: &["php", "phtml", "php4", "php5", "php7", "phps"],
    filenames: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    // One `comment` kind covers //, #, /* */ and /** */ alike.
    prose_nodes: &["comment"],
    ignored_nodes: &["php_tag", "php_end_tag"],
    comment_frame: &["/**", "*/", "/*", "//", "*", "#"],
    annotation_line: &["@"],
};

pub static YAML: Profile = Profile {
    language: "yaml",
    language_fn: tree_sitter_yaml::LANGUAGE,
    extensions: &["yaml", "yml"],
    filenames: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["comment"],
    // Document markers are structure you cannot write your way out of.
    ignored_nodes: &["---", "..."],
    comment_frame: &["#"],
    annotation_line: &[],
};

pub static PROFILES: &[&Profile] = &[&PHP, &YAML];

/// Strip comment sigils and surrounding whitespace to expose a line's body,
/// so pragma and annotation prefixes can be tested against real content.
pub fn comment_body<'a>(line: &'a str, frame: &[&str]) -> &'a str {
    let mut body = line.trim();
    // Two passes clear the deepest real nesting: `/** @param` and ` * @param`.
    for _ in 0..2 {
        let before = body;
        for sigil in frame {
            if let Some(rest) = body.strip_prefix(sigil) {
                body = rest.trim_start();
                break;
            }
        }
        if body == before {
            break;
        }
    }
    body.trim()
}
