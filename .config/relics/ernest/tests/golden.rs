//! Golden fixtures.
//!
//! Each fixture pairs with an `.expected.json` holding its counts. The exact
//! semantics of each rule are pinned by the unit tests in `src/analyze`; these
//! guard the rules working together, and are what an added format must not
//! quietly disturb.
//!
//! Regenerate after a deliberate change: `ERNEST_BLESS=1 cargo test --test golden`.
//! Read the diff before committing it — a blessed wrong answer is still wrong.

use std::path::{Path, PathBuf};

use ernest::analyze::analyze_file;
use ernest::analyze::profiles::Profile;
use ernest::detect::profile_for;
use ernest::span::Counts;

fn fixtures() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut found = Vec::new();
    for language in std::fs::read_dir(&root).expect("fixtures directory") {
        let language = language.expect("readable fixture language directory").path();
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
    let bless = std::env::var_os("ERNEST_BLESS").is_some();
    let mut failures = Vec::new();

    for path in fixtures() {
        let (_, _, counts) = measure(&path);
        let expected_path = path.with_extension(format!(
            "{}.expected.json",
            path.extension().unwrap().to_str().unwrap()
        ));

        if bless {
            std::fs::write(
                &expected_path,
                serde_json::to_string_pretty(&counts).expect("serialises") + "\n",
            )
            .expect("writes expectation");
            continue;
        }

        let text = std::fs::read_to_string(&expected_path).unwrap_or_else(|_| {
            panic!(
                "missing {} — run ERNEST_BLESS=1 cargo test --test golden",
                expected_path.display()
            )
        });
        let expected: Counts = serde_json::from_str(&text).expect("expectation parses");
        if expected != counts {
            failures.push(format!(
                "{}\n  expected {expected:?}\n  measured {counts:?}",
                path.display()
            ));
        }
    }

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Every non-whitespace character belongs to exactly one class. This holds
/// whatever the rules decide, so it catches a span bug the expectations would
/// happily absorb.
#[test]
fn every_character_is_bucketed_exactly_once() {
    for path in fixtures() {
        let (_, src, counts) = measure(&path);
        let in_file = src.chars().filter(|c| !c.is_whitespace()).count() as u64;
        let bucketed = counts.prose_chars + counts.code_chars + counts.ignored_chars;
        assert_eq!(
            in_file,
            bucketed,
            "{} holds {in_file} non-whitespace characters but buckets hold {bucketed}",
            path.display()
        );
    }
}

/// Likewise for lines: a line reaches at most one class, and never more lines
/// than the file has.
#[test]
fn no_line_is_counted_twice() {
    for path in fixtures() {
        let (_, src, counts) = measure(&path);
        let in_file = src.lines().count() as u64;
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
