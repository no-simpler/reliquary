//! The binary, end to end.

// The clippy.toml carve-outs reach `#[test]` functions and `#[cfg(test)]`
// modules, not an integration-test crate's own helpers. Without this the
// restriction lints fire on the scaffolding rather than on the assertions.
#![allow(clippy::expect_used, clippy::unwrap_used)]

use assert_cmd::Command;
use predicates::prelude::*;

fn assay() -> Command {
    let mut command = Command::cargo_bin("assay").expect("the binary is built");
    // Nothing here is about this machine's own home directory.
    command.arg("--home").arg("/nonexistent");
    command
}

#[test]
fn the_roster_lists_every_station_and_runs_none() {
    assay()
        .arg("--list")
        .assert()
        .success()
        .stdout(predicate::str::contains("bedrock"));
}

#[test]
fn a_run_grades_and_exits_by_the_grade() {
    // An empty search path finds no bedrock member at all, which is the loudest
    // verdict the station has: exit 2.
    assay()
        .env("PATH", "")
        .arg("--format")
        .arg("agent")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("grade\tbroken"));
}

#[test]
fn quiet_says_nothing_when_there_is_nothing_to_say() {
    assay()
        .arg("--quiet")
        .arg("--format")
        .arg("agent")
        .assert()
        .stdout(predicate::str::is_empty().or(predicate::str::contains("grade")));
}

#[test]
fn an_unknown_station_is_refused_and_nothing_runs() {
    assay()
        .arg("bedrok")
        .assert()
        .code(3)
        .stderr(predicate::str::contains("no station is called bedrok"));
}

#[test]
fn the_json_shape_is_json() {
    let output = assay()
        .env("PATH", "")
        .arg("--format")
        .arg("json")
        .output()
        .expect("ran");
    let parsed: serde_json::Value =
        serde_json::from_slice(&output.stdout).expect("valid json on stdout");
    assert_eq!(parsed["grade"], "broken");
    assert!(parsed["reports"].is_array());
}

#[test]
fn a_named_station_runs_alone() {
    assay()
        .env("PATH", "")
        .arg("bedrock")
        .arg("--format")
        .arg("agent")
        .assert()
        .code(2)
        .stdout(predicate::str::contains("bedrock"));
}
