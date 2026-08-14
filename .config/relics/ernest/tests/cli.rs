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

/// The languages the fixture tree covers, taken from its directory names — so a
/// test asserting the breakdown covers a new format the day its fixture lands,
/// rather than the day someone remembers to widen a literal list.
fn fixture_languages() -> Vec<String> {
    let mut found: Vec<String> = std::fs::read_dir(fixtures())
        .expect("fixtures directory")
        .map(|entry| entry.expect("readable entry").path())
        .filter(|path| path.is_dir())
        .map(|path| path.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    found.sort();
    assert!(!found.is_empty(), "no fixtures found");
    found
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ernest-test-{name}"));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("scratch directory");
    dir
}

/// A bare run is the figure and the caveats on it. No breakdown: a
/// repository-wide ranking is stationary — it reads the same before and after
/// the change being measured — so every one of them is asked for.
#[test]
fn reports_and_exits_clean() {
    let out = ernest(&[fixtures().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.starts_with("prose density"), "{text}");
    assert!(
        !text.contains("total / cohort"),
        "a bare run carries no breakdown:\n{text}"
    );
    // Nothing was asked for, so the reader is told what could be.
    assert!(text.contains("--by file"), "{text}");
}

/// Every fixture language reaches the report, derived from the fixture
/// directories rather than a literal list — so a new format is covered the day
/// its fixture lands rather than the day someone widens a list.
#[test]
fn every_fixture_language_reaches_the_breakdown() {
    let out = ernest(&[fixtures().to_str().unwrap(), "--by", "language"]);
    let text = String::from_utf8(out.stdout).unwrap();
    for language in fixture_languages() {
        assert!(
            text.contains(&language),
            "{language} is missing from:\n{text}"
        );
    }
}

/// A roll-up is a sum, not one more row of the breakdown, so it leads its group
/// and the rows it sums indent under it — total over cohorts over languages, so
/// the table visibly adds up to the headline.
///
/// Asserted as shape rather than as a fixed list of rows: the depth of a row is
/// what carries the meaning, and a test that named today's languages positionally
/// would break on every format added after it.
#[test]
fn the_table_rolls_up_from_languages_through_cohorts_to_the_total() {
    let out = ernest(&[fixtures().to_str().unwrap(), "--by", "language"]);
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();

    // The heading names the hierarchy, so the roll-up is the row after it.
    let start = 1 + lines
        .iter()
        .position(|l| l.contains("total / cohort / language"))
        .unwrap_or_else(|| panic!("no breakdown heading in:\n{text}"));
    // The blank line that ends the block, not the indent — the notes under it
    // are indented too, and only the blank line separates the two.
    let rows: Vec<(usize, &str)> = lines[start..]
        .iter()
        .take_while(|l| !l.trim().is_empty())
        .map(|l| {
            let label = l.trim_start();
            (
                (l.len() - label.len()) / 2,
                label.split(' ').next().unwrap(),
            )
        })
        .collect();

    assert_eq!(rows[0], (1, "total"), "{text}");

    // One cohort per group, source ahead of docs, and every language nested one
    // level under the cohort that sums it.
    let mut cohorts = Vec::new();
    let mut languages = Vec::new();
    for (depth, label) in &rows[1..] {
        match depth {
            2 => {
                cohorts.push(*label);
                languages.push(Vec::new());
            }
            3 => languages
                .last_mut()
                .unwrap_or_else(|| panic!("a language row before any cohort:\n{text}"))
                .push(*label),
            other => panic!("row {label} sits at depth {other}:\n{text}"),
        }
    }
    assert_eq!(cohorts, ["source", "docs"], "{text}");
    for nested in &languages {
        let mut sorted = nested.clone();
        sorted.sort();
        assert_eq!(nested, &sorted, "languages are not in order:\n{text}");
    }

    let mut listed: Vec<String> = languages.concat().iter().map(|l| l.to_string()).collect();
    listed.sort();
    assert_eq!(listed, fixture_languages(), "{text}");
}

#[test]
fn an_unreadable_path_is_an_error_not_a_verdict() {
    let out = ernest(&["/nonexistent-path-for-ernest-tests"]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        String::from_utf8(out.stderr)
            .unwrap()
            .contains("no such path")
    );
}

/// `ernest --by file --top 0 | head` is the obvious way to skim a long ranking,
/// and the print macros made it a panic. A real pipeline is the faithful test:
/// dropping a piped handle from inside the harness closes the reader at a moment
/// the OS chooses, which is exactly the flake this is guarding against.
#[test]
fn a_severed_pipe_is_not_a_failure() {
    let out = Command::new("sh")
        .arg("-c")
        .arg(r#""$0" "$1" --by file --top 0 | head -1"#)
        .arg(env!("CARGO_BIN_EXE_ernest"))
        .arg(fixtures())
        .output()
        .expect("the pipeline runs");

    assert_eq!(out.status.code(), Some(0));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(!stderr.contains("panicked"), "{stderr}");
}

/// A name no profile carries used to measure nothing and exit 0, so a typo was
/// indistinguishable from a repository with no prose in it.
#[test]
fn an_unknown_language_is_a_usage_error() {
    let out = ernest(&[fixtures().to_str().unwrap(), "--lang", "nope"]);
    assert_eq!(out.status.code(), Some(2));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(stderr.contains("markdown"), "should list what it does take");
}

/// Every shell clap_complete knows, rather than the ones this machine happens to
/// run — the enum is taken whole so a shell added upstream arrives for free, and
/// this is what would notice if one stopped generating.
#[test]
fn completions_are_written_for_every_shell() {
    for shell in ["bash", "elvish", "fish", "powershell", "zsh"] {
        let out = ernest(&["completions", shell]);
        assert_eq!(out.status.code(), Some(0), "{shell}");
        let script = String::from_utf8(out.stdout).unwrap();
        assert!(script.contains("ernest"), "{shell}: {script}");
    }
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
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).expect("json parses");

    assert_eq!(report["schema_version"], 3);
    assert_eq!(report["tool"], "ernest");
    assert_eq!(report["unit"], "chars");

    // Source leads the breakdown, so a documentation format cannot displace it.
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

    // Docs carry prose and it reaches the headline: the total is every cohort
    // summed, so no cohort's prose can go missing from it.
    let docs = report["cohorts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["cohort"] == "docs")
        .expect("the markdown fixture lands in a docs cohort");
    assert!(docs["prose_chars"].as_u64().unwrap() > 0);

    let across: u64 = report["cohorts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["prose_chars"].as_u64().unwrap())
        .sum();
    assert_eq!(report["total"]["prose_chars"].as_u64().unwrap(), across);
    assert!(
        report["total"]["prose_chars"].as_u64().unwrap() > cohort["prose_chars"].as_u64().unwrap(),
        "docs prose must reach the total, not stop at the source cohort"
    );
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
        "--by",
        "file",
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
    assert!(
        text.contains("0.0%"),
        "density should land at zero:\n{text}"
    );
    assert!(text.contains("sample.php"), "should name the file:\n{text}");
}

/// The property the headline exists to have. Prose moved out of a comment and
/// into a document has not been de-prosed, so the number must not reward it —
/// while the cohort rows still show that it moved.
#[test]
fn relocating_prose_into_a_document_does_not_move_the_headline() {
    let dir = scratch("relocation");
    let code = dir.join("sample.php");
    let doc = dir.join("guide.md");

    // A block comment, so the whole body carries one delimiter pair however
    // long it grows — `/*` and `*/`, four characters, the only prose the move
    // can destroy. A line comment per line would lose `//` twelve times over.
    let prose: String = (1..=12)
        .map(|n| format!("Paragraph {n} explains at length why the widget reticulates.\n"))
        .collect();
    let statements: String = (1..=12)
        .map(|n| format!("$widget{n} = reticulate($spline{n}, $tolerance{n});\n"))
        .collect();

    std::fs::write(&code, format!("<?php\n/*\n{prose}*/\n{statements}")).unwrap();
    std::fs::write(&doc, "# Guide\n").unwrap();

    let before = dir.join("before.json");
    let out = ernest(&[dir.to_str().unwrap(), "--json"]);
    std::fs::write(&before, &out.stdout).unwrap();
    let before_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    // Move it: out of the comment, into the document. Nothing is deleted.
    std::fs::write(&code, format!("<?php\n{statements}")).unwrap();
    std::fs::write(&doc, format!("# Guide\n\n{prose}")).unwrap();

    let after = dir.join("after.json");
    let out = ernest(&[dir.to_str().unwrap(), "--json"]);
    std::fs::write(&after, &out.stdout).unwrap();
    let after_json: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();

    let density = |v: &serde_json::Value| v["density"].as_f64().expect("a density");
    let source = |v: &serde_json::Value| {
        v["cohorts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["cohort"] == "source")
            .map(density)
            .expect("a source cohort")
    };

    // Pin the delimiter story exactly, so the tolerance below is a known
    // quantity rather than a shrug: the move destroyed `/*` and `*/`, and
    // nothing else.
    let prose_before = before_json["total"]["prose_chars"].as_u64().unwrap();
    let prose_after = after_json["total"]["prose_chars"].as_u64().unwrap();
    assert_eq!(
        prose_before - prose_after,
        4,
        "only the block delimiters should have been lost"
    );
    assert_eq!(
        before_json["total"]["code_chars"], after_json["total"]["code_chars"],
        "no code was touched"
    );

    let (total_before, total_after) = (
        density(&before_json["total"]),
        density(&after_json["total"]),
    );
    assert!(
        (total_after - total_before).abs() < 0.005,
        "headline moved on a pure relocation: {total_before} -> {total_after}"
    );
    // The old headline was the source cohort alone — this is the drop it used
    // to report as an improvement.
    assert!(
        source(&after_json) < source(&before_json) - 0.4,
        "the source cohort should still show the prose leaving it: {} -> {}",
        source(&before_json),
        source(&after_json)
    );

    // And the diff says so out loud: a headline standing still above two cohort
    // rows moving hard in opposite directions.
    let out = ernest(&[
        "diff",
        before.to_str().unwrap(),
        after.to_str().unwrap(),
        "--by",
        "cohort",
    ]);
    let text = String::from_utf8(out.stdout).unwrap();
    let headline = text.lines().next().unwrap();
    let pp: f64 = headline
        .split_once('(')
        .and_then(|(_, rest)| rest.split_once(" pp"))
        .map(|(pp, _)| pp.trim().parse().expect("a pp delta"))
        .unwrap_or_else(|| panic!("no pp delta in:\n{text}"));
    assert!(
        pp.abs() < 0.5,
        "the headline delta should read as a no-op, got {pp} pp:\n{text}"
    );
    assert!(
        text.contains("-607") && text.contains("+603"),
        "the cohort rows should show where the prose went:\n{text}"
    );
}

/// Prose that is the product rather than prose about the code. Declared, never
/// inferred — and said out loud, because a corpus that vanished silently would
/// read as a repository with less prose in it.
#[test]
fn an_ernestignore_excludes_a_declared_corpus_and_says_so() {
    let dir = scratch("corpus");
    std::fs::create_dir_all(dir.join("stories")).unwrap();
    std::fs::write(dir.join("sample.php"), "<?php\n// A note.\n$x = 1;\n").unwrap();
    std::fs::write(
        dir.join("stories/chapter-one.md"),
        "# Chapter One\n\nThe rain fell on the reticulated widget for a long time.\n",
    )
    .unwrap();

    let measured = ernest(&[dir.to_str().unwrap(), "--json"]);
    let with_corpus: serde_json::Value = serde_json::from_slice(&measured.stdout).unwrap();
    assert_eq!(with_corpus["files_scanned"], 2);

    std::fs::write(dir.join(".ernestignore"), "stories/\n").unwrap();

    // With the breakdown asked for, so the absence below is an absence from
    // something rather than from a report that never names a language.
    let out = ernest(&[dir.to_str().unwrap(), "--by", "language"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(
        text.contains(".ernestignore applied"),
        "an excluded corpus must be declared in the report:\n{text}"
    );
    assert!(
        text.contains("php"),
        "the breakdown should still name what was measured:\n{text}"
    );
    assert!(
        !text.contains("markdown"),
        "the corpus should be gone from the breakdown:\n{text}"
    );

    // Honored at every scope: a corpus is not ernest's subject at any of them,
    // which is what separates this from the git-derived rules.
    for scope in ["shared", "local", "all"] {
        let out = ernest(&[dir.to_str().unwrap(), "--scope", scope, "--json"]);
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            report["files_scanned"], 1,
            "corpus leaked at --scope {scope}"
        );
    }
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
    std::fs::write(
        dir.join("cache/GENERATED.md"),
        "# Noise\n\nGenerated prose.\n",
    )
    .unwrap();

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
    std::fs::write(
        dir.join("notes/EPHEMERAL.md"),
        "# Gone\n\nEphemeral prose.\n",
    )
    .unwrap();
    std::fs::write(dir.join("notes/KEPT.md"), "# Kept\n\nDurable prose.\n").unwrap();

    let out = ernest(&[dir.to_str().unwrap(), "--json", "--by", "file"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["files"].as_array().unwrap().len(),
        0,
        "a nested .gitignore inside a locally-excluded directory still wins"
    );
}

/// The one exclusion that cannot be delegated. git keeps its own directory out
/// structurally rather than by rule — `git check-ignore .git` says nothing — and
/// the walk sets `hidden(false)` on purpose, so without the VCS list a repo's
/// hooks read as ordinary shell source.
#[test]
fn the_vcs_directory_is_never_walked() {
    let dir = scratch("vcs");
    std::fs::create_dir_all(dir.join(".git/hooks")).unwrap();
    std::fs::write(dir.join("app.php"), "<?php\n// A note.\n$x = 1;\n").unwrap();
    std::fs::write(
        dir.join(".git/hooks/post-commit"),
        "#!/usr/bin/env bash\n# Installed by tooling, not written here.\nexit 0\n",
    )
    .unwrap();

    for scope in ["shared", "local", "all"] {
        let out = ernest(&[
            dir.to_str().unwrap(),
            "--json",
            "--by",
            "file",
            "--scope",
            scope,
        ]);
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        let paths: Vec<&str> = report["files"]
            .as_array()
            .unwrap()
            .iter()
            .map(|f| f["path"].as_str().unwrap())
            .collect();
        assert_eq!(paths.len(), 1, "at --scope {scope}: {paths:?}");
        assert!(
            paths[0].ends_with("app.php"),
            "a git hook was measured at --scope {scope}: {paths:?}"
        );
    }
}

/// Dependency and build output is the repository's declaration to make, not
/// ernest's to assume. `--scope all` turns the declaration off, so it reaches
/// what the declaration was hiding — which is what the flag says it does.
#[test]
fn scope_all_reaches_a_gitignored_dependency_tree() {
    let dir = scratch("dependencies");
    std::fs::create_dir_all(dir.join("vendor")).unwrap();
    std::fs::write(dir.join(".gitignore"), "/vendor/\n").unwrap();
    std::fs::write(dir.join("app.php"), "<?php\n// A note.\n$x = 1;\n").unwrap();
    std::fs::write(dir.join("vendor/lib.php"), "<?php\n// Nobody wrote this.\n").unwrap();

    let scanned = |scope: &str| {
        let out = ernest(&[dir.to_str().unwrap(), "--json", "--scope", scope]);
        let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
        report["files_scanned"].as_u64().unwrap()
    };

    assert_eq!(scanned("local"), 1, "the default honors the declaration");
    assert_eq!(scanned("all"), 2, "--scope all means all");
}

/// The regression guard for `require_git(false)`. A yadm-managed tree has its
/// work tree at `$HOME` and its git dir elsewhere, so a `.gitignore` sits with
/// no `.git` beside it — the shape this repository itself has. Left to the
/// crate's default, every rule in that file goes unread.
#[test]
fn a_gitignore_applies_without_a_git_directory() {
    let dir = scratch("no-git");
    std::fs::create_dir_all(dir.join("target")).unwrap();
    assert!(!dir.join(".git").exists());

    std::fs::write(dir.join(".gitignore"), "/target\n").unwrap();
    std::fs::write(dir.join("app.php"), "<?php\n// A note.\n$x = 1;\n").unwrap();
    std::fs::write(dir.join("target/built.php"), "<?php\n// Build output.\n").unwrap();

    let out = ernest(&[dir.to_str().unwrap(), "--json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        report["files_scanned"], 1,
        "a .gitignore with no .git beside it must still be honored"
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
    assert_eq!(
        report["files"][0]["prose_chars"],
        "#Whythisexists.".len() as u64
    );
}

#[test]
fn lang_narrows_the_measurement() {
    let out = ernest(&[
        fixtures().to_str().unwrap(),
        "--lang",
        "yaml",
        "--by",
        "language",
    ]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("yaml"), "{text}");
    assert!(
        !text.contains("php"),
        "php should have been excluded:\n{text}"
    );
}

/// The gate's dialect. `--format value` writes what `--max-density` reads, so
/// the two sides of a threshold speak the same language and neither needs
/// stripping.
#[test]
fn the_value_format_writes_the_number_and_nothing_else() {
    let out = ernest(&[fixtures().to_str().unwrap(), "-f", "value"]);
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).unwrap();
    assert_eq!(text, "38.2\n");
}

/// `n/a` rather than a number, matching `null` in the snapshot: nothing
/// countable was found, which is not the same as no prose.
///
/// A tree of one unsupported file, because `--lang` no longer takes a name no
/// profile carries and every name it does take has a fixture behind it.
#[test]
fn the_value_format_says_n_a_when_nothing_was_countable() {
    let dir = scratch("uncountable");
    std::fs::write(dir.join("package.json"), "{}\n").unwrap();

    let out = ernest(&[dir.to_str().unwrap(), "-f", "value"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "n/a\n");
}

#[test]
fn the_value_format_still_reports_the_verdict_through_the_exit_code() {
    let out = ernest(&[
        fixtures().to_str().unwrap(),
        "-f",
        "value",
        "--max-density",
        "0",
    ]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(String::from_utf8(out.stdout).unwrap(), "38.2\n");
    assert!(String::from_utf8(out.stderr).unwrap().contains("exceeds"));
}

/// `--json` and the format it names are one flag with two spellings, and asking
/// for both is a contradiction rather than a preference.
///
/// The refusal below is keyed on the *resolved* format rather than on which
/// spelling reached it — the defect that let `-q --by file` be refused while
/// `--format value --by file`, the same output mode, was allowed.
#[test]
fn the_format_aliases_agree_and_refuse_a_body_they_cannot_write() {
    let dir = fixtures();
    let dir = dir.to_str().unwrap();
    assert_eq!(
        ernest(&[dir, "--json"]).stdout,
        ernest(&[dir, "--format", "json"]).stdout
    );
    assert_eq!(
        ernest(&[dir, "-f", "value"]).stdout,
        ernest(&[dir, "--format", "value"]).stdout
    );
    for argv in [
        vec![dir, "--json", "--format", "text"],
        vec![dir, "--json", "--format", "json"],
        vec![dir, "--format", "value", "--by", "file"],
        vec![dir, "-f", "value", "--by", "section"],
    ] {
        assert_eq!(ernest(&argv).status.code(), Some(2), "{argv:?}");
    }
    // A snapshot with a body is the documented workflow, not a contradiction.
    assert_eq!(
        ernest(&[dir, "--json", "--by", "file"]).status.code(),
        Some(0)
    );
}

/// One axis counted from both ends, and a caller leaning on the key is not
/// making a mistake worth refusing.
#[test]
fn the_verbosity_axis_is_counted_and_clamps_silently() {
    let dir = fixtures();
    let dir = dir.to_str().unwrap();

    assert_eq!(
        ernest(&[dir, "-vvvv"]).stdout,
        ernest(&[dir, "-vvv"]).stdout
    );
    assert_eq!(ernest(&[dir, "-qq"]).stdout, ernest(&[dir, "-q"]).stdout);
    assert_eq!(ernest(&[dir, "-qqq"]).status.code(), Some(0));
    // The two ends net off: one of each is where it started.
    assert_eq!(ernest(&[dir, "-v", "-q"]).stdout, ernest(&[dir]).stdout);
}

/// Verbosity is presentation, and the machine formats have none: `--json` is a
/// contract whose shape cannot depend on a flag, and `value` is one line by
/// definition. Accepted rather than refused, so a wrapper can pass a flag set it
/// did not assemble without the combination becoming a hard failure.
#[test]
fn verbosity_never_refuses_a_machine_format() {
    let dir = fixtures();
    let dir = dir.to_str().unwrap();

    for format in ["json", "value"] {
        let plain = ernest(&[dir, "-f", format]);
        assert_eq!(plain.status.code(), Some(0), "{format}");
        for level in ["-q", "-v", "-vv", "-vvv"] {
            let loud = ernest(&[dir, "-f", format, level]);
            assert_eq!(loud.status.code(), Some(0), "{format} {level}");
            assert_eq!(loud.stdout, plain.stdout, "{format} {level}");
        }
    }
}

/// `--by` is global, so it means the same thing to a comparison as to a
/// measurement — and a flag that only worked before the subcommand would be a
/// parser artifact rather than a design.
#[test]
fn by_reaches_the_diff_subcommand_from_either_side() {
    let dir = scratch("global");
    std::fs::write(dir.join("sample.php"), "<?php\n// A note.\n$x = 1;\n").unwrap();
    let before = dir.join("before.json");
    let out = ernest(&[dir.to_str().unwrap(), "--json", "--by", "file"]);
    std::fs::write(&before, &out.stdout).unwrap();

    std::fs::write(dir.join("sample.php"), "<?php\n$x = 1;\n").unwrap();
    let after = dir.join("after.json");
    let out = ernest(&[dir.to_str().unwrap(), "--json", "--by", "file"]);
    std::fs::write(&after, &out.stdout).unwrap();

    let (before, after) = (before.to_str().unwrap(), after.to_str().unwrap());
    let ahead = ernest(&["--by", "file", "diff", before, after]);
    let behind = ernest(&["diff", before, after, "--by", "file"]);
    assert_eq!(ahead.status.code(), Some(0));
    assert_eq!(ahead.stdout, behind.stdout);
    assert!(
        String::from_utf8(behind.stdout)
            .unwrap()
            .contains("sample.php")
    );
}

/// A truncated list that looks complete is worse than no list, and 0 is the one
/// reading of `--top` that has no other spelling.
#[test]
fn top_bounds_a_ranked_view_and_zero_lifts_the_bound() {
    let dir = fixtures();
    let dir = dir.to_str().unwrap();

    let capped = ernest(&[dir, "--by", "file", "--top", "2"]);
    let text = String::from_utf8(capped.stdout).unwrap();
    assert!(text.contains("more files"), "{text}");

    let whole = ernest(&[dir, "--by", "file", "--top", "0"]);
    let text = String::from_utf8(whole.stdout).unwrap();
    assert!(!text.contains("more files"), "{text}");
    for language in fixture_languages() {
        assert!(text.contains(&language), "{language} missing from:\n{text}");
    }
}

/// One file is not one files.
#[test]
fn a_count_agrees_with_its_noun() {
    let path = fixtures().join("yaml/adversarial.yaml");
    let out = ernest(&[path.to_str().unwrap()]);
    let text = String::from_utf8(out.stdout).unwrap();
    assert!(text.contains("1 file measured"), "{text}");
}
