//! The sweep end to end, over trees built for each case.
//!
//! Every case builds its own tree under a temporary root and points the binary
//! at it. Nothing reads the real home directory: a test that swept it would be
//! a test that deleted things.

// Clippy's in-test carve-outs (see `clippy.toml`) reach `#[test]` functions and
// `#[cfg(test)]` modules — not the helpers beside them. An integration test
// crate is test code end to end, so the carve-out belongs at its root, where
// its scope is still exactly the tests.
#![allow(clippy::expect_used)]

use std::fs;
use std::path::Path;
use std::process::Command as Plain;

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use predicates::str::contains;
use tempfile::TempDir;

struct Tree {
    dir: TempDir,
}

impl Tree {
    fn new() -> Self {
        Self {
            dir: TempDir::new().expect("a temporary directory"),
        }
    }

    fn root(&self) -> &Path {
        self.dir.path()
    }

    fn file(&self, path: &str) -> &Self {
        let full = self.root().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("a parent directory");
        }
        fs::write(full, b"x").expect("a file");
        self
    }

    fn dir(&self, path: &str) -> &Self {
        fs::create_dir_all(self.root().join(path)).expect("a directory");
        self
    }

    /// A git repository at `path`, ignoring `ignores`, with one committed file
    /// so the repository has a tree to answer about.
    fn repo(&self, path: &str, ignores: &str) -> &Self {
        let at = self.root().join(path);
        fs::create_dir_all(&at).expect("the repository directory");
        for args in [
            vec!["init", "--quiet"],
            vec!["config", "user.name", "test"],
            vec!["config", "user.email", "test@localhost"],
            vec!["config", "commit.gpgsign", "false"],
            // The machine-wide excludes must not decide a test's outcome.
            vec!["config", "core.excludesFile", ""],
        ] {
            assert!(
                Plain::new("git")
                    .current_dir(&at)
                    .args(&args)
                    .status()
                    .expect("git runs")
                    .success(),
                "git {args:?}"
            );
        }
        fs::write(at.join(".gitignore"), ignores).expect("the ignore file");
        assert!(
            Plain::new("git")
                .current_dir(&at)
                .args(["add", ".gitignore"])
                .status()
                .expect("git runs")
                .success()
        );
        assert!(
            Plain::new("git")
                .current_dir(&at)
                .args(["commit", "--quiet", "-m", "seed"])
                .status()
                .expect("git runs")
                .success()
        );
        self
    }

    fn exists(&self, path: &str) -> bool {
        self.root().join(path).exists()
    }

    fn run(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        Command::cargo_bin("decruft")
            .expect("the built binary")
            .arg("--root")
            .arg(self.root())
            .args(args)
            .assert()
    }
}

/// The repository is the oracle, so what it ignores is a candidate.
#[test]
fn ignored_cruft_inside_a_repository_goes() {
    let tree = Tree::new();
    tree.repo("proj", ".DS_Store\n__pycache__/\n")
        .file("proj/.DS_Store")
        .file("proj/src/__pycache__/mod.pyc");
    tree.run(&[]).success();
    assert!(!tree.exists("proj/.DS_Store"));
    assert!(!tree.exists("proj/src/__pycache__"));
}

/// A repository that re-includes cruft keeps it — without this program knowing
/// the rule, because the repository answered.
#[test]
fn a_repository_that_unignores_cruft_keeps_it() {
    let tree = Tree::new();
    tree.repo("proj", ".DS_Store\n!.DS_Store\n")
        .file("proj/.DS_Store");
    tree.run(&[]).success();
    assert!(tree.exists("proj/.DS_Store"));
}

/// Only *ignored, untracked* paths are candidates, so a tracked file with a
/// cruft name is never one.
#[test]
fn a_tracked_file_is_never_a_candidate() {
    let tree = Tree::new();
    tree.repo("proj", "");
    tree.file("proj/.DS_Store");
    assert!(
        Plain::new("git")
            .current_dir(tree.root().join("proj"))
            .args(["add", "-f", ".DS_Store"])
            .status()
            .expect("git runs")
            .success()
    );
    tree.run(&[]).success();
    assert!(tree.exists("proj/.DS_Store"));
}

/// Untracked but *not ignored* is not a candidate either: the repository has
/// not decided about it, and deciding is not this program's job.
#[test]
fn an_unignored_untracked_file_is_kept() {
    let tree = Tree::new();
    tree.repo("proj", "").file("proj/.DS_Store");
    tree.run(&[]).success();
    assert!(tree.exists("proj/.DS_Store"));
}

#[test]
fn editor_state_survives_even_when_ignored() {
    let tree = Tree::new();
    tree.repo("proj", "*.swp\n*~\n")
        .file("proj/.notes.swp")
        .file("proj/notes~");
    tree.run(&[]).success();
    assert!(tree.exists("proj/.notes.swp"));
    assert!(tree.exists("proj/notes~"));
}

/// Inert, but expensive to rebuild. Cost keeps these, not safety.
#[test]
fn dependency_trees_survive_even_when_ignored() {
    let tree = Tree::new();
    tree.repo("proj", "node_modules/\ntarget/\n")
        .file("proj/node_modules/left-pad/index.js")
        .file("proj/target/debug/thing");
    tree.run(&[]).success();
    assert!(tree.exists("proj/node_modules/left-pad/index.js"));
    assert!(tree.exists("proj/target/debug/thing"));
}

#[test]
fn a_dry_run_removes_nothing_and_says_what_it_would() {
    let tree = Tree::new();
    tree.repo("proj", ".DS_Store\n").file("proj/.DS_Store");
    tree.run(&["--dry-run"])
        .success()
        .stdout(contains("would remove"))
        .stdout(contains(".DS_Store"));
    assert!(tree.exists("proj/.DS_Store"));
}

/// What a dry run lists is what a real run removes. One computation answers
/// both, so this pins that it stays one.
#[test]
fn a_dry_run_lists_exactly_what_a_real_run_removes() {
    let tree = Tree::new();
    tree.repo("proj", ".DS_Store\n__pycache__/\n")
        .file("proj/.DS_Store")
        .file("proj/a/.DS_Store")
        .file("proj/b/__pycache__/x.pyc");

    let dry = tree.run(&["--dry-run"]).get_output().stdout.clone();
    let listed = String::from_utf8(dry).expect("utf-8");
    let planned: Vec<&str> = listed
        .lines()
        .filter_map(|line| line.trim().strip_prefix("would remove "))
        .collect();
    assert_eq!(planned.len(), 3, "{listed}");

    tree.run(&[]).success();
    for path in planned {
        assert!(!tree.exists(path), "{path} survived a real run");
    }
}

/// A submodule is its own repository and answers for itself.
#[test]
fn a_nested_repository_answers_for_itself() {
    let tree = Tree::new();
    tree.repo("outer", ".DS_Store\n")
        .repo("outer/inner", "!.DS_Store\n")
        .file("outer/.DS_Store")
        .file("outer/inner/.DS_Store");
    tree.run(&[]).success();
    assert!(!tree.exists("outer/.DS_Store"));
    assert!(tree.exists("outer/inner/.DS_Store"));
}

/// Outside a repository there is nobody to ask, so the name is the answer.
#[test]
fn the_data_directory_is_swept_by_name() {
    let tree = Tree::new();
    tree.dir(".local/share/depot")
        .file(".local/share/depot/.DS_Store")
        .file(".local/share/depot/keep.txt");
    tree.run(&[]).success();
    assert!(!tree.exists(".local/share/depot/.DS_Store"));
    assert!(tree.exists(".local/share/depot/keep.txt"));
}

/// Outside a repository *and* outside the data directory, nothing is swept:
/// there is neither an oracle nor a reason to trust the name.
#[test]
fn an_ordinary_directory_is_left_alone() {
    let tree = Tree::new();
    tree.dir("Documents").file("Documents/.DS_Store");
    tree.run(&[]).success();
    assert!(tree.exists("Documents/.DS_Store"));
}

/// A directory emptied by a removal is reported, never deleted: git does not
/// track an empty directory, so removing one could silently take a placeholder.
#[test]
fn an_emptied_directory_is_reported_not_removed() {
    let tree = Tree::new();
    tree.repo("proj", ".DS_Store\n").file("proj/only/.DS_Store");
    tree.run(&[])
        .success()
        .stdout(contains("left empty"))
        .stdout(contains("only"));
    assert!(tree.exists("proj/only"));
}

#[test]
fn a_clean_tree_reports_nothing_to_remove() {
    let tree = Tree::new();
    tree.repo("proj", ".DS_Store\n").file("proj/README.md");
    tree.run(&[])
        .success()
        .stdout(contains("nothing to remove"))
        .stdout(contains("left empty").not());
}

#[test]
fn quiet_prints_the_summary_and_not_the_listing() {
    let tree = Tree::new();
    tree.repo("proj", ".DS_Store\n").file("proj/.DS_Store");
    tree.run(&["--quiet"])
        .success()
        .stdout(contains("removed 1 item"))
        .stdout(contains("  removed proj/.DS_Store").not());
}

/// A link named like cruft is one file. What it points at is not this
/// program's to judge, so the link is unlinked and the target is untouched.
#[test]
fn a_symlink_is_unlinked_not_followed() {
    let tree = Tree::new();
    tree.repo("proj", ".DS_Store\n").file("proj/real.txt");
    std::os::unix::fs::symlink(
        tree.root().join("proj/real.txt"),
        tree.root().join("proj/.DS_Store"),
    )
    .expect("a symlink");
    tree.run(&[]).success();
    assert!(!tree.exists("proj/.DS_Store"));
    assert!(tree.exists("proj/real.txt"));
}
