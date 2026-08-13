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
    /// Interpreter basenames a shebang may name. The only thing that identifies
    /// a script carrying no extension, which is what a personal-bin utility is.
    pub interpreters: &'static [&'static str],
    pub cohort: Cohort,
    /// What bytes no rule claims are worth. `Code` for source languages, so an
    /// analyzer's whole job is finding prose; `Prose` for documentation
    /// formats, where the polarity inverts and code blocks are what get named.
    pub default_class: Class,
    /// tree-sitter node kinds holding prose.
    pub prose_nodes: &'static [&'static str],
    /// tree-sitter node kinds holding code. What a documentation format names,
    /// its default class being `Prose`.
    pub code_nodes: &'static [&'static str],
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
    /// Pragma bodies opening and closing a region a tool writes and rewrites.
    /// The pair and everything between it is uninteresting whatever it holds,
    /// because nobody authored it.
    pub generated_regions: &'static [(&'static str, &'static str)],
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
    "TOC",
    "/TOC",
    "rumdl-disable",
    "rumdl-enable",
    "markdownlint-disable",
    "markdownlint-enable",
    "vale off",
    "vale on",
];

pub static PHP: Profile = Profile {
    language: "php",
    language_fn: tree_sitter_php::LANGUAGE_PHP,
    extensions: &["php", "phtml", "php4", "php5", "php7", "phps"],
    filenames: &[],
    interpreters: &["php"],
    cohort: Cohort::Source,
    default_class: Class::Code,
    // One `comment` kind covers //, #, /* */ and /** */ alike.
    prose_nodes: &["comment"],
    code_nodes: &[],
    ignored_nodes: &["php_tag", "php_end_tag"],
    comment_frame: &["/**", "*/", "/*", "//", "*", "#"],
    annotation_line: &["@"],
    generated_regions: &[],
};

pub static YAML: Profile = Profile {
    language: "yaml",
    language_fn: tree_sitter_yaml::LANGUAGE,
    extensions: &["yaml", "yml"],
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["comment"],
    code_nodes: &[],
    // Document markers are structure you cannot write your way out of.
    ignored_nodes: &["---", "..."],
    comment_frame: &["#"],
    annotation_line: &[],
    generated_regions: &[],
};

/// POSIX sh and bash, which is what tree-sitter-bash declares and parses
/// cleanly. Do not widen `extensions` to the other dialects: on real zsh the
/// grammar errors in most files, loses about an eighth of the comment
/// characters, and bills `(#i)` glob flags as prose. fish needs its own
/// grammar, and that crate still predates `LanguageFn`.
///
/// Nothing is uninteresting by node kind — shell has no counterpart to `<?php`
/// or a YAML document marker, and the shebang is handled for every language in
/// `analyze::classify`. Nothing is an annotation either: `##`, `#>` and `#.`
/// are house comment sigils, not a machine-consumed convention like `@param`.
pub static SHELL: Profile = Profile {
    language: "shell",
    language_fn: tree_sitter_bash::LANGUAGE,
    extensions: &["sh", "bash"],
    filenames: &[
        ".bashrc",
        ".bash_profile",
        ".bash_login",
        ".bash_logout",
        ".bash_env",
        ".bash_aliases",
        ".profile",
    ],
    interpreters: &["sh", "bash", "dash"],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["comment"],
    code_nodes: &[],
    ignored_nodes: &[],
    comment_frame: &["#"],
    annotation_line: &[],
    generated_regions: &[],
};

/// The polarity inverts here: prose is the default and code is what gets named.
///
/// The line between prose and code runs through structure, not around it.
/// Structure that scales with the construct bills as code — a wide table's
/// pipes are mass that is not text, the way braces are. A one-off frame bills
/// as prose, the way comment delimiters do: a heading's `#` exists only because
/// the heading does.
pub static MARKDOWN: Profile = Profile {
    language: "markdown",
    language_fn: tree_sitter_md::LANGUAGE,
    extensions: &["md", "markdown", "mdown", "mkd", "mkdn"],
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Docs,
    default_class: Class::Prose,
    prose_nodes: &[],
    code_nodes: &[
        "code_fence_content",
        "indented_code_block",
        "html_block",
        "pipe_table_delimiter_row",
        "|",
    ],
    // A fence is unavoidable once a block exists, and front-matter is machine
    // configuration rather than the document's text.
    ignored_nodes: &[
        "fenced_code_block_delimiter",
        "info_string",
        "minus_metadata",
        "plus_metadata",
        "thematic_break",
    ],
    comment_frame: &["<!--", "-->"],
    annotation_line: &[],
    generated_regions: &[("TOC", "/TOC")],
};

pub static PROFILES: &[&Profile] = &[&PHP, &SHELL, &YAML, &MARKDOWN];

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
