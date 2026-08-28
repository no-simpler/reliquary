//! The guard end to end: a definition on disk, files on disk, one exit code.
//!
//! Every case builds its own definition rather than reading the real one. The
//! real one is encrypted and names what it protects; a suite that depended on
//! it could not run on a fresh machine and could not be read in public.

// Clippy's in-test carve-outs (see `clippy.toml`) reach `#[test]` functions and
// `#[cfg(test)]` modules — not the helpers beside them. An integration test
// crate is test code end to end, so the carve-out belongs at its root, where
// its scope is still exactly the tests.
#![allow(clippy::expect_used)]

use std::fs;

use assert_cmd::Command;
use predicates::str::contains;
use tempfile::TempDir;

/// A tree with a definition, a configuration, and whatever files a case needs.
struct Fixture {
    dir: TempDir,
}

impl Fixture {
    fn new() -> Self {
        Self::with_definition(
            r#"
            [guard]
            keywords = ["barleycorn", "quernstone"]
            character-class = "[ΑαΒβΓγ]"
            "#,
        )
    }

    fn with_definition(definition: &str) -> Self {
        let dir = TempDir::new().expect("a temporary directory");
        fs::write(dir.path().join("definition.toml"), definition).expect("the definition");
        Self { dir }
    }

    fn file(&self, name: &str, contents: &[u8]) -> &Self {
        fs::write(self.dir.path().join(name), contents).expect("a file");
        self
    }

    fn config(&self, contents: &str) -> &Self {
        fs::write(self.dir.path().join("config.toml"), contents).expect("the configuration");
        self
    }

    fn run(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let root = self.dir.path();
        let mut command = Command::cargo_bin("warden").expect("the built binary");
        command
            .arg("--root")
            .arg(root)
            .arg("--definition")
            .arg(root.join("definition.toml"));
        let config = root.join("config.toml");
        if config.exists() {
            command.arg("--config").arg(config);
        }
        command.args(args).assert()
    }
}

#[test]
fn a_clean_file_passes() {
    Fixture::new()
        .file("clean.txt", b"nothing of interest here\n")
        .run(&["clean.txt", "--verbose"])
        .success()
        .stderr(contains("1 file(s) examined"));
}

#[test]
fn a_term_refuses_and_names_itself() {
    Fixture::new()
        .file("hit.txt", b"a bushel of Barleycorn\n")
        .run(&["hit.txt"])
        .failure()
        .stderr(contains("barleycorn"))
        .stderr(contains("commit refused"));
}

/// A file carrying three terms should say three. Reporting only the first
/// turns one fix into three commits.
#[test]
fn every_term_in_a_file_is_reported() {
    Fixture::new()
        .file("both.txt", b"barleycorn and quernstone\n")
        .run(&["both.txt"])
        .failure()
        .stderr(contains("2 finding(s)"));
}

#[test]
fn the_test_is_case_insensitive() {
    Fixture::new()
        .file("shout.txt", b"BARLEYCORN\n")
        .run(&["shout.txt"])
        .failure()
        .stderr(contains("barleycorn"));
}

/// The class reports the line, because the class itself says nothing about
/// where in the file the problem is.
#[test]
fn the_character_class_reports_its_first_line() {
    Fixture::new()
        .file("greek.txt", "ordinary\nΑβΓ here\nmore\n".as_bytes())
        .run(&["greek.txt"])
        .failure()
        .stderr(contains("ΑβΓ here"));
}

/// A term is matched literally. A definition is a list of words, not a list of
/// expressions, and a `.` in one must not become "any character".
#[test]
fn a_term_is_matched_literally() {
    Fixture::with_definition(
        r#"
        [guard]
        keywords = ["a.c"]
        "#,
    )
    .file("literal.txt", b"abc\n")
    .run(&["literal.txt"])
    .success();
}

#[test]
fn unreadable_content_is_refused_rather_than_skipped() {
    Fixture::new()
        .file("blob.bin", &[0xff, 0xfe, 0x00, 0x01])
        .run(&["blob.bin"])
        .failure()
        .stderr(contains("nothing can vouch"));
}

#[test]
fn an_allowed_binary_passes() {
    let fixture = Fixture::new();
    fixture
        .file("blob.bin", &[0xff, 0xfe, 0x00, 0x01])
        .config("[warden]\nbinary-allowed = [\"blob.bin\"]\n")
        .run(&["blob.bin"])
        .success();
}

/// The allowlist is per path, not a blanket permission for unreadable content.
#[test]
fn allowing_one_binary_does_not_allow_another() {
    let fixture = Fixture::new();
    fixture
        .file("allowed.bin", &[0xff, 0xfe])
        .file("other.bin", &[0xff, 0xfe])
        .config("[warden]\nbinary-allowed = [\"allowed.bin\"]\n")
        .run(&["allowed.bin", "other.bin"])
        .failure()
        .stderr(contains("other.bin"));
}

#[test]
fn an_empty_file_is_nothing_to_report() {
    Fixture::new()
        .file("empty.txt", b"")
        .run(&["empty.txt"])
        .success();
}

/// A staged path deleted between `git diff` and the read is a race with the
/// working tree, not a finding.
#[test]
fn a_vanished_path_is_not_a_finding() {
    Fixture::new().run(&["gone.txt"]).success();
}

/// A guard that would refuse nothing must refuse to run. Passing every commit
/// while reporting that it checked is the one outcome worse than no guard.
#[test]
fn an_empty_definition_is_refused() {
    Fixture::with_definition("[guard]\nkeywords = []\ncharacter-class = \"\"\n")
        .file("any.txt", b"content\n")
        .run(&["any.txt"])
        .failure()
        .stderr(contains("defines nothing to test for"));
}

#[test]
fn an_unknown_key_in_the_definition_is_refused() {
    Fixture::with_definition("[guard]\nkeywords = [\"x\"]\nkeyword = [\"typo\"]\n")
        .file("any.txt", b"content\n")
        .run(&["any.txt"])
        .failure()
        .stderr(contains("keyword"));
}

#[test]
fn an_absent_definition_names_the_remedy() {
    let dir = TempDir::new().expect("a temporary directory");
    Command::cargo_bin("warden")
        .expect("the built binary")
        .arg("--root")
        .arg(dir.path())
        .arg("--definition")
        .arg(dir.path().join("nowhere.toml"))
        .arg("any.txt")
        .assert()
        .failure()
        .stderr(contains("yadm decrypt"));
}

#[test]
fn an_unknown_key_in_the_configuration_is_refused() {
    Fixture::new()
        .file("any.txt", b"content\n")
        .config("[warden]\nbinary_allowed = []\n")
        .run(&["any.txt"])
        .failure()
        .stderr(contains("configuration"));
}

/// A definition that cannot compile is a definition that tests nothing.
#[test]
fn an_uncompilable_class_is_refused() {
    Fixture::with_definition("[guard]\ncharacter-class = \"[unclosed\"\n")
        .file("any.txt", b"content\n")
        .run(&["any.txt"])
        .failure()
        .stderr(contains("does not compose"));
}

/// A path with a space in it is one path. The retired hook split on
/// whitespace, so this case broke the guard into fragments.
#[test]
fn a_path_with_a_space_is_one_path() {
    Fixture::new()
        .file("two words.txt", b"barleycorn\n")
        .run(&["two words.txt"])
        .failure()
        .stderr(contains("two words.txt"));
}

/// The staged set, for real: no paths given, so the guard asks git. This is the
/// path every commit takes, and the one the retired hook got wrong by reading
/// the whole tree instead.
mod staged {
    use super::Fixture;
    use assert_cmd::Command;
    use predicates::prelude::PredicateBooleanExt;
    use predicates::str::contains;
    use std::process::Command as Plain;

    fn repo(fixture: &Fixture) {
        for args in [
            vec!["init", "--quiet"],
            vec!["config", "user.name", "test"],
            vec!["config", "user.email", "test@localhost"],
            vec!["config", "commit.gpgsign", "false"],
        ] {
            let status = Plain::new("git")
                .current_dir(fixture.dir.path())
                .args(&args)
                .status()
                .expect("git runs");
            assert!(status.success(), "git {args:?}");
        }
    }

    fn stage(fixture: &Fixture, path: &str) {
        let status = Plain::new("git")
            .current_dir(fixture.dir.path())
            .args(["add", "--", path])
            .status()
            .expect("git runs");
        assert!(status.success());
    }

    fn guard(fixture: &Fixture) -> assert_cmd::assert::Assert {
        let root = fixture.dir.path();
        Command::cargo_bin("warden")
            .expect("the built binary")
            .current_dir(root)
            .arg("--root")
            .arg(root)
            .arg("--definition")
            .arg(root.join("definition.toml"))
            .arg("--verbose")
            .assert()
    }

    #[test]
    fn a_clean_staged_set_passes() {
        let fixture = Fixture::new();
        repo(&fixture);
        fixture.file("clean.txt", b"nothing of interest\n");
        stage(&fixture, "clean.txt");
        guard(&fixture)
            .success()
            .stderr(contains("1 file(s) examined"));
    }

    /// Only what is staged. A dirty working tree beside it is not this commit.
    #[test]
    fn an_unstaged_file_is_not_this_commit() {
        let fixture = Fixture::new();
        repo(&fixture);
        fixture.file("clean.txt", b"nothing of interest\n");
        stage(&fixture, "clean.txt");
        fixture.file("loose.txt", b"barleycorn\n");
        guard(&fixture).success();
    }

    #[test]
    fn a_staged_term_refuses() {
        let fixture = Fixture::new();
        repo(&fixture);
        fixture.file("hit.txt", b"barleycorn\n");
        stage(&fixture, "hit.txt");
        guard(&fixture).failure().stderr(contains("barleycorn"));
    }

    /// Outside a repository, git answers `--cached` with a usage screen about
    /// the flag. The cause has to come from a question with a real answer.
    #[test]
    fn no_repository_says_so_rather_than_quoting_git() {
        let fixture = Fixture::new();
        guard(&fixture)
            .failure()
            .stderr(contains("not a git repository"))
            .stderr(contains("usage").not());
    }

    /// Guarding nothing is legal and must never be silent: the same output
    /// would follow from git answering for the wrong repository.
    #[test]
    fn an_empty_staged_set_says_so() {
        let fixture = Fixture::new();
        repo(&fixture);
        guard(&fixture)
            .success()
            .stderr(contains("nothing is staged"));
    }
}
