//! Golden fixtures.
//!
//! Each fixture pairs with a snapshot of its counts. The exact semantics of each
//! rule are pinned by the unit tests in `src/analyze`; these guard the rules
//! working together, and are what an added format must not quietly disturb.
//!
//! Accept a deliberate change with `cargo insta review`, which shows one diff per
//! fixture and takes them one at a time. Read each — an accepted wrong answer is
//! still wrong. `insta::glob!` names snapshots after the fixture path and reports
//! every drifted fixture in one run rather than stopping at the first.

// Clippy's in-test carve-outs (see `clippy.toml`) reach `#[test]` functions and
// `#[cfg(test)]` modules — not the helpers beside them. An integration test crate
// is test code end to end, so the carve-out belongs at its root, where its scope
// is still exactly the tests.
#![allow(
    clippy::expect_used,
    clippy::panic,
    clippy::indexing_slicing,
    clippy::unwrap_used
)]

use std::path::{Path, PathBuf};

use ernest::analyze::analyze_file;
use ernest::analyze::profiles::Profile;
use ernest::detect::profile_for;
use ernest::span::Counts;

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut found = Vec::new();
    for language in std::fs::read_dir(&root).expect("fixtures directory") {
        let language = language
            .expect("readable fixture language directory")
            .path();
        if !language.is_dir() {
            continue;
        }
        for entry in std::fs::read_dir(&language).expect("readable fixture directory") {
            let path = entry.expect("readable fixture").path();
            if path.extension().is_some_and(|e| e == "json") {
                continue;
            }
            found.push(path);
        }
    }
    found.sort();
    assert!(!found.is_empty(), "no fixtures found");
    found
}

fn measure(path: &Path) -> (&'static Profile, String, Counts) {
    let profile = profile_for(path).unwrap_or_else(|| panic!("no profile for {}", path.display()));
    let src = std::fs::read_to_string(path).expect("readable fixture");
    let counts = analyze_file(&src, profile).expect("fixture analyzes");
    (profile, src, counts)
}

#[test]
fn fixtures_match_their_expected_counts() {
    insta::glob!("fixtures/*/*", |path| {
        let (_, _, counts) = measure(path);
        insta::assert_json_snapshot!(counts);
    });
}

/// Every non-whitespace character belongs to exactly one class. This holds
/// whatever the rules decide, so it catches a span bug the expectations would
/// happily absorb.
#[test]
fn every_character_is_bucketed_exactly_once() {
    for path in fixtures() {
        let (_, src, counts) = measure(&path);
        let in_file = ernest::span::tally(src.chars().filter(|c| !c.is_whitespace()).count());
        let bucketed = counts.prose_chars + counts.code_chars + counts.ignored_chars;
        assert_eq!(
            in_file,
            bucketed,
            "{} holds {in_file} non-whitespace characters but buckets hold {bucketed}",
            path.display()
        );
    }
}

/// A fixture the grammar cannot read measures the grammar's confusion rather
/// than the file, and the blessed expectations absorb that silently — an ERROR
/// node produces spans like any other. This is also the check a profile that
/// borrows another dialect's grammar has to pass: CLAUDE.md records that the
/// bash grammar errors on real zsh, and this is what turns that from a
/// judgement call into a verdict.
#[test]
fn every_fixture_parses_without_error_nodes() {
    for path in fixtures() {
        let profile = profile_for(&path).expect("profile");
        let src = std::fs::read_to_string(&path).expect("readable fixture");
        let tree = ernest::analyze::parse(&src, profile).expect("fixture parses");
        assert!(
            !tree.root_node().has_error(),
            "{} parses to an ERROR node under the {} grammar",
            path.display(),
            profile.language,
        );
    }
}

/// Likewise for lines: a line reaches at most one class, and never more lines
/// than the file has.
#[test]
fn no_line_is_counted_twice() {
    for path in fixtures() {
        let (_, src, counts) = measure(&path);
        let in_file = ernest::span::tally(src.lines().count());
        let bucketed = counts.prose_lines + counts.code_lines + counts.ignored_lines;
        assert!(
            bucketed <= in_file,
            "{} has {in_file} lines but buckets hold {bucketed}",
            path.display()
        );
    }
}

#[test]
fn the_adversarial_php_fixture_finds_prose_without_false_positives() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/php/adversarial.php");
    let (_, src, counts) = measure(&path);

    // Everything a regex-based classifier would misread is code, so the code
    // bucket must hold these verbatim strings' content.
    for decoy in ["http://not-a-comment", "-- not a comment either"] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // The shebang, both PHP tags, the closing tag and the phpcs directive.
    assert!(counts.ignored_chars > 0, "found nothing uninteresting");
}

#[test]
fn the_adversarial_rust_fixture_finds_prose_without_false_positives() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rust/adversarial.rs");
    let (_, src, counts) = measure(&path);

    for decoy in [
        "\"http://not-a-comment/#frag\"",
        "r\"C:\\path\\ // still a string\"",
        "r##\"contains \"# inside\"##",
        "b\"bytes // here\"",
        "let slash = '/';",
        "'outer: loop",
        "/* outer /* inner */ still outer */",
        "//// Four slashes",
    ] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // Three directives, and nothing else: the `#![deny(…)]` first line that is
    // not a shebang, the SPDX identifier, and the `#[allow(…)]`.
    assert_eq!(counts.ignored_chars, 70);
    assert_eq!(counts.ignored_lines, 3);
}

#[test]
fn the_adversarial_toml_fixture_finds_prose_without_false_positives() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/toml/adversarial.toml");
    let (_, src, counts) = measure(&path);

    for decoy in [
        "\"https://example.com/page#section\"",
        "'C:\\path\\#raw'",
        "\"quoted#key\"",
        "\"#ff0000\"",
        "\"b#c\"",
    ] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // The `#:schema` directive, and nothing else.
    assert_eq!(counts.ignored_chars, 47);
    assert_eq!(counts.ignored_lines, 1);
}

#[test]
fn the_adversarial_javascript_fixture_finds_prose_without_false_positives() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/javascript/adversarial.js");
    let (_, src, counts) = measure(&path);

    for decoy in [
        "\"http://not-a-comment/#frag\"",
        "\"/* not a comment */\"",
        "`${id} // not a comment`",
        "/https?:\\/\\/example\\.com\\/[*]/",
        "/[/*]+/g",
        "id / 2 / 3",
        "this.#count",
    ] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // The shebang, the SPDX identifier, `@ts-check`, and the three linter and
    // coverage directives.
    assert_eq!(counts.ignored_lines, 6);
}

/// JSX text is the interface's copy rather than prose about the code, so it
/// bills as code — and a comment inside the markup still bills as prose.
///
/// Asserted by shortening the copy rather than by a fixed number: what has to
/// hold is that the characters move the *code* bucket and leave prose alone,
/// and that survives every later edit to the fixture.
#[test]
fn the_adversarial_jsx_fixture_bills_markup_copy_as_code() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/javascript/adversarial.jsx");
    let (profile, src, counts) = measure(&path);

    let copy = "<h1>Reticulating splines</h1>";
    assert!(src.contains(copy), "fixture lost its markup copy");
    assert!(counts.prose_chars > 0, "the JSX comment is prose");

    let shortened = src.replace(copy, "<h1>x</h1>");
    let after = analyze_file(&shortened, profile).expect("analyzes");
    assert_eq!(
        counts.prose_chars, after.prose_chars,
        "interface copy reached the prose bucket"
    );
    assert_eq!(
        counts.code_chars - after.code_chars,
        ernest::span::tally("ReticulatingSplines".len() - 1),
        "interface copy should bill as code, character for character"
    );
}

#[test]
fn the_adversarial_typescript_fixture_finds_prose_without_false_positives() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/typescript/adversarial.ts");
    let (_, src, counts) = measure(&path);

    for decoy in [
        "\"http://not-a-comment/#frag\"",
        "`${id} // not a comment`",
        "/\\/\\*[^*]*\\*\\//g",
        "id / 2 / 3",
        "<string>(<unknown>await readFile(url, \"utf8\"))",
        "`tenant-${string}`",
    ] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // The triple-slash reference, the SPDX identifier and `@ts-expect-error`.
    assert_eq!(counts.ignored_lines, 3);
}

#[test]
fn the_adversarial_css_fixture_finds_prose_without_false_positives() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/css/adversarial.css");
    let (_, src, counts) = measure(&path);

    for decoy in [
        "\"/* not a comment */\"",
        "url(http://not-a-comment/#frag)",
        "quotes: \"//\" \"*/\"",
        "calc(100% / 3)",
    ] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // The SPDX identifier, the stylelint directive and the prettier one.
    assert_eq!(counts.ignored_lines, 3);
}

/// Body copy is the interface's own text rather than prose describing code, so
/// it bills as code — and a comment beside it still bills as prose.
///
/// Asserted by shortening the copy rather than by a fixed number, as the JSX
/// fixture is: what has to hold is that the characters move the *code* bucket
/// and leave prose alone, and that survives every later edit to the fixture.
#[test]
fn the_adversarial_html_fixture_bills_markup_copy_as_code() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/html/adversarial.html");
    let (profile, src, counts) = measure(&path);

    for decoy in [
        "&lt;!-- an escaped comment, which is text --&gt;",
        "https://example.com/?a=1&amp;b=2#frag",
        "*ngIf=\"ready\" [value]=\"row.total\" (click)=\"open()\" #anchor",
    ] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // The doctype, the SPDX identifier and the prettier directive.
    assert_eq!(counts.ignored_lines, 3);

    let copy = "<p>Interface copy is the product, not prose about code.</p>";
    assert!(src.contains(copy), "fixture lost its markup copy");
    let shortened = src.replace(copy, "<p>x</p>");
    let after = analyze_file(&shortened, profile).expect("analyzes");
    assert_eq!(
        counts.prose_chars, after.prose_chars,
        "interface copy reached the prose bucket"
    );
    assert_eq!(
        counts.code_chars - after.code_chars,
        ernest::span::tally("Interfacecopyistheproduct,notproseaboutcode.".len() - 1),
        "interface copy should bill as code, character for character"
    );
}

/// The grammar is template-first, so the markup between the delimiters is one
/// opaque `text` node. The comment in it bills as code, and this is where that
/// is pinned against the real file rather than a two-line snippet.
#[test]
fn the_adversarial_twig_fixture_finds_prose_without_false_positives() {
    let path =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/twig/adversarial.html.twig");
    let (profile, src, counts) = measure(&path);

    for decoy in [
        "'a string with #} inside it'",
        "\"and a {# one in the other quote\"",
        "{{ this is not parsed }}",
    ] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // The SPDX identifier and the twig-cs-fixer directive.
    assert_eq!(counts.ignored_lines, 2);

    let markup = "The injection gap, not a rule this profile could fix.";
    assert!(src.contains(markup), "fixture lost its HTML comment");
    let shortened = src.replace(markup, "x");
    let after = analyze_file(&shortened, profile).expect("analyzes");
    assert_eq!(
        counts.prose_chars, after.prose_chars,
        "an HTML comment inside a template reached the prose bucket"
    );
    assert!(
        counts.code_chars > after.code_chars,
        "an HTML comment inside a template should bill as code"
    );
}

#[test]
fn the_adversarial_xml_fixture_finds_prose_without_false_positives() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/xml/adversarial.xml");
    let (_, src, counts) = measure(&path);

    for decoy in [
        "attr=\"&lt;!-- not a comment --&gt;\"",
        "<!-- not a comment either -->",
        "WHERE tag <> 'x' AND note LIKE '%--%'",
    ] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // The XML declaration, the doctype and the SPDX identifier.
    assert_eq!(counts.ignored_lines, 3);
}

#[test]
fn the_adversarial_shell_fixture_finds_prose_without_false_positives() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/shell/adversarial.sh");
    let (_, src, counts) = measure(&path);

    for decoy in [
        "${PWD##*/}",
        "${#REPOS[@]}",
        "'#'*|''",
        "/^[[:space:]]*#/",
        "printf '#!/usr/bin/env bash",
    ] {
        assert!(src.contains(decoy), "fixture lost its decoy: {decoy}");
    }
    assert!(counts.prose_chars > 0, "found no prose at all");
    // The shebang and the shellcheck directive, and nothing else.
    assert_eq!(counts.ignored_chars, 43);
}
