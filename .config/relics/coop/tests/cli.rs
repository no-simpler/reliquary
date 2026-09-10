// Clippy's in-test carve-outs (see `clippy.toml`) reach `#[test]` functions and
// `#[cfg(test)]` modules — not the helpers beside them. An integration test crate
// is test code end to end, so the carve-out belongs at its root, where its scope
// is still exactly the tests.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::path::{Path, PathBuf};
use std::process::Command;

use assert_cmd::prelude::*;
use predicates::prelude::*;
use tempfile::TempDir;

struct Coop {
    _dir: TempDir,
    base: PathBuf,
    config: PathBuf,
    state: PathBuf,
    session: String,
}

impl Coop {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temporary tree");
        // macOS /tmp is itself a symlink, and the binary resolves what it reads.
        let base = dir.path().canonicalize().expect("a real path");
        let config = base.join("config");
        let state = base.join("state");
        std::fs::create_dir_all(config.join("sources.d")).expect("a sources directory");
        std::fs::create_dir_all(&state).expect("a state directory");
        Self {
            _dir: dir,
            base,
            config,
            state,
            session: "test-session".to_owned(),
        }
    }

    fn run(&self, args: &[&str]) -> assert_cmd::assert::Assert {
        let mut command = Command::new(env!("CARGO_BIN_EXE_coop"));
        command
            .args(args)
            .env("COOP_CONFIG", &self.config)
            .env("COOP_ROOT", &self.state)
            .env("COOP_SESSION", &self.session)
            // HOME is where a tilde in a declaration expands to.
            .env("HOME", &self.base)
            .env("CLAUDECODE", "1")
            .env_remove("COOP_UI")
            .env_remove("COOP_DISABLE");
        command.assert()
    }

    /// The card is only ever drawn for a person, so a test that wants one says so.
    fn card(&self, args: &[&str]) -> String {
        let mut full = vec!["--format", "human", "--color", "never"];
        full.extend_from_slice(args);
        let output = self.run(&full).get_output().stdout.clone();
        String::from_utf8(output).expect("utf-8")
    }

    fn declare(&self, name: &str, body: &str) {
        std::fs::write(self.config.join("sources.d").join(name), body).expect("a declaration");
    }

    fn stamp(&self, name: &str) -> PathBuf {
        let path = self.base.join(name);
        std::fs::write(&path, "x").expect("a stamp");
        path
    }

    /// A producer, written as a script so the test owns exactly what it says.
    fn producer(&self, name: &str, body: &str) -> PathBuf {
        let path = self.base.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("a producer");
        make_executable(&path);
        path
    }

    fn badge(&self) -> String {
        std::fs::read_to_string(self.state.join("badge")).unwrap_or_default()
    }

    fn with_session(&self, session: &str) -> Self {
        Self {
            _dir: TempDir::new().expect("an unused handle"),
            base: self.base.clone(),
            config: self.config.clone(),
            state: self.state.clone(),
            session: session.to_owned(),
        }
    }
}

fn make_executable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let mut permissions = std::fs::metadata(path).expect("metadata").permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(path, permissions).expect("an executable bit");
}

/// The declaration that migrated `up`, verbatim in shape.
fn stale_stamp(coop: &Coop) {
    let stamp = coop.stamp("last_upped_at");
    coop.declare(
        "up.toml",
        &format!(
            r#"
[source]
id = "up"
summary = "{{age}} since the last system update"
fix = "up"

[source.when]
path = "{}"
older-than = "0s"
missing = "fire"
"#,
            stamp.display()
        ),
    );
}

// ---------------------------------------------------------------------------
// An empty coop
// ---------------------------------------------------------------------------

#[test]
fn an_empty_coop_draws_no_box_at_all() {
    let coop = Coop::new();
    assert_eq!(coop.card(&["tick"]), "");
    assert!(!coop.card(&[]).contains('╭'));
}

#[test]
fn an_empty_coop_has_an_empty_badge() {
    let coop = Coop::new();
    coop.run(&["prompt"]).success().stdout("");
}

#[test]
fn an_empty_coop_is_healthy() {
    let coop = Coop::new();
    coop.run(&["doctor"]).success();
}

// ---------------------------------------------------------------------------
// The stat tier
// ---------------------------------------------------------------------------

#[test]
fn a_stat_source_fires_and_is_retired_by_touching_its_path() {
    let coop = Coop::new();
    stale_stamp(&coop);
    assert!(coop.card(&[]).contains("since the last system update"));

    // Running `up` is the dismissal, and this is all `up` does.
    coop.declare(
        "up.toml",
        &format!(
            r#"
[source]
id = "up"
summary = "{{age}} since the last system update"
fix = "up"

[source.when]
path = "{}"
older-than = "1d"
missing = "quiet"
"#,
            coop.base.join("last_upped_at").display()
        ),
    );
    assert!(!coop.card(&[]).contains("since the last system update"));
}

#[test]
fn never_having_run_reads_differently_from_being_overdue() {
    let coop = Coop::new();
    coop.declare(
        "up.toml",
        r#"
[source]
id = "up"
summary = "{age} since the last system update"
summary-missing = "no record of a system update"
fix = "up"

[source.when]
path = "~/last_upped_at"
older-than = "0s"
missing = "fire"
"#,
    );
    let absent = coop.card(&[]);
    assert!(absent.contains("no record of a system update"), "{absent}");
    assert!(
        !absent.contains("{age}"),
        "an unresolved template reached the card"
    );

    coop.stamp("last_upped_at");
    let present = coop.card(&[]);
    assert!(
        present.contains("since the last system update"),
        "{present}"
    );
    assert!(!present.contains("no record"), "{present}");
}

#[test]
fn a_tilde_in_a_declaration_expands_so_it_travels_between_machines() {
    let coop = Coop::new();
    coop.stamp("stamp");
    coop.declare(
        "t.toml",
        r#"
[source]
id = "t"
summary = "the stamp is there"
fix = "t"

[source.when]
path = "~/stamp"
exists = true
"#,
    );
    assert!(coop.card(&[]).contains("the stamp is there"));
}

// ---------------------------------------------------------------------------
// The asked tier
// ---------------------------------------------------------------------------

#[test]
fn a_findings_producer_is_read_through_the_protocol_it_already_speaks() {
    let coop = Coop::new();
    let producer = coop.producer(
        "rote",
        r#"echo '{"station":"rote","outcome":{"ran":[{"station":"rote","severity":"soft","summary":"2 drills due","fix":"rote"}]}}'"#,
    );
    coop.declare(
        "rote.toml",
        &format!(
            r#"
[source]
id = "rote"

[source.ask]
run = ["{}"]
kind = "findings"
refresh = "10m"
"#,
            producer.display()
        ),
    );
    assert!(coop.card(&[]).contains("2 drills due"));
}

#[test]
fn a_text_producer_needs_no_protocol_at_all() {
    let coop = Coop::new();
    let producer = coop.producer("thing", "echo 'the thing needs doing'");
    coop.declare(
        "thing.toml",
        &format!(
            r#"
[source]
id = "thing"
fix = "do the thing"

[source.ask]
run = ["{}"]
kind = "text"
refresh = "10m"
"#,
            producer.display()
        ),
    );
    assert!(coop.card(&[]).contains("the thing needs doing"));
}

#[test]
fn a_cached_answer_is_reused_rather_than_asked_again() {
    let coop = Coop::new();
    let ledger = coop.base.join("runs");
    let producer = coop.producer(
        "counted",
        &format!("echo x >> {}\necho 'still outstanding'", ledger.display()),
    );
    coop.declare(
        "counted.toml",
        &format!(
            r#"
[source]
id = "counted"
fix = "act"

[source.ask]
run = ["{}"]
kind = "text"
refresh = "1h"
"#,
            producer.display()
        ),
    );
    for _ in 0..3 {
        assert!(coop.card(&[]).contains("still outstanding"));
    }
    let runs = std::fs::read_to_string(&ledger).unwrap_or_default();
    assert_eq!(
        runs.lines().count(),
        1,
        "the producer ran once, not per read"
    );
}

#[test]
fn a_moved_key_asks_again_even_though_the_clock_has_not_moved() {
    let coop = Coop::new();
    let watched = coop.stamp("watched");
    let ledger = coop.base.join("runs");
    let producer = coop.producer(
        "keyed",
        &format!("echo x >> {}\necho 'still outstanding'", ledger.display()),
    );
    coop.declare(
        "keyed.toml",
        &format!(
            r#"
[source]
id = "keyed"
fix = "act"

[source.ask]
run = ["{}"]
kind = "text"
refresh = "1h"
keys = ["{}"]
"#,
            producer.display(),
            watched.display()
        ),
    );
    coop.card(&[]);
    std::fs::write(&watched, "a longer body, so size moves").expect("a write");
    coop.run(&["refresh"]).success();
    let runs = std::fs::read_to_string(&ledger).unwrap_or_default();
    assert_eq!(
        runs.lines().count(),
        2,
        "the key moved, so the answer had to"
    );
}

#[test]
fn a_producer_that_is_not_on_this_machine_is_dormant_rather_than_fatal() {
    let coop = Coop::new();
    stale_stamp(&coop);
    coop.declare(
        "absent.toml",
        r#"
[source]
id = "absent"

[source.ask]
run = ["/definitely/not/here"]
kind = "text"
refresh = "10m"
"#,
    );
    // The declaration travels; the producer does not. The other source still works.
    assert!(coop.card(&[]).contains("since the last system update"));
    coop.run(&["sources"])
        .success()
        .stdout(predicate::str::contains("dormant"));
    coop.run(&["doctor"]).success();
}

// ---------------------------------------------------------------------------
// Cadence
// ---------------------------------------------------------------------------

#[test]
fn the_card_is_drawn_once_per_shell_and_not_again() {
    let coop = Coop::new();
    stale_stamp(&coop);
    assert!(coop.card(&["tick"]).contains("╭"), "the first prompt draws");
    assert_eq!(coop.card(&["tick"]), "", "every prompt after it does not");
    assert_eq!(coop.card(&["tick"]), "");
}

#[test]
fn a_second_shell_is_shown_the_card_too() {
    let coop = Coop::new();
    stale_stamp(&coop);
    assert!(coop.card(&["tick"]).contains("╭"));
    let second = coop.with_session("another-shell");
    assert!(second.card(&["tick"]).contains("╭"));
}

#[test]
fn a_changed_set_draws_again_in_a_shell_that_had_already_been_shown() {
    let coop = Coop::new();
    stale_stamp(&coop);
    assert!(coop.card(&["tick"]).contains("up"));
    assert_eq!(coop.card(&["tick"]), "");

    coop.declare(
        "other.toml",
        r#"
[source]
id = "other"
summary = "something else arrived"
fix = "deal with it"

[source.when]
path = "/"
exists = true
"#,
    );
    assert!(coop.card(&["tick"]).contains("something else arrived"));
}

#[test]
fn the_badge_tracks_the_count_and_clears_itself() {
    let coop = Coop::new();
    stale_stamp(&coop);
    coop.card(&["tick"]);
    assert_eq!(coop.badge(), "1");
    coop.run(&["prompt"]).success().stdout("1");

    std::fs::remove_file(coop.config.join("sources.d").join("up.toml")).expect("a removal");
    coop.card(&["tick"]);
    assert_eq!(coop.badge(), "", "an empty coop leaves no badge behind");
}

#[test]
fn a_tick_off_a_terminal_draws_nothing_whatever_is_outstanding() {
    let coop = Coop::new();
    stale_stamp(&coop);
    // No --format: stdout is a pipe and CLAUDECODE is set, so this is not a
    // person, and a card in an agent's tool result is noise.
    coop.run(&["tick"]).success().stdout("");
}

#[test]
fn the_kill_switch_silences_the_tick() {
    let coop = Coop::new();
    stale_stamp(&coop);
    let mut command = Command::new(env!("CARGO_BIN_EXE_coop"));
    command
        .args(["--format", "human", "--color", "never", "tick"])
        .env("COOP_CONFIG", &coop.config)
        .env("COOP_ROOT", &coop.state)
        .env("COOP_SESSION", &coop.session)
        .env("HOME", &coop.base)
        .env("COOP_DISABLE", "1");
    command.assert().success().stdout("");
}

// ---------------------------------------------------------------------------
// The card itself
// ---------------------------------------------------------------------------

#[test]
fn the_card_is_drawn_exactly_as_designed() {
    let coop = Coop::new();
    coop.stamp("here");
    coop.declare(
        "up.toml",
        r#"
[source]
id = "up"
summary = "3d since the last system update"
fix = "up"

[source.when]
path = "~/here"
exists = true
"#,
    );
    coop.declare(
        "rote.toml",
        r#"
[source]
id = "rote"
summary = "2 drills due"
fix = "rote"

[source.when]
path = "~/here"
exists = true
"#,
    );
    std::fs::write(coop.config.join("config.toml"), "width = 50\n").expect("a config");

    assert_eq!(
        coop.card(&[]),
        "\
╭─ coop ─────────────────────────────────────────╮
│ rote  2 drills due                             │
│ up    3d since the last system update          │
╰────────────────────────────────────────────────╯
"
    );
}

#[test]
fn ascii_is_the_same_card_where_the_other_one_cannot_arrive() {
    let coop = Coop::new();
    coop.stamp("here");
    coop.declare(
        "up.toml",
        r#"
[source]
id = "up"
summary = "3d since the last system update"
fix = "up"

[source.when]
path = "~/here"
exists = true
"#,
    );
    std::fs::write(
        coop.config.join("config.toml"),
        "width = 50\nstyle = \"ascii\"\n",
    )
    .expect("a config");
    assert_eq!(
        coop.card(&[]),
        "\
+- coop -----------------------------------------+
| up  3d since the last system update            |
+------------------------------------------------+
"
    );
}

// ---------------------------------------------------------------------------
// The other readers
// ---------------------------------------------------------------------------

#[test]
fn list_answers_json_a_script_can_read() {
    let coop = Coop::new();
    stale_stamp(&coop);
    let output = coop
        .run(&["list", "--json"])
        .success()
        .get_output()
        .stdout
        .clone();
    let rows: serde_json::Value = serde_json::from_slice(&output).expect("a document, not prose");
    assert_eq!(rows[0]["source"], "up");
    assert_eq!(rows[0]["fix"], "up");
    assert_eq!(rows[0]["severity"], "soft");
}

#[test]
fn show_carries_the_detail_the_card_leaves_out() {
    let coop = Coop::new();
    let producer = coop.producer(
        "p",
        r#"echo '{"station":"p","outcome":{"ran":[{"station":"p","severity":"soft","summary":"a summary","detail":"the long story","fix":"p"}]}}'"#,
    );
    coop.declare(
        "p.toml",
        &format!(
            r#"
[source]
id = "p"

[source.ask]
run = ["{}"]
kind = "findings"
refresh = "10m"
"#,
            producer.display()
        ),
    );
    coop.run(&["list"]).success();
    coop.run(&["show", "p"])
        .success()
        .stdout(predicate::str::contains("the long story"));
    let card = coop.card(&[]);
    assert!(
        !card.contains("the long story"),
        "the card stays one line per notice"
    );
}

#[test]
fn show_of_something_that_is_not_outstanding_fails_rather_than_pretending() {
    let coop = Coop::new();
    coop.run(&["show", "nothing"]).failure();
}

#[test]
fn doctor_answers_a_report_the_registry_can_collect() {
    let coop = Coop::new();
    let output = coop.run(&["doctor", "--json"]).get_output().stdout.clone();
    let report: relic_core_shape::Report =
        serde_json::from_slice(&output).expect("a report, not prose");
    assert_eq!(report.station, "coop");
}

/// The wire shape `assay` reads, restated here so this crate asserts against the
/// document rather than against the types that produced it.
mod relic_core_shape {
    use serde::Deserialize;

    #[derive(Deserialize)]
    pub struct Report {
        pub station: String,
    }
}

// ---------------------------------------------------------------------------
// Declarations that are wrong
// ---------------------------------------------------------------------------

#[test]
fn a_broken_declaration_is_a_finding_and_never_takes_the_others_down() {
    let coop = Coop::new();
    stale_stamp(&coop);
    coop.declare("wrong.toml", "this is not toml at all {{{");

    assert!(
        coop.card(&[]).contains("since the last system update"),
        "one bad file must not silence the good ones"
    );
    coop.run(&["doctor"])
        .code(2)
        .stdout(predicate::str::contains("will not compile"));
}

#[test]
fn a_stat_declaration_without_a_fix_is_refused_at_the_door() {
    let coop = Coop::new();
    coop.declare(
        "fixless.toml",
        r#"
[source]
id = "fixless"
summary = "something you cannot act on"

[source.when]
path = "/"
exists = true
"#,
    );
    assert!(!coop.card(&[]).contains("something you cannot act on"));
    coop.run(&["doctor"])
        .code(2)
        .stdout(predicate::str::contains("fix is required"));
}

#[test]
fn a_note_is_never_drawn_because_a_note_cannot_be_dismissed() {
    let coop = Coop::new();
    let producer = coop.producer(
        "noted",
        r#"echo '{"station":"noted","outcome":{"ran":[{"station":"noted","severity":"note","summary":"just so you know"}]}}'"#,
    );
    coop.declare(
        "noted.toml",
        &format!(
            r#"
[source]
id = "noted"

[source.ask]
run = ["{}"]
kind = "findings"
refresh = "10m"
"#,
            producer.display()
        ),
    );
    assert_eq!(coop.card(&["tick"]), "");
    assert_eq!(coop.badge(), "");
}

// ---------------------------------------------------------------------------
// Reference and doctrine
// ---------------------------------------------------------------------------

#[test]
fn help_and_guide_work_before_any_declaration_exists() {
    let coop = Coop::new();
    std::fs::remove_dir_all(coop.config.join("sources.d")).expect("no declarations");
    coop.run(&["help", "tiers"])
        .success()
        .stdout(predicate::str::contains("when — stat and arithmetic"));
    coop.run(&["guide", "cadence"])
        .success()
        .stdout(predicate::str::contains("edge-triggered"));
    coop.run(&["completions", "fish"]).success();
}

#[test]
fn nothing_the_binary_prints_uses_backticks() {
    let coop = Coop::new();
    // Bare `help` is clap rendering doc comments this crate does not own,
    // relic-core's shared ColorChoice among them. Everything below is coop's
    // own prose, and a backtick in a terminal is a backtick on the screen.
    for args in [
        vec!["guide"],
        vec!["guide", "sources"],
        vec!["guide", "cadence"],
        vec!["help", "wiring"],
        vec!["help", "tiers"],
        vec!["help", "keys"],
    ] {
        let output = coop.run(&args).get_output().stdout.clone();
        let text = String::from_utf8(output).expect("utf-8");
        assert!(!text.contains('`'), "{args:?} printed a backtick");
    }
}
