//! The format registry.
//!
//! Adding a language is adding one `Profile` constant here and listing it in
//! `PROFILES`. Write a bespoke `Analyzer` only when a profile cannot express
//! the format.

use tree_sitter_language::LanguageFn;

use crate::span::Class;

/// How a language's contribution is broken out. Prose-by-nature formats such as
/// Markdown belong to `Docs`, whose own density says little — near 100% in any
/// real project. Its prose still counts toward the headline, which sums both:
/// prose moved from a comment into a document has not gone anywhere.
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
    /// Bodies opening with one of these are this language's tooling directives:
    /// machine-consumed, and unavoidable once you want the tooling. The whole
    /// node becomes uninteresting.
    ///
    /// A language's directives belong to its language. Held flat and global,
    /// Rust attribute syntax only Rust can produce would be matched against a
    /// YAML comment, and the list only grows — every added format brings its
    /// linters with it. `UNIVERSAL_PRAGMA_PREFIXES` carries the few that really
    /// are everyone's.
    pub pragma_prefixes: &'static [&'static str],
}

/// The directives that belong to no single language, because the tool consuming
/// them reads every file regardless of format: a licence identifier, an editor
/// modeline, an editorconfig-checker directive. Tested for every profile,
/// alongside whatever that profile declares for itself.
///
/// Keep this list closed. Anything a particular language's toolchain produces
/// goes on that language's profile, where it cannot reach a comment in another.
pub const UNIVERSAL_PRAGMA_PREFIXES: &[&str] =
    &["SPDX-License-Identifier:", "vim:", "editorconfig-checker-"];

/// Two comment kinds, both reachable anywhere: `comment` is `/* */` and
/// `js_comment` is the `//` form postcss and the preprocessors accept. Naming
/// the second costs one word against a false negative, which is the trade
/// `WEB_PROSE_NODES` already makes for JavaScript's Annex B comment.
///
/// Nothing is uninteresting by node kind. `@charset` reads like a header but is
/// optional, so it is code — the line TOML records for `[section]`: there is no
/// counterpart to `<?php` here.
///
/// Do not widen `extensions` to the preprocessors. The grammar reads standard
/// nesting, but `$var`, `@mixin` and `@include` error, and both scss crates
/// still predate `LanguageFn` — see `TODO.md`.
pub static CSS: Profile = Profile {
    language: "css",
    language_fn: tree_sitter_css::LANGUAGE,
    extensions: &["css"],
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["comment", "js_comment"],
    code_nodes: &[],
    ignored_nodes: &[],
    // Longest first, as in `RUST`.
    comment_frame: &["/**", "/*", "*/", "*", "//"],
    annotation_line: &[],
    generated_regions: &[],
    // Deliberately absent: `!`, for the `/*! preserved banner */` convention.
    // A one-character prefix is the objection `WEB_PRAGMA_PREFIXES` records
    // against eslint's `/* global */`, and a licence block is the queued
    // header-detection item's to claim rather than a prefix rule's.
    pragma_prefixes: &[
        "stylelint-",
        "prettier-ignore",
        "biome-ignore",
        "autoprefixer:",
    ],
};

/// `Source`, not `Docs`, which answers the question the format roadmap left
/// open. An `.html` file is a template or a page — the product rather than a
/// description of it — and `Docs` is for formats whose `default_class` inverts
/// to `Prose`. This one's does not.
///
/// `text` is named by no rule and so falls to `Code`, which is the wanted
/// answer for exactly the reason `jsx_text` gets it: interface copy is the
/// product. A `comment` is an `extra` here, so it is reached wherever it sits.
///
/// `doctype` is the one unavoidable construct — `<?php` in another costume.
/// `<html>`, `<head>` and `<body>` are all optional, and naming them starts a
/// slide with no floor.
pub static HTML: Profile = Profile {
    language: "html",
    language_fn: tree_sitter_html::LANGUAGE,
    extensions: &["html", "htm"],
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["comment"],
    code_nodes: &[],
    ignored_nodes: &["doctype"],
    comment_frame: &["<!--", "-->"],
    annotation_line: &[],
    generated_regions: &[],
    pragma_prefixes: &["prettier-ignore", "htmlhint ", "biome-ignore"],
};

pub static PHP: Profile = Profile {
    language: "php",
    language_fn: tree_sitter_php::LANGUAGE_PHP,
    extensions: &["php", "phtml", "php4", "php5", "php7", "phps", "phpstub"],
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
    // The static analysers, in both the vehicles PHP gives them: a plain
    // comment and an annotation inside a docblock.
    pragma_prefixes: &[
        "phpcs:",
        "phpstan-",
        "psalm-",
        "@phpcs",
        "@phpstan-",
        "@psalm-",
    ],
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
    pragma_prefixes: &["yaml-language-server:", "yamllint ", "prettier-ignore"],
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
    pragma_prefixes: &["shellcheck "],
};

/// There is no plain `comment` kind here. The grammar defines one, but `extras`
/// names `line_comment` and `block_comment` directly and the rule is
/// unreachable, so those two are what a walk ever sees — and between them they
/// carry `//`, `///`, `//!`, `////`, `/* */`, `/** */` and `/*! */` alike.
///
/// A doc comment's markers are *children* of those kinds, so naming the parents
/// counts the comment whole, sigils included, exactly as a PHP docblock is
/// counted. Splitting `///` from `//` would mean naming the children instead,
/// which loses `//` entirely — it has none.
///
/// Nothing is uninteresting by node kind: Rust has no counterpart to `<?php`,
/// and its `shebang` is handled for every language in `analyze::classify`.
/// The attribute kinds are named as code, which is what they already were by
/// default — listing them is what lets the pragma rule see a `#[allow(…)]`.
pub static RUST: Profile = Profile {
    language: "rust",
    language_fn: tree_sitter_rust::LANGUAGE,
    extensions: &["rs"],
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["line_comment", "block_comment"],
    code_nodes: &["attribute_item", "inner_attribute_item"],
    ignored_nodes: &[],
    // Longest first: `comment_body` strips the first sigil that matches, so
    // `//` ahead of `///` would leave a slash behind. `#` must stay out of it,
    // or `#[allow(` would strip to `[allow(` and match no pragma prefix.
    comment_frame: &["///", "//!", "//", "/**", "/*!", "/*", "*/", "*"],
    annotation_line: &[],
    generated_regions: &[],
    // Lint and format attributes — syntax rather than comments, which is why
    // the attribute kinds are named as `code_nodes` above: a directive is the
    // same directive whichever vehicle a language gives it.
    //
    // `cfg_attr` is deliberately absent: `#![cfg_attr(test, deny(warnings))]`
    // is a lint directive and `#![cfg_attr(docsrs, feature(doc_cfg))]` is a
    // feature gate, and a prefix cannot tell them apart — so both stay code.
    pragma_prefixes: &[
        "#[allow(",
        "#![allow(",
        "#[expect(",
        "#![expect(",
        "#[deny(",
        "#![deny(",
        "#[warn(",
        "#![warn(",
        "#[forbid(",
        "#![forbid(",
        "#[rustfmt::skip",
        "#![rustfmt::skip",
    ],
};

/// One `comment` kind, reachable anywhere the grammar allows an extra —
/// including between a table header and its first pair, and inside an array.
///
/// Nothing is uninteresting here. The bracket tokens would work mechanically,
/// but `[section]` scales with the tables you chose to write, which puts it on
/// the code side of the same line braces fall on; YAML's `---` frames a
/// document once and does not.
///
/// Do not reach for `table` or `table_array_element` to name a header: those
/// nodes span the whole section, `pair` children included, and the walk stops
/// at the outermost match.
pub static TOML: Profile = Profile {
    language: "toml",
    language_fn: tree_sitter_toml_ng::LANGUAGE,
    extensions: &["toml"],
    // No `Cargo.lock`. It is TOML, and it is also generated, so leaving it on
    // its own extension keeps it out of the measurement.
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["comment"],
    code_nodes: &[],
    ignored_nodes: &[],
    comment_frame: &["#"],
    annotation_line: &[],
    generated_regions: &[],
    pragma_prefixes: &[":schema"],
};

/// The grammar parses the union of the Jinja family — Jinja2, Nunjucks, Twig,
/// Tera, Django — so a second constant would buy `.j2` and `.njk` for the price
/// of one word. There is no such file to verify against here, so only `.twig`
/// is claimed; the rest is queued in `TODO.md`.
///
/// `.html.twig` needs no rule of its own: `Path::extension` returns the last
/// component, which is what a Symfony template's name makes `twig`.
///
/// **The markup is opaque.** This is a template-first grammar: everything
/// between the delimiters arrives as one `text` node, which falls to `Code`.
/// That is the right answer for the denominator and the wrong one for an
/// `<!-- -->` comment sitting in it, which bills as code — the injection gap,
/// recorded under Known imprecisions in `CLAUDE.md`.
///
/// Nothing is uninteresting by node kind. A `{% raw %}` region is content the
/// author wrote and chose not to render, not something a tool rewrites.
pub static TWIG: Profile = Profile {
    language: "twig",
    language_fn: tree_sitter_jinja_dialects::LANGUAGE,
    extensions: &["twig"],
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    // The node spans `{#` through `#}`, so the delimiters count as prose
    // exactly as a PHP docblock's do.
    prose_nodes: &["comment"],
    code_nodes: &[],
    ignored_nodes: &[],
    // The whitespace-control variants lead the bare form: `{#` matching first
    // would strip to `-`, and the body would never reach a prefix test.
    comment_frame: &["{#-", "{#~", "{#", "-#}", "~#}", "#}"],
    // `{# @var post \App\Entity\Post #}` is PHPDoc's convention in another
    // vehicle — avoidable, machine-consumed, and not prose.
    annotation_line: &["@"],
    generated_regions: &[],
    // Covers -disable, -disable-next-line and -enable.
    pragma_prefixes: &["twig-cs-fixer-"],
};

/// Shared by the three ECMAScript profiles, which differ only in the grammar
/// they load. Longest first, as in `RUST`: `//` ahead of `///` would leave a
/// slash behind on a triple-slash directive.
///
/// `#` must stay out of it. JavaScript has no `#` comment — what it has is a
/// private class field, and stripping `#` would read `this.#count` as a body.
const WEB_COMMENT_FRAME: &[&str] = &["///", "/**", "//", "/*", "*/", "*"];
const WEB_PROSE_NODES: &[&str] = &["comment", "html_comment"];

/// The toolchain directives every ECMAScript dialect shares, since the linters,
/// formatters and bundlers do not distinguish between them either.
///
/// Deliberately absent: eslint's `/* global foo */` and `/* globals foo */`. A
/// prefix that short would swallow any comment opening with the English word.
const WEB_PRAGMA_PREFIXES: &[&str] = &[
    // Covers -disable, -disable-next-line, -disable-line, -enable and -env.
    "eslint-",
    // Covers -ignore, -expect-error, -nocheck and -check. Reached before
    // `annotation_line` sees the leading `@`, so it lands as a directive
    // rather than as an annotation.
    "@ts-",
    // A triple-slash compiler directive, once the frame has stripped the `///`.
    "<reference",
    "prettier-ignore",
    "biome-ignore",
    "tslint:",
    "istanbul ignore",
    "c8 ignore",
    "v8 ignore",
    "webpackChunkName:",
    "@vite-ignore",
];

/// One `comment` kind carries `//`, `/* */` and `/** */` alike, so a `JSDoc` block
/// is counted whole exactly as a PHP docblock is. `html_comment` is the Annex B
/// `<!--` line comment — rare, but genuinely a comment, and naming it costs one
/// word against a false negative.
///
/// `JSDoc`'s `@param` is `PHPDoc`'s convention verbatim, so `annotation_line`
/// transfers unchanged. Decorators are untouched by it: they are syntax rather
/// than comment bodies, and the rule only runs inside a prose node.
///
/// Nothing is uninteresting by node kind — there is no counterpart to `<?php`,
/// and the `hash_bang_line` a `#!/usr/bin/env node` produces is handled for
/// every language in `analyze::classify`.
pub static JAVASCRIPT: Profile = Profile {
    language: "javascript",
    language_fn: tree_sitter_javascript::LANGUAGE,
    // JSX needs no second grammar here: this one carries it natively, which is
    // what separates JavaScript from TypeScript below.
    extensions: &["js", "mjs", "cjs", "jsx"],
    filenames: &[],
    interpreters: &["node", "bun"],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: WEB_PROSE_NODES,
    code_nodes: &[],
    ignored_nodes: &[],
    comment_frame: WEB_COMMENT_FRAME,
    annotation_line: &["@"],
    generated_regions: &[],
    pragma_prefixes: WEB_PRAGMA_PREFIXES,
};

/// TypeScript without JSX. The grammar rejects it deliberately, because `<T>x`
/// is a type assertion here — which is why tree-sitter ships two, and why `TSX`
/// below exists at all.
pub static TYPESCRIPT: Profile = Profile {
    language: "typescript",
    language_fn: tree_sitter_typescript::LANGUAGE_TYPESCRIPT,
    extensions: &["ts", "mts", "cts"],
    filenames: &[],
    // None, and that is a rule rather than an omission: `ts-node` and `tsx`
    // pick their loader from the file extension, so an extensionless
    // TypeScript file is not a thing that runs.
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: WEB_PROSE_NODES,
    code_nodes: &[],
    ignored_nodes: &[],
    comment_frame: WEB_COMMENT_FRAME,
    annotation_line: &["@"],
    generated_regions: &[],
    pragma_prefixes: WEB_PRAGMA_PREFIXES,
};

/// The same language as `TYPESCRIPT` and it reports as such — `language` is
/// deliberately shared, because the split is an artifact of needing a second
/// grammar and not a distinction the table should carry. `aggregate.rs` keys
/// its rows on `language`, so the two fold into one row.
///
/// `jsx_text` — the `Hello world` in `<p>Hello world</p>` — is named by no rule
/// and so falls to `default_class`, which is `Code`. That is the wanted answer.
/// Interface copy is the product, not prose describing code: it is not
/// re-derivable on demand, and it sits on the same side of the line as any
/// other string literal. A comment *inside* JSX arrives as an ordinary
/// `comment` under a `jsx_expression`, and is prose like any other.
pub static TSX: Profile = Profile {
    language: "typescript",
    language_fn: tree_sitter_typescript::LANGUAGE_TSX,
    extensions: &["tsx"],
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: WEB_PROSE_NODES,
    code_nodes: &[],
    ignored_nodes: &[],
    comment_frame: WEB_COMMENT_FRAME,
    annotation_line: &["@"],
    generated_regions: &[],
    pragma_prefixes: WEB_PRAGMA_PREFIXES,
};

/// The crate ships two grammars; `LANGUAGE_XML` is the document one, and
/// `LANGUAGE_DTD` parses a standalone DTD, which is not a file that turns up
/// here. Node kinds come straight from the spec's own names, so they are
/// capitalised: `Comment`, not `comment`.
///
/// `XMLDecl` and `doctypedecl` are unavoidable given the file exists, which is
/// `<?php` and `<!DOCTYPE html>` in a third costume. A processing instruction
/// such as `<?xml-stylesheet ?>` is not: it scales with what you asked the
/// document to do, so it is code.
///
/// `pragma_prefixes` is empty on purpose rather than by omission. XML's
/// consumers are schema validators that read attributes, not comments, so
/// `UNIVERSAL_PRAGMA_PREFIXES` is all this format has any use for.
///
/// Two extensions are deliberately unclaimed, for the reason `TOML` leaves
/// `Cargo.lock` alone. `.svg` is exported artwork — 828 files across
/// `~/Developer`, whose path data nobody authored — and folding it in would
/// bury a repository's stylesheets under its icon set. `.plist` is marked
/// binary by this repository's own `git/attributes`.
pub static XML: Profile = Profile {
    language: "xml",
    language_fn: tree_sitter_xml::LANGUAGE_XML,
    extensions: &["xml", "xsd", "xsl", "xslt"],
    filenames: &[],
    interpreters: &[],
    cohort: Cohort::Source,
    default_class: Class::Code,
    prose_nodes: &["Comment"],
    code_nodes: &[],
    ignored_nodes: &["XMLDecl", "doctypedecl"],
    comment_frame: &["<!--", "-->"],
    annotation_line: &[],
    generated_regions: &[],
    pragma_prefixes: &[],
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
    // The prose linters, plus the region markers themselves: an opener that
    // finds no closer still has to read as a directive rather than as prose.
    pragma_prefixes: &[
        "TOC",
        "/TOC",
        "rumdl-disable",
        "rumdl-enable",
        "markdownlint-disable",
        "markdownlint-enable",
        "vale off",
        "vale on",
        "prettier-ignore",
    ],
};

/// Every profile, in the order a lookup by name resolves them — which is why
/// `TYPESCRIPT` leads `TSX` despite sharing a `language`: a search for the name
/// should land on the dialect that does not need the JSX grammar. Lookups by
/// extension are unambiguous either way.
pub static PROFILES: &[&Profile] = &[
    &CSS,
    &HTML,
    &JAVASCRIPT,
    &PHP,
    &RUST,
    &SHELL,
    &TOML,
    &TWIG,
    &TYPESCRIPT,
    &TSX,
    &XML,
    &YAML,
    &MARKDOWN,
];

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
