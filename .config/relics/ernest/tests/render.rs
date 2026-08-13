//! Rendered output, blessed.
//!
//! `tests/cli.rs` asserts what a report *means*; this asserts what it *looks
//! like*. Those are different failures — a roll-up that stops summing is a bug
//! in the arithmetic, a stray blank line is a bug in the layout — and neither
//! suite should be able to absorb the other's.
//!
//! The trees under `tests/render/` are frozen. `tests/fixtures/` grows every
//! time a format lands, so snapshots over it would churn on changes that are
//! not about rendering and drown the diff someone is meant to read.
//!
//! Regenerate after a deliberate change: `ERNEST_BLESS=1 cargo test --test
//! render`, then **read the diff**. A blessed wrong answer is still wrong.

use std::path::{Path, PathBuf};
use std::process::Command;

fn manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).to_path_buf()
}

/// Run from the manifest directory with a relative root, so every path the
/// report prints is relative and the snapshots hold on any machine.
fn run(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_ernest"))
        .current_dir(manifest())
        .args(args)
        .output()
        .expect("ernest runs");
    String::from_utf8(out.stdout).expect("utf-8 stdout")
}

/// The layout invariants, asserted on every case rather than eyeballed in each
/// snapshot — so they hold whatever the expectations happen to say, and catch
/// what a blessing would otherwise absorb.
fn well_formed(case: &str, rendered: &str) {
    assert!(
        !rendered.contains("\n\n\n"),
        "{case}: two blank lines in a row"
    );
    assert!(
        rendered.is_empty() || rendered.ends_with('\n'),
        "{case}: no final newline"
    );
    for line in rendered.lines() {
        assert_eq!(line, line.trim_end(), "{case}: trailing whitespace");
    }
    assert!(
        !rendered.contains(manifest().to_str().unwrap()),
        "{case}: an absolute path leaked into the report"
    );
}

fn check(case: &str, rendered: &str) {
    well_formed(case, rendered);
    let expected = manifest().join(format!("tests/render/expected/{case}.expected.txt"));
    if std::env::var_os("ERNEST_BLESS").is_some() {
        std::fs::write(&expected, rendered).expect("writes expectation");
        return;
    }
    let want = std::fs::read_to_string(&expected).unwrap_or_else(|_| {
        panic!(
            "missing {} — run ERNEST_BLESS=1 cargo test --test render",
            expected.display()
        )
    });
    assert_eq!(want, rendered, "{case}");
}

const TREE: &str = "tests/render/tree";
const EDGE: &str = "tests/render/edge";

#[test]
fn a_bare_run_is_the_figure_and_its_caveats() {
    check("default", &run(&[TREE]));
}

#[test]
fn the_roll_up_stops_at_the_depth_it_was_asked_for() {
    check("by-cohort", &run(&[TREE, "--by", "cohort"]));
    check("by-language", &run(&[TREE, "--by", "language"]));
}

#[test]
fn the_ranked_views_lead_with_their_measurements() {
    check("by-file", &run(&[TREE, "--by", "file"]));
    check("by-section", &run(&[TREE, "--by", "section"]));
}

#[test]
fn a_bounded_view_says_what_it_withheld() {
    check("top-2", &run(&[TREE, "--by", "file", "--top", "2"]));
    check("top-0", &run(&[TREE, "--by", "file", "--top", "0"]));
}

#[test]
fn the_line_unit_carries_its_caveat() {
    check(
        "unit-lines",
        &run(&[TREE, "--unit", "lines", "--by", "cohort"]),
    );
}

/// Nothing found is still a report, and the block that would have held the
/// breakdown is absent rather than blank.
#[test]
fn a_report_of_nothing_leaves_no_gap() {
    check("empty", &run(&[TREE, "--lang", "nothing"]));
}

/// A declared corpus, an overflowing histogram of unsupported extensions, and a
/// breakdown, all at once.
#[test]
fn the_caveats_stack_without_crowding_each_other() {
    check("edge", &run(&[EDGE, "--by", "language"]));
}

#[test]
fn the_value_format_is_one_line() {
    check("value", &run(&[TREE, "--quiet"]));
}

#[test]
fn a_diff_carries_the_same_registers_as_a_report() {
    let before = snapshot("before");
    let after = snapshot("after");
    let (b, a) = (before.to_str().unwrap(), after.to_str().unwrap());

    check("diff-default", &run(&["diff", b, a]));
    check(
        "diff-by-cohort-file",
        &run(&["diff", b, a, "--by", "cohort,file"]),
    );
    // A view asked for that the snapshots cannot serve is said out loud rather
    // than silently absent.
    check("diff-by-section", &run(&["diff", b, a, "--by", "section"]));
    check("diff-value", &run(&["diff", b, a, "--quiet"]));
}

/// Both sides measured from inside themselves, so the keys match and the movers
/// line up on `./sample.php` rather than on two different absolute paths.
fn snapshot(side: &str) -> PathBuf {
    let dir = manifest().join("tests/render/diff").join(side);
    let out = Command::new(env!("CARGO_BIN_EXE_ernest"))
        .current_dir(&dir)
        .args([".", "--json", "--by", "file"])
        .output()
        .expect("ernest runs");
    let path = std::env::temp_dir().join(format!("ernest-render-{side}.json"));
    std::fs::write(&path, out.stdout).expect("writes snapshot");
    path
}

/// Every snapshot below is a snapshot of the whole tree or of something else. An
/// ignore rule above this repository would make it the latter silently; this
/// says so instead.
#[test]
fn the_render_trees_are_walked_whole() {
    let count = |root: &str| {
        let out = run(&[root, "--json"]);
        let report: serde_json::Value = serde_json::from_str(&out).expect("json parses");
        (
            report["files_scanned"].as_u64().unwrap(),
            report["files_skipped"].as_u64().unwrap(),
        )
    };
    assert_eq!(count(TREE), (5, 2), "the canonical tree");
    assert_eq!(count(EDGE), (1, 7), "the edge tree, its corpus excluded");
}
