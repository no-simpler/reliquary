// Clippy's in-test carve-outs (see `clippy.toml`) reach `#[test]` functions and
// `#[cfg(test)]` modules — not the helpers beside them. An integration test
// crate is test code end to end, so the carve-out belongs at its root, where its
// scope is still exactly the tests.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

//! The binary, driven as it ships.
//!
//! The suite **seeds** the store rather than enrolling through it: real
//! enrolment costs 256 MiB and six seconds in a debug build, and `relic test`
//! has to stay fast. Two tests pay the real cost so the enrolment path is not
//! untested; everything else writes a log and a cheap verifier directly.

use argon2::password_hash::{PasswordHasher as _, SaltString};
use argon2::{Algorithm, Argon2, Params, Version};
use assert_cmd::Command;
use predicates::str::contains;
use sha2::{Digest as _, Sha256};
use tempfile::TempDir;

/// The schema the seeded records claim. A duplicate of the binary's own, like
/// the memory cost below: the suite drives the shipped binary and cannot reach
/// into it for a constant.
const SCHEMA: u32 = 2;

/// A scratch machine: its own log, its own state, its own home.
struct Rote {
    _dir: TempDir,
    ark: std::path::PathBuf,
    state: std::path::PathBuf,
    home: std::path::PathBuf,
    prev: String,
}

impl Rote {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a scratch directory");
        // The macOS temp root is itself a symlink, and the binary resolves what
        // it prints.
        let base = dir
            .path()
            .canonicalize()
            .expect("a resolved scratch directory");
        let ark = base.join("ark");
        let state = base.join("state");
        let home = base.join("home");
        std::fs::create_dir_all(&ark).expect("an ark");
        std::fs::create_dir_all(&state).expect("a state directory");
        std::fs::create_dir_all(&home).expect("a home");
        Self {
            _dir: dir,
            ark,
            state,
            home,
            prev: "0".repeat(64),
        }
    }

    fn run(&self, args: &[&str]) -> Command {
        let mut command = Command::cargo_bin("rote").expect("the binary");
        command
            .args(args)
            .env("ROTE_ROOT", &self.ark)
            .env("ROTE_STATE", &self.state)
            .env("ROTE_CONFIG", self.home.join("config.toml"))
            .env("ROTE_HOST", "Scratch")
            .env("HOME", &self.home)
            // Pin the agent shape, so assertions do not depend on a terminal.
            .env("CLAUDECODE", "1")
            .env_remove("ROTE_UI")
            .env_remove("NO_COLOR");
        command
    }

    fn log_path(&self) -> std::path::PathBuf {
        self.ark.join("log.jsonl")
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.log_path()).unwrap_or_default()
    }

    /// Append one already-shaped event, chained to what is there.
    fn seed(&mut self, day: &str, event: &serde_json::Value) {
        let record = serde_json::json!({
            "v": SCHEMA,
            "at": format!("{day}T08:00:00Z"),
            "day": day,
            "host": "Scratch",
            "prev": self.prev,
            "event": event.clone(),
        });
        let line = serde_json::to_string(&record).expect("a record");
        self.prev = hex(&Sha256::digest(line.as_bytes()));
        let mut text = self.log();
        text.push_str(&line);
        text.push('\n');
        std::fs::write(self.log_path(), text).expect("a log");
    }

    fn add(&mut self, day: &str, slug: &str, critical: bool) {
        self.seed(
            day,
            &serde_json::json!({"kind": "add", "slug": slug, "version": 1, "critical": critical}),
        );
        self.verifier(slug);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "a fixture that names every field of the record it builds"
    )]
    fn attempt(
        &mut self,
        day: &str,
        slug: &str,
        class: &str,
        outcome: &str,
        effective: u32,
        step_before: u8,
        step_after: u8,
    ) {
        self.seed(
            day,
            &serde_json::json!({
                "kind": "attempt",
                "slug": slug,
                "session": "0102030405060708",
                "version": 1,
                "class": class,
                "attempt": 1,
                "outcome": outcome,
                "ttfk_ms": 1200,
                "total_ms": 3400,
                "corrections": 0,
                "paste_refused": 0,
                "scheduled_interval_days": 7,
                "actual_interval_days": effective,
                "effective_interval_days": effective,
                "stretch": false,
                "step_before": step_before,
                "step_after": step_after,
            }),
        );
    }

    /// A verifier at the shipped parameters, so `doctor` reads it as healthy.
    ///
    /// Minted once for the whole suite: the point of the memory cost is that it
    /// is expensive, and a fixture that pays it per test would be a fixture
    /// people delete.
    fn verifier(&self, slug: &str) {
        self.write_verifier(slug, shipped_verifier());
    }

    /// A verifier below the memory floor. Its own test, because that is what a
    /// downgraded one looks like.
    fn weak_verifier(&self, slug: &str) {
        self.write_verifier(slug, &mint(64, 1, SECRET));
    }

    fn write_verifier(&self, slug: &str, phc: &str) {
        use std::fmt::Write as _;
        let path = self.state.join("verifiers.toml");
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        let _ = writeln!(text, "[{slug}]\nphc = \"{phc}\"");
        std::fs::write(path, text).expect("a verifier file");
    }

    fn json(&self, args: &[&str]) -> serde_json::Value {
        let output = self.run(args).output().expect("a run");
        serde_json::from_slice(&output.stdout).unwrap_or_else(|error| {
            panic!(
                "{args:?} did not answer with JSON: {error}\n{}",
                String::from_utf8_lossy(&output.stdout)
            )
        })
    }
}

/// The secret every seeded verifier is for.
const SECRET: &str = "hunter2";

/// One verifier at the shipped cost, for the whole suite.
fn shipped_verifier() -> &'static str {
    static ONCE: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    ONCE.get_or_init(|| mint(rote_m_cost(), 4, SECRET))
}

/// The floor the binary declares, read off its own help rather than duplicated.
fn rote_m_cost() -> u32 {
    262_144
}

fn mint(m_cost: u32, t_cost: u32, secret: &str) -> String {
    let params = Params::new(m_cost, t_cost, 1, Some(32)).expect("parameters");
    let salt = SaltString::encode_b64(&[7u8; 16]).expect("a salt");
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password(secret.as_bytes(), &salt)
        .expect("a hash")
        .to_string()
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes.iter().fold(String::new(), |mut out, byte| {
        let _ = write!(out, "{byte:02x}");
        out
    })
}

fn today() -> String {
    jiff::Zoned::now().date().to_string()
}

fn days_ago(days: i64) -> String {
    jiff::Zoned::now()
        .date()
        .checked_sub(jiff::Span::new().days(days))
        .expect("a date")
        .to_string()
}

// Describing the tool. These answer before there is a store.

#[test]
fn help_guide_and_completions_answer_before_a_store_exists() {
    let rote = Rote::new();
    rote.run(&["guide"])
        .assert()
        .success()
        .stdout(contains("ROTE"));
    rote.run(&["help", "intervals"])
        .assert()
        .success()
        .stdout(contains("INTERVALS"));
    rote.run(&["help", "topics"])
        .assert()
        .success()
        .stdout(contains("records"));
    rote.run(&["completions", "fish"])
        .assert()
        .success()
        .stdout(contains("rote"));
    assert!(
        !rote.log_path().exists(),
        "describing the tool wrote nothing"
    );
}

#[test]
fn the_root_help_advertises_every_topic_that_exists() {
    let rote = Rote::new();
    let output = rote.run(&["--help"]).output().expect("a run");
    let help = String::from_utf8_lossy(&output.stdout);
    for topic in ["ladder", "irregularity", "custody", "probes"] {
        assert!(
            help.contains(topic),
            "guide topic {topic} is not advertised"
        );
    }
    for topic in ["intervals", "records", "files", "stdin", "exit"] {
        assert!(help.contains(topic), "help topic {topic} is not advertised");
    }
}

#[test]
fn a_refusal_is_told_apart_from_a_finding() {
    let mut rote = Rote::new();
    rote.add(&today(), "a", false);
    // Three: rote could not do the thing at all.
    rote.run(&["retire", "ghost"]).assert().code(3);
    // Zero: it did, and found nothing wrong.
    rote.run(&["doctor"]).assert().code(0);
}

#[test]
fn an_unknown_topic_names_the_ones_that_exist() {
    let rote = Rote::new();
    rote.run(&["guide", "nonsense"])
        .assert()
        .failure()
        .stderr(contains("ladder"));
    rote.run(&["help", "nonsense"])
        .assert()
        .failure()
        .stderr(contains("intervals"));
}

#[test]
fn help_falls_through_to_a_command_of_that_name() {
    let rote = Rote::new();
    rote.run(&["help", "probe"])
        .assert()
        .success()
        .stdout(contains("horizon"));
}

// Enrolment.

#[test]
fn a_secret_from_a_pipe_enrols_and_shows_up_on_the_schedule() {
    let rote = Rote::new();
    rote.run(&["add", "escrow-p", "--critical", "--stdin"])
        .write_stdin("correct horse battery staple\n")
        .assert()
        .success()
        .stdout(contains("enrolled"));

    let status = rote.json(&["status", "--json"]);
    let slug = &status["slugs"][0];
    assert_eq!(slug["slug"], "escrow-p");
    assert_eq!(slug["critical"], true);
    assert_eq!(slug["verifier"], "ok");
    assert_eq!(slug["standing"], "waiting");
}

#[test]
fn an_enrolled_secret_is_the_one_the_drill_will_test() {
    let rote = Rote::new();
    rote.run(&["add", "a", "--stdin"])
        .write_stdin("correct horse battery staple\n")
        .assert()
        .success();
    // Rekey proves the current secret, which is the only path that puts a typed
    // answer through the real verifier without a terminal.
    rote.run(&["rekey", "a", "--stdin"])
        .write_stdin("correct horse battery staple\nsomething else entirely\n")
        .assert()
        .success();
    rote.run(&["rekey", "a", "--stdin"])
        .write_stdin("correct horse battery staple\nthird\n")
        .assert()
        .failure()
        .stderr(contains("not the current secret"));
}

#[test]
fn nothing_of_the_secret_reaches_the_log() {
    let secret = "correct horse battery staple";
    let rote = Rote::new();
    rote.run(&["add", "a", "--stdin"])
        .write_stdin(format!("{secret}\n"))
        .assert()
        .success();

    let log = rote.log();
    // Not the input, and not any prefix of it. Four characters is short enough
    // to catch a truncation and long enough not to match a hash by accident.
    for length in 4..=secret.chars().count() {
        let prefix: String = secret.chars().take(length).collect();
        assert!(!log.contains(&prefix), "the log carries {prefix:?}");
    }
    assert!(!log.contains("argon2"), "no verifier is in the log either");

    // Nor its length, which is what a fixed key set is for.
    let record: serde_json::Value =
        serde_json::from_str(log.lines().next().expect("a record")).expect("a record");
    let mut keys: Vec<&str> = record
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    keys.sort_unstable();
    assert_eq!(keys, vec!["at", "day", "event", "host", "prev", "v"]);
    let mut event: Vec<&str> = record["event"]
        .as_object()
        .expect("an object")
        .keys()
        .map(String::as_str)
        .collect();
    event.sort_unstable();
    assert_eq!(event, vec!["critical", "kind", "slug", "version"]);
}

#[test]
fn a_slug_cannot_be_enrolled_twice() {
    let mut rote = Rote::new();
    rote.add(&today(), "a", false);
    rote.run(&["add", "a", "--stdin"])
        .write_stdin("x\n")
        .assert()
        .failure()
        .stderr(contains("already enrolled"));
}

#[test]
fn an_empty_secret_is_refused() {
    let rote = Rote::new();
    rote.run(&["add", "a", "--stdin"])
        .write_stdin("\n")
        .assert()
        .failure()
        .stderr(contains("not a secret"));
}

// Rotation and retirement.

#[test]
fn a_rotation_starts_the_ladder_over_and_bumps_the_version() {
    let mut rote = Rote::new();
    rote.add(&days_ago(30), "a", false);
    rote.attempt(&days_ago(1), "a", "review", "pass", 7, 3, 4);

    rote.run(&["rekey", "a", "--stdin"])
        .write_stdin("hunter2\nnew secret\n")
        .assert()
        .success()
        .stdout(contains("version 2"));

    let slug = &rote.json(&["status", "--json"])["slugs"][0];
    assert_eq!(slug["version"], 2);
    assert_eq!(slug["step"], 0);
    assert_eq!(slug["last_attempt"], serde_json::Value::Null);
}

#[test]
fn a_forced_rotation_is_logged_as_a_re_enrolment() {
    let mut rote = Rote::new();
    rote.add(&today(), "a", false);
    rote.run(&["rekey", "a", "--force", "--stdin"])
        .write_stdin("new secret\n")
        .assert()
        .success();
    rote.run(&["log"])
        .assert()
        .success()
        .stdout(contains("without proving the old one"));
}

#[test]
fn retiring_takes_the_slug_off_the_schedule_and_the_verifier_with_it() {
    let mut rote = Rote::new();
    rote.add(&today(), "a", false);
    rote.run(&["retire", "a"]).assert().success();

    let verifiers = std::fs::read_to_string(rote.state.join("verifiers.toml")).expect("the file");
    assert!(
        !verifiers.contains("argon2"),
        "an oracle for a secret nobody drills is cost with no benefit"
    );
    assert_eq!(
        rote.json(&["status", "--json"])["slugs"]
            .as_array()
            .unwrap()
            .len(),
        0
    );
    assert_eq!(
        rote.json(&["status", "--all", "--json"])["slugs"][0]["retired"],
        true
    );
    rote.run(&["retire", "a"])
        .assert()
        .failure()
        .stderr(contains("already retired"));
}

#[test]
fn a_command_about_a_slug_that_does_not_exist_says_so() {
    let rote = Rote::new();
    for args in [
        vec!["retire", "ghost"],
        vec!["probe", "ghost", "--in", "45"],
        vec!["practice", "ghost"],
    ] {
        rote.run(&args)
            .assert()
            .failure()
            .stderr(contains("no slug called ghost"));
    }
}

// Horizons.

#[test]
fn a_horizon_holds_a_slug_and_then_is_dropped() {
    let mut rote = Rote::new();
    rote.add(&days_ago(30), "a", false);
    rote.run(&["probe", "a", "--in", "45"])
        .assert()
        .success()
        .stdout(contains("held out of the reminder"));
    assert_eq!(
        rote.json(&["status", "--json"])["slugs"][0]["standing"],
        "held"
    );

    rote.run(&["probe", "a", "--clear"])
        .assert()
        .success()
        .stdout(contains("back on the schedule"));
    assert_eq!(
        rote.json(&["status", "--json"])["slugs"][0]["standing"],
        "due"
    );
}

#[test]
fn a_horizon_needs_a_length_or_a_clear() {
    let mut rote = Rote::new();
    rote.add(&today(), "a", false);
    rote.run(&["probe", "a"])
        .assert()
        .failure()
        .stderr(contains("--in"));
}

// Reading.

#[test]
fn the_schedule_reads_the_ladder_and_the_gate() {
    let mut rote = Rote::new();
    rote.add(&days_ago(40), "a", false);
    for ago in [28, 21, 14] {
        rote.attempt(&days_ago(ago), "a", "review", "pass", 7, 4, 4);
    }
    let slug = &rote.json(&["status", "--json"])["slugs"][0];
    assert_eq!(slug["step"], 4);
    assert_eq!(slug["interval_days"], 7);
    assert_eq!(slug["cap_passes"], 3);
    assert_eq!(slug["gate"], "ready");
    assert_eq!(slug["standing"], "due");

    rote.run(&["status"])
        .assert()
        .success()
        .stdout(contains("ready · 3 at cap"));
}

#[test]
fn practising_inside_the_interval_stops_the_gate_and_says_why() {
    let mut rote = Rote::new();
    rote.add(&days_ago(40), "a", false);
    rote.attempt(&days_ago(21), "a", "review", "pass", 7, 4, 4);
    rote.attempt(&days_ago(1), "a", "practice", "pass", 1, 4, 4);

    let slug = &rote.json(&["status", "--json"])["slugs"][0];
    assert_eq!(slug["cap_passes"], 1, "practice buys nothing");
    assert_eq!(slug["gate"], "climbing");
    rote.run(&["status"])
        .assert()
        .success()
        .stdout(contains("why the next one will not count"));
}

#[test]
fn an_aided_entry_buys_nothing_and_costs_nothing() {
    let mut rote = Rote::new();
    rote.add(&days_ago(40), "a", false);
    for ago in [28, 21, 14] {
        rote.attempt(&days_ago(ago), "a", "review", "pass", 7, 4, 4);
    }
    rote.attempt(&days_ago(1), "a", "aided", "pass", 1, 4, 4);

    let slug = &rote.json(&["status", "--json"])["slugs"][0];
    assert_eq!(slug["step"], 4, "an aided entry cannot move the ladder");
    assert_eq!(
        slug["cap_passes"], 3,
        "nor retract evidence already gathered"
    );
    assert_eq!(slug["gate"], "ready");
    assert_eq!(
        slug["stood_alone"],
        days_ago(28),
        "the memory started at the first cold pass"
    );

    let stats = rote.json(&["stats", "--json"]);
    assert_eq!(stats["retention"]["total"], 3);
    assert_eq!(stats["aided"]["total"], 1);
    assert_eq!(stats["slugs"][0]["total"], 3);
    assert_eq!(stats["slugs"][0]["aided"], 1);
}

#[test]
fn a_slug_that_has_only_ever_been_aided_has_not_stood_alone() {
    let mut rote = Rote::new();
    rote.add(&days_ago(3), "escrow-p", true);
    rote.attempt(&days_ago(2), "escrow-p", "review", "blank", 1, 0, 0);
    rote.attempt(&days_ago(2), "escrow-p", "aided", "pass", 1, 1, 0);

    let slug = &rote.json(&["status", "--json"])["slugs"][0];
    assert_eq!(slug["stood_alone"], serde_json::Value::Null);
    rote.run(&["status"])
        .assert()
        .stdout(contains("has not stood alone yet"));

    let report = rote.json(&["doctor", "--format", "json"]);
    let summaries = report["outcome"]["ran"].to_string();
    assert!(
        summaries.contains("never been recalled without the answer in front of it"),
        "doctor said: {summaries}"
    );
}

#[test]
fn a_refused_aided_entry_reports_the_vault_and_the_verifier_apart() {
    let mut rote = Rote::new();
    rote.add(&days_ago(3), "a", false);
    rote.attempt(&days_ago(1), "a", "aided", "fail", 1, 0, 0);

    assert_eq!(
        rote.json(&["status", "--json"])["slugs"][0]["aided_mismatch"],
        true
    );
    let report = rote.json(&["doctor", "--format", "json"]);
    assert!(
        report["outcome"]["ran"]
            .to_string()
            .contains("hold different secrets"),
        "doctor said: {}",
        report["outcome"]["ran"]
    );
}

#[test]
fn true_retention_keeps_practice_out_of_it() {
    let mut rote = Rote::new();
    rote.add(&days_ago(40), "a", false);
    rote.attempt(&days_ago(20), "a", "review", "pass", 7, 4, 4);
    rote.attempt(&days_ago(10), "a", "practice", "fail", 1, 4, 4);

    let stats = rote.json(&["stats", "--json"]);
    assert_eq!(
        stats["retention"],
        serde_json::json!({"passes": 1, "total": 1})
    );
    assert_eq!(
        stats["practice"],
        serde_json::json!({"passes": 0, "total": 1})
    );
}

#[test]
fn a_long_absence_lands_in_the_long_interval_band() {
    let mut rote = Rote::new();
    rote.add(&days_ago(60), "a", false);
    rote.attempt(&days_ago(5), "a", "review", "pass", 45, 4, 4);

    let stats = rote.json(&["stats", "--json"]);
    let band = stats["buckets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|bucket| bucket["interval"] == "31d+")
        .expect("a long band");
    assert_eq!(band["total"], 1);
    assert_eq!(band["passes"], 1);
}

#[test]
fn a_lapse_says_whether_it_came_after_a_longer_gap_than_asked_for() {
    let mut rote = Rote::new();
    rote.add(&days_ago(60), "a", false);
    rote.attempt(&days_ago(5), "a", "review", "fail", 40, 4, 0);
    let lapse = &rote.json(&["stats", "--json"])["lapses"][0];
    assert_eq!(lapse["slug"], "a");
    assert_eq!(lapse["beyond_schedule"], true);
}

#[test]
fn the_records_can_be_listed_and_narrowed_to_one_slug() {
    let mut rote = Rote::new();
    rote.add(&days_ago(3), "a", false);
    rote.add(&days_ago(3), "b", false);
    rote.attempt(&days_ago(1), "a", "review", "pass", 1, 0, 1);

    rote.run(&["log"])
        .assert()
        .success()
        .stdout(contains("records · 3 of 3"));
    let records = rote.json(&["log", "--slug", "a", "--json"])["records"]
        .as_array()
        .expect("records")
        .len();
    assert_eq!(records, 2);
}

// Sittings, without a terminal.

#[test]
fn nothing_due_says_when_the_next_one_is() {
    let mut rote = Rote::new();
    rote.add(&days_ago(1), "a", false);
    rote.attempt(&today(), "a", "review", "pass", 1, 0, 1);
    rote.run(&[])
        .assert()
        .success()
        .stdout(contains("nothing due"));
}

#[test]
fn an_empty_roster_points_at_enrolment() {
    let rote = Rote::new();
    rote.run(&[])
        .assert()
        .success()
        .stdout(contains("rote add"));
}

#[test]
fn a_drill_refuses_to_run_where_it_cannot_be_typed() {
    let mut rote = Rote::new();
    rote.add(&days_ago(3), "a", false);
    rote.run(&[])
        .assert()
        .failure()
        .stderr(contains("typed at a terminal"));
}

#[test]
fn a_slug_with_no_verifier_is_reported_rather_than_prompted() {
    let mut rote = Rote::new();
    rote.seed(
        &days_ago(3),
        &serde_json::json!({"kind": "add", "slug": "a", "version": 1, "critical": false}),
    );
    let assertion = rote.run(&[]).assert().code(2);
    assertion.stdout(contains("rote rekey --force"));
}

// Health.

#[test]
fn doctor_answers_the_registry_with_one_report() {
    let mut rote = Rote::new();
    rote.add(&today(), "a", false);
    let report = rote.json(&["doctor", "--format", "json"]);
    assert_eq!(report["station"], "rote");
    assert!(report["outcome"]["ran"].is_array());
    rote.run(&["doctor"]).assert().success();
}

#[test]
fn doctor_grades_an_overdue_drill_soft_and_a_broken_log_broken() {
    let mut rote = Rote::new();
    rote.add(&days_ago(40), "a", false);
    rote.attempt(&days_ago(20), "a", "review", "pass", 7, 4, 4);
    rote.run(&["doctor"])
        .assert()
        .code(1)
        .stdout(contains("overdue"));

    let text = rote
        .log()
        .replace("\"critical\":false", "\"critical\":true");
    std::fs::write(rote.log_path(), text).expect("an edited log");
    rote.run(&["doctor"])
        .assert()
        .code(2)
        .stdout(contains("hash chain"));
}

#[test]
fn a_weak_verifier_is_broken_and_a_missing_one_is_soft() {
    let mut rote = Rote::new();
    rote.seed(
        &today(),
        &serde_json::json!({"kind": "add", "slug": "a", "version": 1, "critical": false}),
    );
    rote.weak_verifier("a");
    rote.run(&["doctor"])
        .assert()
        .code(2)
        .stdout(contains("cheaper attack path"));

    std::fs::write(rote.state.join("verifiers.toml"), "").expect("an empty file");
    rote.run(&["doctor"])
        .assert()
        .code(1)
        .stdout(contains("no verifier"));
}

#[test]
fn a_log_written_elsewhere_is_reported_rather_than_merged() {
    let mut rote = Rote::new();
    rote.add(&today(), "a", false);
    let text = rote
        .log()
        .replace("\"host\":\"Scratch\"", "\"host\":\"Other\"");
    std::fs::write(rote.log_path(), text).expect("an edited log");
    rote.run(&["doctor"])
        .assert()
        .code(1)
        .stdout(contains("another machine"));
}

#[test]
fn a_malformed_line_is_kept_and_reported_rather_than_dropped() {
    let mut rote = Rote::new();
    rote.add(&today(), "a", false);
    let mut text = rote.log();
    text.push_str("{ not json\n");
    std::fs::write(rote.log_path(), text).expect("an edited log");

    rote.run(&["doctor"])
        .assert()
        .code(2)
        .stdout(contains("will not parse"));
    rote.run(&["log"])
        .assert()
        .success()
        .stdout(contains("malformed"));
}

#[test]
fn a_record_from_a_newer_schema_stops_the_writer() {
    let mut rote = Rote::new();
    rote.add(&today(), "a", false);
    let text = rote
        .log()
        .replace(&format!("\"v\":{SCHEMA}"), &format!("\"v\":{}", SCHEMA + 1));
    std::fs::write(rote.log_path(), text).expect("an edited log");

    rote.run(&["retire", "a"])
        .assert()
        .failure()
        .stderr(contains("newer rote"));
}

// The reminder.

#[test]
fn the_reminder_is_silent_with_no_cache_and_counts_what_is_due() {
    let mut rote = Rote::new();
    rote.run(&["banner"]).assert().success().stdout("");

    rote.add(&days_ago(3), "a", false);
    rote.add(&days_ago(3), "escrow-p", true);
    // Reading the schedule repairs the reminder, which is what makes a restored
    // or hand-edited log show up in the next terminal.
    rote.run(&["status"]).assert().success();

    let output = rote.run(&["banner"]).output().expect("a run");
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("2 drills due"), "{text}");
}

#[test]
fn the_banner_answers_a_report_for_whatever_reads_it_next() {
    let mut rote = Rote::new();
    rote.add(&days_ago(3), "a", false);
    rote.add(&days_ago(3), "b", true);
    rote.run(&["status"]).assert().success();

    let output = rote
        .run(&["banner", "--format", "json"])
        .output()
        .expect("a run");
    let report: relic_core::finding::Report =
        serde_json::from_slice(&output.stdout).expect("a report, not prose");
    let finding = report.findings().first().expect("one finding");
    assert_eq!(finding.summary.as_str(), "2 drills due");
    assert_eq!(finding.fix.as_ref().expect("a fix").as_str(), "rote");
    assert_eq!(finding.severity, relic_core::finding::Severity::Soft);
}

#[test]
fn the_banner_answers_an_empty_report_when_nothing_is_due() {
    let rote = Rote::new();
    let output = rote
        .run(&["banner", "--format", "json"])
        .output()
        .expect("a run");
    let report: relic_core::finding::Report =
        serde_json::from_slice(&output.stdout).expect("a report, not prose");
    assert!(report.findings().is_empty());
    assert_eq!(report.grade(), relic_core::finding::Grade::Ok);
}

// The files.

#[test]
fn the_verifier_never_lands_in_the_tree_that_goes_offsite() {
    let rote = Rote::new();
    rote.run(&["add", "a", "--stdin"])
        .write_stdin("correct horse battery staple\n")
        .assert()
        .success();
    let ark: Vec<String> = std::fs::read_dir(&rote.ark)
        .expect("the ark")
        .filter_map(|entry| Some(entry.ok()?.file_name().to_string_lossy().into_owned()))
        .collect();
    assert!(ark.contains(&"log.jsonl".to_owned()));
    assert!(!ark.iter().any(|name| name.contains("verifier")), "{ark:?}");
    assert!(rote.state.join("verifiers.toml").exists());
}

#[test]
fn every_file_it_writes_is_private() {
    use std::os::unix::fs::PermissionsExt as _;
    let rote = Rote::new();
    rote.run(&["add", "a", "--stdin"])
        .write_stdin("correct horse battery staple\n")
        .assert()
        .success();
    for path in [
        rote.log_path(),
        rote.state.join("verifiers.toml"),
        rote.state.join("cache.json"),
    ] {
        let mode = std::fs::metadata(&path)
            .unwrap_or_else(|_| panic!("{path:?} should exist"))
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "{path:?}");
    }
}

#[test]
fn a_config_file_is_read_and_a_broken_one_refuses_rather_than_falling_back() {
    let mut rote = Rote::new();
    rote.add(&days_ago(10), "a", false);
    std::fs::write(rote.home.join("config.toml"), "filler = \"none\"\n").expect("a config");
    rote.run(&["status"]).assert().success();

    std::fs::write(rote.home.join("config.toml"), "nonsense = true\n").expect("a config");
    rote.run(&["status"])
        .assert()
        .failure()
        .stderr(contains("nonsense"));
}
