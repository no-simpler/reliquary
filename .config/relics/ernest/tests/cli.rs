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
    assert!(text.contains("shell"), "{text}");
    assert!(text.contains("yaml"), "{text}");
}

/// A roll-up is a sum, not one more row of the breakdown, so it leads its group
/// and the rows it sums indent under it.
#[test]
fn a_cohort_rolls_up_above_its_indented_languages() {
    let out = ernest(&[fixtures().to_str().unwrap()]);
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();

    let cohort = lines
        .iter()
        .position(|l| l.starts_with("  source"))
        .unwrap_or_else(|| panic!("no source roll-up in:\n{text}"));
    assert!(lines[cohort + 1].starts_with("    php"), "{text}");
    assert!(lines[cohort + 2].starts_with("    shell"), "{text}");
    assert!(lines[cohort + 3].starts_with("    yaml"), "{text}");
    assert!(
        !text.contains("total"),
        "the roll-up no longer needs a label:\n{text}"
    );
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
    let out = ernest(&[fixtures().to_str().unwrap(), "--json", "--by", "file"]);
    assert_eq!(out.status.code(), Some(0));
    let report: serde_json::Value =
        serde_json::from_slice(&out.stdout).expect("json parses");

    assert_eq!(report["schema_version"], 2);
    assert_eq!(report["tool"], "ernest");
    assert_eq!(report["unit"], "chars");

    // The headline cohort leads, so a documentation format cannot displace it.
    let cohort = &report["cohorts"][0];
    assert_eq!(cohort["cohort"], "source");
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
        .filter(|f| f["cohort"] == "source")
        .map(|f| f["prose_chars"].as_u64().unwrap())
        .sum();
    assert_eq!(cohort["prose_chars"].as_u64().unwrap(), per_file);

    // Docs never fold into the headline, however much prose they carry.
    let docs = report["cohorts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["cohort"] == "docs")
        .expect("the markdown fixture lands in a docs cohort");
    assert!(docs["prose_chars"].as_u64().unwrap() > 0);
}

#[test]
fn diff_reports_the_prose_a_change_removed() {
    let dir = scratch("diff");
    let file = dir.join("sample.php");

    let with_prose = "<?php\n// Explains a thing at length, twice over.\n$x = 1;\n";
    let without = "<?php\n$x = 1;\n";

    std::fs::write(&file, with_prose).unwrap();
    let before = dir.join("before.json");
    let out = ernest(&[dir.to_str().unwrap(), "--json", "--by", "file"]);
    std::fs::write(&before, &out.stdout).unwrap();

    std::fs::write(&file, without).unwrap();
    let after = dir.join("after.json");
    let out = ernest(&[dir.to_str().unwrap(), "--json", "--by", "file"]);
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

/// The two ignore mechanisms mean different things, and the scope levels are
/// where that difference shows: `.gitignore` is committed and holds the noise,
/// `.git/info/exclude` is local and holds what you keep but do not share.
#[test]
fn scope_separates_the_second_brain_from_the_noise() {
    let dir = scratch("scope");
    std::fs::create_dir_all(dir.join(".git/info")).unwrap();
    std::fs::create_dir_all(dir.join("cache")).unwrap();

    std::fs::write(dir.join(".gitignore"), "/cache/\n").unwrap();
    std::fs::write(dir.join(".git/info/exclude"), "NOTES.md\n").unwrap();
    std::fs::write(dir.join("README.md"), "# Shared\n\nShared prose.\n").unwrap();
    std::fs::write(dir.join("NOTES.md"), "# Local\n\nLocal prose.\n").unwrap();
    std::fs::write(dir.join("cache/GENERATED.md"), "# Noise\n\nGenerated prose.\n").unwrap();

    let paths = |args: &[&str]| {
        let mut argv = vec![dir.to_str().unwrap(), "--json", "--by", "file"];
        argv.extend_from_slice(args);
        let out = ernest(&argv);
        assert_eq!(out.status.code(), Some(0));
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        report["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| {
                let path = f["path"].as_str().unwrap();
                let name = path.rsplit('/').next().unwrap().to_string();
                (name, f["provenance"].as_str().unwrap().to_string())
            })
            .collect::<Vec<_>>()
    };

    let shared = paths(&["--scope", "shared"]);
    assert_eq!(shared, vec![("README.md".into(), "tracked".into())]);

    let mut local = paths(&[]);
    local.sort();
    assert_eq!(
        local,
        vec![
            ("NOTES.md".to_string(), "local".to_string()),
            ("README.md".to_string(), "tracked".to_string()),
        ],
        "the default reaches the second brain but not the noise"
    );

    let all = paths(&["--scope", "all"]);
    assert!(
        all.iter().any(|(name, _)| name == "GENERATED.md"),
        "--scope all reaches gitignored files: {all:?}"
    );
}

/// The walk exempts its own root from its ignore rules, so pointing ernest
/// straight at the second brain must not make it read as shared.
#[test]
fn naming_an_excluded_path_does_not_launder_it() {
    let dir = scratch("named-root");
    std::fs::create_dir_all(dir.join(".git/info")).unwrap();
    std::fs::create_dir_all(dir.join("brain")).unwrap();

    std::fs::write(dir.join(".git/info/exclude"), "/brain/\n").unwrap();
    std::fs::write(dir.join("brain/NOTES.md"), "# Local\n\nLocal prose.\n").unwrap();

    let named = dir.join("brain");
    let out = ernest(&[named.to_str().unwrap(), "--json", "--by", "file"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["files"][0]["provenance"], "local");

    let out = ernest(&[named.to_str().unwrap(), "--json", "--scope", "shared"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["files_scanned"], 0,
        "--scope shared must hold even when the root is the excluded path"
    );
}

/// Where both mechanisms match a path, `.gitignore` wins and the file is noise.
#[test]
fn gitignore_beats_a_local_exclude() {
    let dir = scratch("crossover");
    std::fs::create_dir_all(dir.join(".git/info")).unwrap();
    std::fs::create_dir_all(dir.join("notes")).unwrap();

    std::fs::write(dir.join(".git/info/exclude"), "/notes/\n").unwrap();
    std::fs::write(dir.join("notes/.gitignore"), "*\n").unwrap();
    std::fs::write(dir.join("notes/EPHEMERAL.md"), "# Gone\n\nEphemeral prose.\n").unwrap();
    std::fs::write(dir.join("notes/KEPT.md"), "# Kept\n\nDurable prose.\n").unwrap();

    let out = ernest(&[dir.to_str().unwrap(), "--json", "--by", "file"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["files"].as_array().unwrap().len(),
        0,
        "a nested .gitignore inside a locally-excluded directory still wins"
    );
}

#[test]
fn sections_rank_a_documents_headings() {
    let path = fixtures().join("markdown/adversarial.md");
    let out = ernest(&[path.to_str().unwrap(), "--by", "section"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();

    assert!(text.contains("#Adversarial Markdown > Fences"), "{text}");
    assert!(text.contains("#Adversarial Markdown > Tables"), "{text}");
}

/// A personal-bin utility carries no extension, so the shebang is the only
/// thing that names its language.
#[test]
fn an_extensionless_script_is_measured_by_its_shebang() {
    let dir = scratch("shebang");
    std::fs::write(
        dir.join("tool"),
        "#!/usr/bin/env bash\n# Why this exists.\nset -euo pipefail\n",
    )
    .unwrap();
    std::fs::write(dir.join("notes"), "Just text, no shebang.\n").unwrap();

    let out = ernest(&[dir.to_str().unwrap(), "--json", "--by", "file"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    assert_eq!(report["files_scanned"], 1);
    assert_eq!(report["files_skipped"], 1);
    assert_eq!(report["files"][0]["language"], "shell");
    // The shebang is uninteresting; only the comment counts as prose.
    assert_eq!(report["files"][0]["prose_chars"], "#Whythisexists.".len() as u64);
}

#[test]
fn lang_narrows_the_measurement() {
    let out = ernest(&[fixtures().to_str().unwrap(), "--lang", "yaml"]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("yaml"), "{text}");
    assert!(!text.contains("php"), "php should have been excluded:\n{text}");
}
