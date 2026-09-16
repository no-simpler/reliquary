//! The published binary, through its own command line.
//!
//! Everything that can be asserted without a terminal. The card, the placement,
//! the cursor and what a prompt does with a paste only exist at a tty;
//! `CLAUDE.md` records how to reach those.

mod support;

use predicates::prelude::*;
use rote::corpus::record::Outcome;
use support::{Drilled, Rote, day, days_ago, today};

// ── describing the tool ──────────────────────────────────────────────────────

#[test]
fn the_root_help_advertises_both_namespaces() {
    Rote::new()
        .cmd(&["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rote guide"))
        .stdout(predicate::str::contains("rote help"));
}

#[test]
fn every_reference_topic_answers() {
    let rote = Rote::new();
    for topic in [
        "keys",
        "intervals",
        "records",
        "files",
        "machines",
        "stdin",
        "exit",
    ] {
        rote.cmd(&["help", topic])
            .assert()
            .success()
            .stdout(predicate::str::contains(topic.to_uppercase()));
    }
}

#[test]
fn the_guide_carries_doctrine_and_names_the_good_faith_principle() {
    Rote::new()
        .cmd(&["guide"])
        .assert()
        .success()
        .stdout(predicate::str::contains("THE LADDER"))
        .stdout(predicate::str::contains("good faith"));
}

#[test]
fn an_unknown_topic_names_the_ones_that_exist() {
    Rote::new()
        .cmd(&["help", "nonsense"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("records"));
}

#[test]
fn completions_are_generated_for_a_real_shell() {
    Rote::new()
        .cmd(&["completions", "fish"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rote"));
}

#[test]
fn an_unknown_verb_is_refused() {
    let rote = Rote::new();
    rote.cmd(&["nonsense", "a"]).assert().failure();
}

// ── the flagship gate ────────────────────────────────────────────────────────

#[test]
fn a_machine_with_no_marker_refuses_to_write_and_says_how_to_fix_it() {
    let rote = Rote::new();
    rote.demote();
    rote.cmd(&["enroll", "escrow-p", "--stdin"])
        .write_stdin("whatever\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not the flagship"))
        .stderr(predicate::str::contains("Reads are fine"));
}

#[test]
fn a_marker_naming_another_machine_refuses_and_names_it() {
    let rote = Rote::new();
    let other = rote::machine::MachineId::of("somebody else");
    rote.marker_names(&other);
    rote.cmd(&["enroll", "a", "--stdin"])
        .write_stdin("whatever\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains(other.to_string()));
}

#[test]
fn reading_still_works_where_writing_does_not() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "escrow-p", true);
    rote.demote();
    rote.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("escrow-p@1"));
    rote.cmd(&["stats"]).assert().success();
    rote.cmd(&["machines"])
        .assert()
        .success()
        .stdout(predicate::str::contains("not the flagship"));
}

#[test]
fn an_empty_marker_authorises_and_the_machines_table_says_which_machine_this_is() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    let machine = rote.machine.to_string();
    rote.cmd(&["machines"])
        .assert()
        .success()
        .stdout(predicate::str::contains(&machine))
        .stdout(predicate::str::contains("Scratch"));
}

// ── enrolment ────────────────────────────────────────────────────────────────

#[test]
fn enrolling_writes_a_verifier_and_a_record() {
    let rote = Rote::new();
    rote.cmd(&["enroll", "escrow-p", "--critical", "--stdin"])
        .write_stdin(format!("{}\n", support::SECRET))
        .assert()
        .success()
        .stdout(predicate::str::contains("escrow-p@1 enrolled"));

    let chain = std::fs::read_to_string(rote.chain()).unwrap();
    assert!(chain.contains("\"kind\":\"enroll\""));
    assert!(chain.contains("\"critical\":true"));
    assert!(
        std::fs::read_to_string(rote.verifiers_path())
            .unwrap()
            .contains("phc")
    );
}

#[test]
fn enrolling_a_live_lineage_names_all_three_ways_forward() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "escrow-p", false);
    rote.cmd(&["enroll", "escrow-p", "--stdin"])
        .write_stdin("x\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("already enrolled"))
        .stderr(predicate::str::contains("rote rotate"))
        .stderr(predicate::str::contains("rote attach"));
}

#[test]
fn enrolling_a_retired_lineage_points_at_the_verb_that_reopens_it() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    rote.retire(day(2026, 9, 2), "a");
    rote.cmd(&["enroll", "a", "--stdin"])
        .write_stdin("x\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("rote rotate --force"));
}

#[test]
fn an_empty_secret_is_not_a_secret() {
    Rote::new()
        .cmd(&["enroll", "a", "--stdin"])
        .write_stdin("\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("empty secret"));
}

#[test]
fn a_bad_lineage_name_is_refused_at_the_edge() {
    Rote::new()
        .cmd(&["enroll", "Not A Name"])
        .assert()
        .failure();
}

// ── attachment ───────────────────────────────────────────────────────────────

#[test]
fn attaching_restores_a_verifier_and_leaves_the_record_alone() {
    let mut rote = Rote::new();
    let engram = rote.enroll(day(2026, 9, 1), "escrow-p", false);
    rote.capture(day(2026, 9, 2), "escrow-p", engram, Drilled::passed());
    // What a restore onto a new machine looks like.
    rote.drop_verifiers();

    rote.cmd(&["attach", "escrow-p", "--stdin"])
        .write_stdin(format!("{}\n", support::SECRET))
        .assert()
        .success()
        .stdout(predicate::str::contains("took your word for it"));

    let chain = std::fs::read_to_string(rote.chain()).unwrap();
    assert!(chain.contains("\"kind\":\"attach\""));

    rote.cmd(&["status", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"rung\": 1"))
        .stdout(predicate::str::contains("\"here\": \"attached\""));
}

#[test]
fn attaching_takes_any_secret_at_all_because_there_is_nothing_to_check_it_against() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    rote.drop_verifiers();
    rote.cmd(&["attach", "a", "--stdin"])
        .write_stdin("something else entirely\n")
        .assert()
        .success();
}

#[test]
fn attaching_over_a_verifier_that_is_already_here_says_it_is_replacing_one() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    rote.cmd(&["attach", "a", "--stdin"])
        .write_stdin(format!("{}\n", support::SECRET))
        .assert()
        .success();
}

#[test]
fn attaching_an_unknown_or_retired_lineage_is_refused() {
    let mut rote = Rote::new();
    rote.cmd(&["attach", "ghost", "--stdin"])
        .write_stdin("x\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("no lineage called ghost"));

    rote.enroll(day(2026, 9, 1), "a", false);
    rote.retire(day(2026, 9, 2), "a");
    rote.cmd(&["attach", "a", "--stdin"])
        .write_stdin("x\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("retired"));
}

#[test]
fn a_dormant_lineage_is_reported_and_is_not_counted_as_due() {
    let mut rote = Rote::new();
    rote.enroll_dormant(day(2026, 9, 1), "escrow-p");
    rote.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dormant"));
    rote.cmd(&["banner", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dormant on this machine"))
        .stdout(predicate::str::contains("\"fix\": \"rote\""))
        .stdout(predicate::str::contains("\"severity\": \"soft\""))
        .stdout(predicate::str::contains("drills due").not());
}

#[test]
fn attaching_clears_the_reminder_it_was_raised_by() {
    let mut rote = Rote::new();
    rote.enroll_dormant(day(2026, 9, 1), "a");
    rote.cmd(&["attach", "a", "--stdin"])
        .write_stdin(format!("{}\n", support::SECRET))
        .assert()
        .success();
    rote.cmd(&["banner", "--json"])
        .assert()
        .success()
        .stdout(predicate::str::contains("dormant").not());
}

// ── rotation and retirement ──────────────────────────────────────────────────

#[test]
fn rotating_proves_the_current_secret_and_forgets_it_afterwards() {
    let mut rote = Rote::new();
    let first = rote.enroll(day(2026, 9, 1), "escrow-p", false);
    rote.cmd(&["rotate", "escrow-p", "--stdin"])
        .write_stdin(format!("{}\nsomething new\n", support::SECRET))
        .assert()
        .success()
        .stdout(predicate::str::contains("escrow-p@2 rotated"));

    let held = std::fs::read_to_string(rote.verifiers_path()).unwrap();
    assert!(
        !held.contains(&first.to_string()),
        "a verifier for a secret that has been rotated away is a live oracle for it"
    );
}

#[test]
fn rotating_refuses_a_secret_that_is_not_the_current_one() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    rote.cmd(&["rotate", "a", "--stdin"])
        .write_stdin("not it\nsomething new\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("not the current secret"));
}

#[test]
fn rotating_a_dormant_engram_names_attach_as_the_thing_you_probably_meant() {
    let mut rote = Rote::new();
    rote.enroll_dormant(day(2026, 9, 1), "escrow-p");
    rote.cmd(&["rotate", "escrow-p", "--stdin"])
        .write_stdin("x\ny\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("nothing to prove against"))
        .stderr(predicate::str::contains("rote attach escrow-p"));
}

#[test]
fn forcing_a_rotation_records_that_the_old_secret_was_not_proved() {
    let mut rote = Rote::new();
    rote.enroll_dormant(day(2026, 9, 1), "a");
    rote.cmd(&["rotate", "a", "--force", "--stdin"])
        .write_stdin("a new one\n")
        .assert()
        .success();
    let chain = std::fs::read_to_string(rote.chain()).unwrap();
    assert!(chain.contains("\"proved\":false"));
}

#[test]
fn retiring_keeps_the_history_and_drops_every_verifier() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    rote.cmd(&["retire", "a"])
        .assert()
        .success()
        .stdout(predicate::str::contains("its history stays"));
    let held = std::fs::read_to_string(rote.verifiers_path()).unwrap_or_default();
    assert!(!held.contains("phc"));
    rote.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a@1").not());
    rote.cmd(&["status", "--all"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a@1"));
}

// ── the readings ─────────────────────────────────────────────────────────────

#[test]
fn status_shows_the_rung_the_schedule_and_what_this_machine_holds() {
    let mut rote = Rote::new();
    let engram = rote.enroll(day(2026, 9, 1), "escrow-p", true);
    rote.capture(day(2026, 9, 2), "escrow-p", engram, Drilled::passed());
    rote.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("ENGRAM"))
        .stdout(predicate::str::contains("escrow-p@1"))
        .stdout(predicate::str::contains("attached"));
}

#[test]
fn due_is_the_short_way_to_the_schedule() {
    Rote::new().cmd(&["due"]).assert().success();
}

#[test]
fn history_shows_superseded_engrams_and_the_default_does_not() {
    let mut rote = Rote::new();
    let first = rote.enroll(day(2026, 9, 1), "a", false);
    rote.rotate(day(2026, 9, 2), "a", first, true);
    rote.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a@2"))
        .stdout(predicate::str::contains("a@1").not());
    rote.cmd(&["status", "--history"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a@1"))
        .stdout(predicate::str::contains("superseded"));
}

#[test]
fn stats_are_per_engram_and_never_pooled_across_a_rotation() {
    let mut rote = Rote::new();
    let first = rote.enroll(day(2026, 9, 1), "a", false);
    rote.capture(day(2026, 9, 2), "a", first, Drilled::passed());
    let second = rote.rotate(day(2026, 9, 3), "a", first, true);
    rote.capture(day(2026, 9, 4), "a", second, Drilled::missed());
    rote.cmd(&["stats"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a@1"))
        .stdout(predicate::str::contains("a@2"))
        .stdout(predicate::str::contains("STREAK"));
}

#[test]
fn the_lineage_rollup_carries_only_what_survives_a_rotation() {
    let mut rote = Rote::new();
    let first = rote.enroll(day(2026, 9, 1), "a", false);
    rote.rotate(day(2026, 9, 2), "a", first, true);
    rote.cmd(&["stats", "--lineage"])
        .assert()
        .success()
        .stdout(predicate::str::contains("LINEAGE"))
        .stdout(predicate::str::contains("ENGRAMS"))
        .stdout(predicate::str::contains("RETENTION").not());
}

#[test]
fn the_log_names_the_engram_and_what_each_record_did() {
    let mut rote = Rote::new();
    let engram = rote.enroll(day(2026, 9, 1), "a", false);
    rote.attach(day(2026, 9, 2), "a", engram);
    rote.cmd(&["log"])
        .assert()
        .success()
        .stdout(predicate::str::contains("enroll"))
        .stdout(predicate::str::contains("attach"))
        .stdout(predicate::str::contains("nothing was checked"));
}

#[test]
fn the_log_narrows_to_one_lineage() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    rote.enroll(day(2026, 9, 1), "b", false);
    rote.cmd(&["log", "--lineage", "a"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a@1"))
        .stdout(predicate::str::contains("b@1").not());
}

#[test]
fn the_json_shapes_are_documents_rather_than_prose() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    for args in [
        vec!["status", "--json"],
        vec!["stats", "--json"],
        vec!["log", "--json"],
        vec!["machines", "--json"],
        vec!["doctor", "--json"],
        vec!["banner", "--json"],
    ] {
        let out = rote.cmd(&args).assert().get_output().stdout.clone();
        let text = String::from_utf8(out).unwrap();
        serde_json::from_str::<serde_json::Value>(&text)
            .unwrap_or_else(|_| panic!("{args:?} did not answer with a document: {text}"));
    }
}

// ── the output shape ─────────────────────────────────────────────────────────

#[test]
fn the_output_shape_changes_what_is_printed_and_never_what_is_done() {
    // A shape says how an answer is written. The suite runs at agent shape
    // throughout, so this is the half that is otherwise unwatched: a command
    // that read the shape to decide whether to act would differ here.
    let mut rote = Rote::new();
    let engram = rote.enroll(days_ago(1), "a", false);
    rote.capture(today(), "a", engram, Drilled::passed());
    let chain = std::fs::read(rote.chain()).expect("the chain");

    for args in [
        vec![],
        vec!["status"],
        vec!["stats"],
        vec!["log"],
        vec!["machines"],
        vec!["doctor"],
        vec!["banner"],
    ] {
        let codes: Vec<Option<i32>> = ["human", "agent", "json"]
            .iter()
            .map(|shape| {
                rote.cmd(&args)
                    .env("ROTE_UI", shape)
                    .assert()
                    .get_output()
                    .status
                    .code()
            })
            .collect();
        assert!(
            codes.windows(2).all(|pair| pair[0] == pair[1]),
            "{args:?} exited differently by shape: {codes:?}"
        );
        assert_eq!(
            std::fs::read(rote.chain()).expect("the chain"),
            chain,
            "{args:?} wrote to the corpus"
        );
    }
}

// ── sittings without a terminal ──────────────────────────────────────────────

#[test]
fn nothing_enrolled_says_so_rather_than_opening_a_screen() {
    Rote::new()
        .cmd(&[])
        .assert()
        .success()
        .stdout(predicate::str::contains("rote enroll"));
}

#[test]
fn nothing_due_says_when_the_next_one_is_and_offers_no_prompt_off_a_terminal() {
    let mut rote = Rote::new();
    let engram = rote.enroll(days_ago(1), "a", false);
    rote.capture(today(), "a", engram, Drilled::passed());
    rote.cmd(&[])
        .assert()
        .success()
        .stdout(predicate::str::contains("nothing due"));
}

#[test]
fn a_drill_refuses_to_run_where_it_cannot_be_typed() {
    let mut rote = Rote::new();
    rote.enroll(day(2020, 1, 1), "a", false);
    rote.cmd(&[])
        .assert()
        .failure()
        .stderr(predicate::str::contains("terminal"));
}

#[test]
fn practice_on_an_unknown_lineage_is_refused() {
    Rote::new()
        .cmd(&["practice", "ghost"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("no lineage called ghost"));
}

// ── health ───────────────────────────────────────────────────────────────────

#[test]
fn a_clean_machine_reports_nothing() {
    let mut rote = Rote::new();
    let engram = rote.enroll(days_ago(1), "a", false);
    rote.capture(today(), "a", engram, Drilled::passed());
    rote.cmd(&["doctor", "--format", "human"])
        .assert()
        .success()
        .stdout(predicate::str::contains("rote ok"));
}

#[test]
fn the_doctor_speaks_the_protocol_assay_collects() {
    let rote = Rote::new();
    let out = rote
        .cmd(&["doctor", "--json"])
        .assert()
        .get_output()
        .stdout
        .clone();
    let report: serde_json::Value = serde_json::from_slice(&out).unwrap();
    assert_eq!(report["station"], "rote");
    assert!(report["outcome"]["ran"].is_array());
    for finding in report["outcome"]["ran"].as_array().unwrap() {
        let station = finding["station"].as_str().unwrap();
        assert!(
            station == "rote" || station.starts_with("rote-"),
            "{station} is not namespaced"
        );
    }
}

#[test]
fn a_verifier_for_a_superseded_engram_is_broken_rather_than_merely_untidy() {
    let mut rote = Rote::new();
    let first = rote.enroll(day(2026, 9, 1), "a", false);
    rote.rotate(day(2026, 9, 2), "a", first, true);
    // The rotation forgot it; put it back, which is what a hand-edited file or a
    // half-finished rotation leaves behind.
    rote.hold(first, "a", day(2026, 9, 1));
    rote.cmd(&["doctor", "--format", "human"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("superseded"));
}

#[test]
fn a_verifier_below_the_memory_floor_is_broken() {
    let mut rote = Rote::new();
    let engram = rote.enroll_dormant(day(2026, 9, 1), "a");
    rote.hold_weak(engram, "a", day(2026, 9, 1));
    rote.cmd(&["doctor", "--format", "human"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("weaker than the artifact"));
}

#[test]
fn a_dormant_lineage_is_soft_and_points_at_the_sitting() {
    let mut rote = Rote::new();
    rote.enroll_dormant(today(), "a");
    rote.cmd(&["doctor", "--format", "human"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("no verifier on this machine"))
        .stdout(predicate::str::contains("fix: rote"));
}

#[test]
fn an_overdue_drill_is_reported_once_the_grace_is_spent() {
    let mut rote = Rote::new();
    rote.enroll(day(2020, 1, 1), "a", false);
    rote.cmd(&["doctor", "--format", "human"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("overdue"));
}

#[test]
fn a_refused_aided_capture_is_the_vault_drift_signal() {
    let mut rote = Rote::new();
    let engram = rote.enroll(day(2026, 9, 1), "a", false);
    rote.capture(day(2026, 9, 2), "a", engram, Drilled::aided(Outcome::Fail));
    rote.cmd(&["doctor", "--format", "human"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("different secrets"));
}

#[test]
fn something_in_the_chains_directory_that_is_not_a_chain_is_excluded_and_reported() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    let copy = rote.chains().join(format!("{} (1).jsonl", rote.machine));
    std::fs::copy(rote.chain(), &copy).unwrap();

    rote.cmd(&["doctor", "--format", "human"])
        .assert()
        .code(2)
        .stdout(predicate::str::contains("not a chain"));
    rote.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("a@1"));
}

#[test]
fn a_record_from_a_newer_schema_stops_a_write_rather_than_being_half_read() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    let chain = std::fs::read_to_string(rote.chain()).unwrap();
    // The schema this binary writes is read off the binary: a literal here goes
    // stale the next time a field is added, and it goes stale silently.
    let written = format!("\"v\":{}", rote::corpus::record::SCHEMA);
    std::fs::write(rote.chain(), chain.replace(&written, "\"v\":99")).unwrap();

    rote.cmd(&["enroll", "b", "--stdin"])
        .write_stdin("x\n")
        .assert()
        .failure()
        .stderr(predicate::str::contains("newer rote"));
}

#[test]
fn two_machines_writing_on_one_day_is_reported_as_a_double_count() {
    let mut first = Rote::new();
    let engram = first.enroll(day(2026, 9, 1), "a", false);
    let mut second = Rote::beside(&first, "two");
    second.capture(day(2026, 9, 1), "a", engram, Drilled::passed());
    first
        .cmd(&["doctor", "--format", "human"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("two machines wrote"));
    first
        .cmd(&["machines"])
        .assert()
        .success()
        .stdout(predicate::str::contains(second.machine.to_string()));
}

#[test]
fn a_capture_written_against_a_history_the_merge_does_not_give_is_reported() {
    let mut first = Rote::new();
    let engram = first.enroll(day(2026, 9, 1), "a", false);
    first.capture(day(2026, 9, 2), "a", engram, Drilled::passed());
    let mut second = Rote::beside(&first, "two");
    // A machine that had not seen the first capture, so it believed the rung was
    // still at the foot.
    second.capture(day(2026, 9, 3), "a", engram, Drilled::passed());
    first
        .cmd(&["doctor", "--format", "human"])
        .assert()
        .code(1)
        .stdout(predicate::str::contains("different histories"));
}

// ── the reminder ─────────────────────────────────────────────────────────────

#[test]
fn the_reminder_says_nothing_when_there_is_nothing_to_say() {
    Rote::new()
        .cmd(&["banner"])
        .assert()
        .success()
        .stdout(predicate::str::is_empty());
}

#[test]
fn the_reminder_counts_and_never_names() {
    let mut rote = Rote::new();
    rote.enroll(day(2020, 1, 1), "escrow-p", false);
    rote.cmd(&["banner"])
        .assert()
        .success()
        .stdout(predicate::str::contains("1 drill due"))
        .stdout(predicate::str::contains("escrow-p").not());
}

#[test]
fn the_reminder_never_resolves_the_machine_identity() {
    // It runs before every shell prompt through coop, so the subprocess that
    // derives an identity is a cost it may never pay. A seam that cannot parse
    // proves it: anything reaching for the identity would fail on it.
    let mut rote = Rote::new();
    rote.enroll(day(2020, 1, 1), "a", false);
    rote.cmd(&["banner"])
        .env("ROTE_MACHINE", "not-an-identity")
        .assert()
        .success()
        .stdout(predicate::str::contains("due"));
}

#[test]
fn a_restored_machine_is_told_every_lineage_is_dormant() {
    // The corpus comes back and the verifiers do not, which is the one state
    // the attachment verb exists to serve. Nothing in the machine-local tree
    // survives a restore either — no verifier, no stamp — and the reminder
    // depends on none of it: it reads the corpus and the verifier file afresh.
    let mut rote = Rote::new();
    rote.enroll(day(2020, 1, 1), "a", false);
    rote.enroll(day(2020, 1, 1), "b", false);
    std::fs::remove_file(rote.verifiers_path()).unwrap();
    assert!(!rote.stamp_path().exists());
    rote.cmd(&["banner"])
        .assert()
        .success()
        .stdout(predicate::str::contains("2 lineages dormant here"))
        .stdout(predicate::str::contains("due").not());
}

#[test]
fn every_write_touches_the_stamp_and_a_reading_does_not() {
    // The stamp is what the shell reminder keys on, so a write that misses it
    // is a change the reminder never hears about. Enrolling saves a verifier
    // and appends a record; retiring does both again; status writes nothing.
    let rote = Rote::new();
    assert!(!rote.stamp_path().exists());
    rote.cmd(&["enroll", "a", "--stdin"])
        .write_stdin(format!("{}\n", support::SECRET))
        .assert()
        .success();
    assert!(rote.stamp_path().is_file());
    assert_eq!(std::fs::read_to_string(rote.stamp_path()).unwrap(), "");

    std::fs::remove_file(rote.stamp_path()).unwrap();
    rote.cmd(&["status"]).assert().success();
    rote.cmd(&["banner"]).assert().success();
    assert!(!rote.stamp_path().exists());

    rote.cmd(&["retire", "a"]).assert().success();
    assert!(rote.stamp_path().is_file());
}

#[test]
fn a_reminder_with_nothing_to_say_says_nothing() {
    let rote = Rote::new();
    rote.cmd(&["banner"]).assert().success().stdout("");
}

// ── the files ────────────────────────────────────────────────────────────────

#[test]
fn nothing_but_the_record_lands_in_the_tree_that_goes_offsite() {
    let mut rote = Rote::new();
    rote.enroll(day(2026, 9, 1), "a", false);
    let mut found = Vec::new();
    for entry in walk(&rote.ark) {
        found.push(entry);
    }
    assert!(
        found.iter().all(|path| path.contains("chains")),
        "{found:?}"
    );
}

#[test]
fn what_rote_writes_is_private_the_moment_it_exists() {
    use std::os::unix::fs::PermissionsExt as _;
    let rote = Rote::new();
    rote.cmd(&["enroll", "a", "--stdin"])
        .write_stdin(format!("{}\n", support::SECRET))
        .assert()
        .success();
    for path in [rote.chain(), rote.verifiers_path()] {
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600, "{path}");
    }
}

#[test]
fn a_config_that_declares_a_ladder_that_is_not_one_is_refused_loudly() {
    let rote = Rote::new();
    rote.configure("ladder = [7, 2]\n");
    rote.cmd(&["status"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("ladder"));
}

#[test]
fn a_config_ladder_changes_the_schedule_it_proposes() {
    let mut rote = Rote::new();
    rote.configure("ladder = [3]\n");
    rote.enroll(day(2026, 9, 1), "a", false);
    rote.cmd(&["status"])
        .assert()
        .success()
        .stdout(predicate::str::contains("3d"));
}

#[test]
fn a_config_with_a_key_this_binary_does_not_read_is_refused_loudly() {
    let rote = Rote::new();
    rote.configure("policy = \"all\"\n");
    rote.cmd(&["status"]).assert().failure();
}

fn walk(root: &camino::Utf8Path) -> Vec<String> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return out;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir()
            && let Ok(path) = camino::Utf8PathBuf::from_path_buf(path.clone())
        {
            out.extend(walk(&path));
        }
        out.push(path.to_string_lossy().into_owned());
    }
    out
}

#[test]
fn drift_is_stated_by_doctor_and_restated_nowhere() {
    // Drift is a defect with a remedy, and `assay` collects doctor's findings
    // into `yadm doctor` — so doctor holds the standing claim (asserted next
    // door) and the descriptive surfaces do not restate it. Two copies of one
    // state is how they came to give opposite answers about it.
    let mut rote = Rote::new();
    let engram = rote.enroll(days_ago(40), "escrow-p", false);
    rote.capture(
        days_ago(2),
        "escrow-p",
        engram,
        Drilled::aided(Outcome::Fail),
    );

    rote.cmd(&["status"])
        .env("ROTE_UI", "human")
        .assert()
        .success()
        .stdout(predicate::str::contains("disagree").not());
    // A count, and nothing attached to it: a present-tense clause over a
    // ninety-day window outlives the state it describes.
    rote.cmd(&["stats"])
        .env("ROTE_UI", "human")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 refused"))
        .stdout(predicate::str::contains("disagree").not());
}

#[test]
fn a_later_aided_pass_clears_the_drift_and_leaves_the_count_behind() {
    let mut rote = Rote::new();
    let engram = rote.enroll(days_ago(40), "escrow-p", false);
    rote.capture(
        days_ago(3),
        "escrow-p",
        engram,
        Drilled::aided(Outcome::Fail),
    );
    rote.capture(
        days_ago(1),
        "escrow-p",
        engram,
        Drilled::aided(Outcome::Pass),
    );

    rote.cmd(&["doctor"])
        .env("ROTE_UI", "human")
        .assert()
        .success()
        .stdout(predicate::str::contains("different secrets").not());
    // The history is still history. It is a count and says so.
    rote.cmd(&["stats"])
        .env("ROTE_UI", "human")
        .assert()
        .success()
        .stdout(predicate::str::contains("1 refused"));
}

#[test]
fn the_first_interval_band_says_what_it_covers() {
    let mut rote = Rote::new();
    let engram = rote.enroll(days_ago(10), "escrow-p", false);
    // Two drills on one day: an effective gap of zero, which the first band
    // covers and must not report as a point value of one day.
    rote.capture(days_ago(1), "escrow-p", engram, Drilled::passed());
    rote.capture(days_ago(1), "escrow-p", engram, Drilled::passed());
    rote.cmd(&["stats"])
        .env("ROTE_UI", "human")
        .assert()
        .success()
        .stdout(predicate::str::contains("0-1d"));
}
