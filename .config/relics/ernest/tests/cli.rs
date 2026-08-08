//! End-to-end behaviour of the binary: exit codes, snapshot round-trip, and
//! the before/after loop ernest exists for.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn ernest(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ernest"))
        .args(args)
        .output()
        .expect("ernest runs")
}

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ernest-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

#[test]
fn reports_and_exits_clean() {
    let out = ernest(&[fixtures().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("prose density"), "{text}");
    assert!(text.contains("php"), "{text}");
    assert!(text.contains("yaml"), "{text}");
}

#[test]
fn an_unreadable_path_is_an_error_not_a_verdict() {
    let out = ernest(&["/nonexistent-path-for-ernest-tests"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8(out.stderr).unwrap().contains("no such path"));
}

#[test]
fn the_threshold_separates_exceeded_from_broken() {
    let dir = fixtures();
    let dir = dir.to_str().unwrap();

    // The fixtures carry prose, so any density is above zero.
    let over = ernest(&[dir, "--max-density", "0"]);
    assert_eq!(over.status.code(), Some(1));
    assert!(
        String::from_utf8(over.stderr).unwrap().contains("exceeds"),
        "should say why it failed"
    );

    let under = ernest(&[dir, "--max-density", "100"]);
    assert_eq!(under.status.code(), Some(0));
}

#[test]
fn json_carries_a_versioned_schema_that_reconciles() {
    let out = ernest(&[fixtures().to_str().unwrap(), "--json", "--by-file"]);
    assert_eq!(out.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json parses");

    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["tool"], "ernest");
    assert_eq!(report["unit"], "chars");

    let cohort = &report["cohorts"][0];
    let summed: u64 = cohort["languages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["prose_chars"].as_u64().unwrap())
        .sum();
    assert_eq!(cohort["prose_chars"].as_u64().unwrap(), summed);

    let per_file: u64 = report["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["prose_chars"].as_u64().unwrap())
        .sum();
    assert_eq!(cohort["prose_chars"].as_u64().unwrap(), per_file);
}

#[test]
fn diff_reports_the_prose_a_change_removed() {
    let dir = scratch("diff");
    let file = dir.join("sample.php");

    let with_prose = "<?php\n// Explains a thing at length, twice over.\n$x = 1;\n";
    let without = "<?php\n$x = 1;\n";

    std::fs::write(&file, with_prose).unwrap();
    let before = dir.join("before.json");
    let out = ernest(&[dir.to_str().unwrap(), "--json", "--by-file"]);
    std::fs::write(&before, &out.stdout).unwrap();

    std::fs::write(&file, without).unwrap();
    let after = dir.join("after.json");
    let out = ernest(&[dir.to_str().unwrap(), "--json", "--by-file"]);
    std::fs::write(&after, &out.stdout).unwrap();

    let out = ernest(&[
        "diff",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();

    // The comment held 39 non-whitespace characters, all of them prose.
    let removed = with_prose.chars().filter(|c| !c.is_whitespace()).count()
        - without.chars().filter(|c| !c.is_whitespace()).count();
    assert!(
        text.contains(&format!("-{removed}")),
        "expected a -{removed} prose delta in:\n{text}"
    );
    assert!(text.contains("0.0%"), "density should land at zero:\n{text}");
    assert!(text.contains("sample.php"), "should name the file:\n{text}");
}

#[test]
fn a_snapshot_from_another_schema_is_refused() {
    let dir = scratch("schema");
    let stale = dir.join("stale.json");
    std::fs::write(&stale, r#"{"schema_version":99,"tool":"ernest","unit":"chars","files_scanned":0,"files_skipped":0,"files_failed":0,"cohorts":[]}"#).unwrap();

    let out = ernest(&["diff", stale.to_str().unwrap(), stale.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("schema_version")
    );
}

#[test]
fn lang_narrows_the_measurement() {
    let out = ernest(&[fixtures().to_str().unwrap(), "--lang", "yaml"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("yaml"), "{text}");
    assert!(!text.contains("php"), "php should have been excluded:\n{text}");
}
