// Clippy's in-test carve-outs (see `clippy.toml`) reach `#[test]` functions and
// `#[cfg(test)]` modules — not the helpers beside them. An integration test crate
// is test code end to end, so the carve-out belongs at its root, where its scope
// is still exactly the tests.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use serde_json::{Value, json};
use tempfile::TempDir;

/// A machine with its own home, its own mode corpus and its own state, so a run
/// can never see the live ones.
struct Machine {
    _dir: TempDir,
    home: PathBuf,
    root: PathBuf,
    transcript: PathBuf,
}

impl Machine {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("a temporary directory");
        // On macOS the temporary root is itself a symlink, and the resolver
        // compares resolved paths.
        let base = fs::canonicalize(dir.path()).expect("canonical");
        let home = base.join("home");
        let root = base.join("state");
        fs::create_dir_all(home.join(".claude/modes")).expect("mkdir");
        Self {
            _dir: dir,
            transcript: home.join("transcript.jsonl"),
            home,
            root,
        }
    }

    fn mode(&self, name: &str, text: &str) -> &Self {
        fs::write(
            self.home.join(".claude/modes").join(format!("{name}.md")),
            text,
        )
        .expect("write a mode");
        self
    }

    /// A transcript whose last assistant turn reports a window of `tokens`.
    fn window(&self, tokens: u64) -> &Self {
        let read = tokens - 3;
        fs::write(
            &self.transcript,
            format!(
                r#"{{"type":"assistant","message":{{"model":"claude-opus-5","usage":{{"input_tokens":2,"cache_read_input_tokens":{read},"output_tokens":1}}}}}}
"#
            ),
        )
        .expect("write a transcript");
        self
    }

    /// A transcript in which the user once sent `prompt`, and nothing else.
    fn said(&self, prompt: &str) -> &Self {
        let line = json!({"type": "user", "message": {"content": prompt}});
        fs::write(&self.transcript, format!("{line}\n")).expect("write a transcript");
        self
    }

    fn cmd(&self) -> Command {
        let mut cmd = Command::cargo_bin("mantra").expect("the binary");
        cmd.env("HOME", &self.home)
            .env("MANTRA_ROOT", &self.root)
            .env_remove("CLAUDE_PROJECT_DIR")
            .env_remove("CLAUDECODE")
            .env("MANTRA_UI", "agent")
            .env("NO_COLOR", "1")
            .current_dir(&self.home);
        cmd
    }

    /// Feeds one hook payload and returns the injected context, if any.
    fn hook(&self, payload: &Value) -> Option<String> {
        let out = self
            .cmd()
            .arg("hook")
            .write_stdin(payload.to_string())
            .output()
            .expect("run");
        assert!(out.status.success(), "a hook must never fail: {out:?}");
        let stdout = String::from_utf8(out.stdout).expect("utf-8");
        if stdout.trim().is_empty() {
            return None;
        }
        let envelope: Value = serde_json::from_str(&stdout).expect("one JSON envelope");
        assert_eq!(
            envelope["hookSpecificOutput"]["hookEventName"], payload["hook_event_name"],
            "the envelope must echo the event it answers"
        );
        Some(
            envelope["hookSpecificOutput"]["additionalContext"]
                .as_str()
                .expect("additionalContext is a string")
                .to_owned(),
        )
    }

    fn base(&self, event: &str) -> Value {
        json!({
            "hook_event_name": event,
            "session_id": "s-1",
            "transcript_path": self.transcript.to_str().expect("utf-8"),
            "cwd": self.home.to_str().expect("utf-8"),
        })
    }

    fn prompt(&self, text: &str) -> Option<String> {
        let mut payload = self.base("UserPromptSubmit");
        payload["prompt"] = json!(text);
        self.hook(&payload)
    }

    fn batch(&self) -> Option<String> {
        self.hook(&self.base("PostToolBatch"))
    }

    fn start(&self, source: &str) -> Option<String> {
        let mut payload = self.base("SessionStart");
        payload["source"] = json!(source);
        self.hook(&payload)
    }

    fn state(&self) -> Option<Value> {
        let text = fs::read_to_string(self.root.join("s-1.json")).ok()?;
        Some(serde_json::from_str(&text).expect("state is JSON"))
    }
}

const TERSE: &str = "---\ntriggers:\n  - on: activate\n  - every: { tokens: 25000 }\nrefrain: Cut filler.\n---\n\n# Terse mode\n\nSay few words.\n\n## Suspend\n\nNot for warnings.\n";
const PACED: &str = "---\ntriggers:\n  - when: { context_tokens: 500000 }\n---\n\n# Paced mode\n\nStop at a milestone.\n";
const ROBUST: &str = "# Robust mode\n\nSolve the class.\n";

// ── activation ──────────────────────────────────────────────────────────────

#[test]
fn a_token_injects_the_body() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    let out = m.prompt("+terse fix the parser").expect("an injection");
    assert!(out.contains("===== MODE: terse ====="), "{out}");
    assert!(out.contains("Say few words."), "{out}");
    assert!(
        out.contains("## Suspend"),
        "the body carries its exceptions"
    );
    assert!(out.contains("ACTIVE and BINDING"), "{out}");
}

#[test]
fn a_prompt_with_no_tokens_says_nothing() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    assert_eq!(m.prompt("fix the parser"), None);
    assert_eq!(m.state(), None, "and writes no state");
}

#[test]
fn a_token_naming_no_mode_says_nothing() {
    let m = Machine::new();
    m.window(1_000);
    assert_eq!(m.prompt("+nosuchmode do a thing"), None);
}

#[test]
fn activating_twice_is_one_activation() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse +terse go").expect("an injection");
    assert_eq!(
        m.state().expect("state")["modes"]
            .as_array()
            .expect("modes")
            .len(),
        1
    );
    assert_eq!(
        m.prompt("+terse again"),
        None,
        "already on, and not yet due"
    );
}

#[test]
fn several_modes_arrive_in_the_order_they_were_written() {
    let m = Machine::new();
    m.mode("terse", TERSE).mode("robust", ROBUST).window(1_000);
    let out = m.prompt("+terse +robust go").expect("an injection");
    assert!(
        out.find("MODE: terse").expect("terse") < out.find("MODE: robust").expect("robust"),
        "{out}"
    );
}

// ── periodic refresh ────────────────────────────────────────────────────────

#[test]
fn a_refresh_waits_for_the_mark_and_then_says_the_refrain() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");

    m.window(20_000);
    assert_eq!(m.batch(), None, "not yet");

    m.window(26_000);
    let out = m.batch().expect("a refresh");
    assert!(out.contains("Cut filler."), "{out}");
    assert!(
        !out.contains("## Suspend"),
        "the carve-out stays out: {out}"
    );
    assert!(out.contains("still active"), "{out}");

    m.window(30_000);
    assert_eq!(m.batch(), None, "and not again until the next mark");
}

#[test]
fn a_turn_boundary_refreshes_too() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");
    m.window(40_000);
    let out = m.prompt("carry on").expect("a refresh");
    assert!(out.contains("Cut filler."), "{out}");
}

#[test]
fn one_boundary_can_activate_and_refresh_under_separate_frames() {
    let m = Machine::new();
    m.mode("terse", TERSE).mode("robust", ROBUST).window(1_000);
    m.prompt("+terse go").expect("activation");
    m.window(40_000);
    let out = m.prompt("+robust more").expect("both");
    let activate = out.find("The user activated").expect("activation frame");
    let refresh = out.find("still active").expect("refresh frame");
    assert!(activate < refresh, "{out}");
    assert!(
        out.find("Solve the class.").expect("robust") < refresh,
        "{out}"
    );
    assert!(out.find("Cut filler.").expect("terse") > refresh, "{out}");
}

#[test]
fn a_mode_with_no_token_gated_clause_never_refreshes() {
    let m = Machine::new();
    m.mode("robust", ROBUST).window(1_000);
    m.prompt("+robust go").expect("activation");
    m.window(900_000);
    assert_eq!(m.batch(), None);
}

// ── deferred ────────────────────────────────────────────────────────────────

#[test]
fn a_deferred_mode_says_nothing_until_its_mark() {
    let m = Machine::new();
    m.mode("paced", PACED).window(1_000);
    assert_eq!(m.prompt("+paced go"), None, "switched on, and silent");
    assert_eq!(m.state().expect("state")["modes"][0]["name"], "paced");

    m.window(499_999);
    assert_eq!(m.batch(), None);

    m.window(500_001);
    let out = m.batch().expect("the edge");
    assert!(
        out.contains("Stop at a milestone."),
        "the body, since nothing said it yet"
    );

    m.window(600_000);
    assert_eq!(m.batch(), None, "one edge, one firing");
}

// ── compaction ──────────────────────────────────────────────────────────────

#[test]
fn a_compaction_restates_every_active_mode_in_full() {
    let m = Machine::new();
    m.mode("terse", TERSE)
        .mode("robust", ROBUST)
        .window(400_000);
    m.prompt("+terse +robust go").expect("activation");

    m.window(9_958);
    let out = m.start("compact").expect("a restatement");
    assert!(out.contains("was just compacted"), "{out}");
    assert!(out.contains("## Suspend"), "in full: {out}");
    assert!(out.contains("Solve the class."), "{out}");
    assert_eq!(m.state().expect("state")["generation"], 1);
}

#[test]
fn a_compaction_does_not_re_arm_a_spent_edge() {
    let m = Machine::new();
    m.mode("paced", PACED).window(600_000);
    m.prompt("+paced go")
        .expect("the mark is already behind us");
    m.window(9_958);
    m.start("compact").expect("a restatement");
    m.window(600_000);
    assert_eq!(m.batch(), None);
}

#[test]
fn a_compaction_with_nothing_active_says_nothing() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(9_958);
    assert_eq!(m.start("compact"), None);
}

// ── session lifecycle ───────────────────────────────────────────────────────

#[test]
fn clearing_forgets_the_modes_spoken_into_the_old_context() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");
    assert!(m.state().is_some());
    assert_eq!(m.start("clear"), None);
    assert_eq!(m.state(), None);
}

#[test]
fn resuming_rebuilds_from_the_transcript_and_says_nothing() {
    let m = Machine::new();
    m.mode("terse", TERSE).said("+terse fix the parser");
    assert_eq!(
        m.start("resume"),
        None,
        "the restored context still holds it"
    );
    let state = m.state().expect("state rebuilt");
    assert_eq!(state["modes"][0]["name"], "terse");
    assert_eq!(state["modes"][0]["fires"], 0);
}

#[test]
fn a_rebuild_ignores_a_token_that_names_no_mode() {
    let m = Machine::new();
    m.said("+nosuchmode fix the parser");
    assert_eq!(m.start("resume"), None);
    assert_eq!(m.state(), None, "nothing to remember");
}

#[test]
fn a_compaction_with_no_state_rebuilds_before_restating() {
    let m = Machine::new();
    m.mode("terse", TERSE).said("+terse fix the parser");
    let out = m.start("compact").expect("a restatement");
    assert!(out.contains("Say few words."), "{out}");
}

// ── the gates ───────────────────────────────────────────────────────────────

#[test]
fn a_subagent_is_never_answered() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    let mut payload = m.base("UserPromptSubmit");
    payload["prompt"] = json!("+terse go");
    payload["agent_id"] = json!("sub-1");
    assert_eq!(m.hook(&payload), None);
    assert_eq!(m.state(), None);
}

#[test]
fn an_event_this_does_not_answer_is_ignored() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    let mut payload = m.base("PreToolUse");
    payload["prompt"] = json!("+terse go");
    assert_eq!(m.hook(&payload), None);
}

#[test]
fn a_session_id_that_is_a_path_is_refused() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    let mut payload = m.base("UserPromptSubmit");
    payload["prompt"] = json!("+terse go");
    payload["session_id"] = json!("../escape");
    assert_eq!(m.hook(&payload), None);
}

// ── failing open ────────────────────────────────────────────────────────────

#[test]
fn an_empty_or_malformed_payload_is_silent_and_clean() {
    let m = Machine::new();
    for input in [
        "",
        "{",
        "null",
        "[]",
        r#"{"hook_event_name":"UserPromptSubmit"}"#,
    ] {
        let out = m
            .cmd()
            .arg("hook")
            .write_stdin(input)
            .output()
            .expect("run");
        assert!(out.status.success(), "{input:?} must exit clean");
        assert!(out.stdout.is_empty(), "{input:?} must say nothing");
    }
}

#[test]
fn corrupt_state_is_rebuilt_rather_than_fatal() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");
    fs::write(m.root.join("s-1.json"), "{not json").expect("corrupt it");
    let out = m.prompt("+terse again").expect("it activates afresh");
    assert!(out.contains("Say few words."), "{out}");
}

#[test]
fn a_mode_that_does_not_read_is_skipped_rather_than_fatal() {
    let m = Machine::new();
    m.mode("broken", "---\ntriggers:\n  - whenever: 1\n---\n\nBody.\n")
        .window(1_000);
    assert_eq!(m.prompt("+broken go"), None);
}

#[test]
fn a_missing_transcript_does_not_stop_an_activation() {
    let m = Machine::new();
    m.mode("terse", TERSE);
    let out = m.prompt("+terse go").expect("an injection");
    assert!(out.contains("Say few words."), "{out}");
}

#[test]
fn a_mode_file_removed_mid_session_leaves_the_mode_switched_on() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");
    fs::remove_file(m.home.join(".claude/modes/terse.md")).expect("remove it");
    m.window(40_000);
    assert_eq!(m.batch(), None);
    assert_eq!(m.state().expect("state")["modes"][0]["name"], "terse");
    m.mode("terse", TERSE);
    assert!(m.batch().is_some(), "putting it back resumes it");
}

// ── the reading surfaces ────────────────────────────────────────────────────

#[test]
fn list_reports_every_mode_and_names_a_broken_one() {
    let m = Machine::new();
    m.mode("terse", TERSE)
        .mode("broken", "---\nrefrain: |\n  two\n  lines\n---\n\nBody.\n");
    let out = m.cmd().arg("list").output().expect("run");
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert!(text.contains("terse"), "{text}");
    assert!(text.contains("every 25000 tokens"), "{text}");
    assert!(text.contains("BROKEN"), "{text}");
}

#[test]
fn explain_says_why_a_clause_has_not_fired() {
    let m = Machine::new();
    m.mode("terse", TERSE).mode("paced", PACED).window(1_000);
    m.prompt("+terse +paced go").expect("activation");
    let out = m.cmd().arg("explain").output().expect("run");
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert!(text.contains("due in 25000"), "{text}");
    assert!(text.contains("waiting — 499000 away"), "{text}");
}

#[test]
fn explain_without_state_says_so_rather_than_inventing_a_session() {
    let m = Machine::new();
    let out = m.cmd().arg("explain").output().expect("run");
    assert!(!out.status.success());
    assert!(out.stdout.is_empty(), "an error is never stdout");
}

#[test]
fn dry_run_shows_exactly_what_activation_would_inject() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    let out = m.cmd().args(["dry-run", "terse"]).output().expect("run");
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert!(text.contains("===== MODE: terse ====="), "{text}");
    assert!(text.contains("## Suspend"), "{text}");
    assert_eq!(m.state(), None, "and changes nothing");
}

#[test]
fn dry_run_on_an_unknown_token_fails_loudly() {
    let m = Machine::new();
    m.cmd().args(["dry-run", "nosuchmode"]).assert().failure();
}

#[test]
fn json_is_a_document_a_script_can_read() {
    let m = Machine::new();
    m.mode("terse", TERSE);
    let out = m.cmd().args(["list", "--json"]).output().expect("run");
    let parsed: Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(parsed[0]["name"], "terse");
    assert_eq!(parsed[0]["refrain"], "Cut filler.");
}

#[test]
fn doctor_names_a_mode_that_does_not_read() {
    let m = Machine::new();
    m.mode("terse", TERSE);
    m.cmd().arg("doctor").assert().success();
    m.mode("broken", "---\nwhat: 1\n---\n\nBody.\n");
    let out = m.cmd().arg("doctor").output().expect("run");
    assert!(!out.status.success());
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert!(text.contains("broken.md"), "{text}");
}

#[test]
fn gc_sweeps_only_what_it_said_it_would() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");

    let out = m
        .cmd()
        .args(["gc", "--days", "0", "-n"])
        .output()
        .expect("run");
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert!(text.contains("s-1"), "{text}");
    assert!(m.state().is_some(), "a dry run removes nothing");

    m.cmd().args(["gc", "--days", "0"]).assert().success();
    assert_eq!(m.state(), None);
    m.cmd().arg("gc").assert().success();
}

#[test]
fn the_binary_documents_itself() {
    let m = Machine::new();
    for args in [
        vec!["guide"],
        vec!["guide", "schedule"],
        vec!["help", "triggers"],
        vec!["help", "hooks"],
        vec!["completions", "zsh"],
    ] {
        m.cmd().args(&args).assert().success();
    }
    m.cmd().args(["guide", "nosuchtopic"]).assert().failure();
    m.cmd().args(["help", "nosuchtopic"]).assert().failure();
}

#[test]
fn a_project_mode_is_reachable_and_cannot_shadow_a_home_one() {
    let m = Machine::new();
    let project = m.home.join("work");
    fs::create_dir_all(project.join(".claude/modes")).expect("mkdir");
    fs::write(
        project.join(".claude/modes/local.md"),
        "# Local\n\nProject only.\n",
    )
    .expect("write");
    fs::write(
        project.join(".claude/modes/terse.md"),
        "# Shadow\n\nNot this one.\n",
    )
    .expect("write");
    m.mode("terse", TERSE).window(1_000);

    let mut payload = m.base("UserPromptSubmit");
    payload["prompt"] = json!("+local +terse go");
    payload["cwd"] = json!(project.to_str().expect("utf-8"));
    let out = m.hook(&payload).expect("an injection");
    assert!(out.contains("Project only."), "{out}");
    assert!(out.contains("Say few words."), "the home mode wins: {out}");
    assert!(!out.contains("Not this one."), "{out}");
}

#[test]
fn a_plugin_mode_resolves_by_its_bare_token() {
    let m = Machine::new();
    let plugin: &Path = &m.home.join(".claude/skills/attic/modes");
    fs::create_dir_all(plugin).expect("mkdir");
    fs::write(plugin.join("mr.md"), "# MR mode\n\nReview it.\n").expect("write");
    m.window(1_000);
    let out = m.prompt("+mr go").expect("an injection");
    assert!(out.contains("Review it."), "{out}");
}

// ── the shapes ──────────────────────────────────────────────────────────────

#[test]
fn every_shape_survives_an_empty_corpus_and_a_full_one() {
    let m = Machine::new();
    for shape in ["human", "agent", "json"] {
        let out = m
            .cmd()
            .args(["list", "--format", shape])
            .output()
            .expect("run");
        assert!(out.status.success(), "{shape} on an empty corpus");
        assert!(!out.stdout.is_empty(), "{shape} says something");
    }
    m.mode("terse", TERSE).mode("paced", PACED);
    for shape in ["human", "agent", "json"] {
        let out = m
            .cmd()
            .args(["list", "--format", shape])
            .output()
            .expect("run");
        let text = String::from_utf8(out.stdout).expect("utf-8");
        assert!(text.contains("terse"), "{shape}: {text}");
        assert!(text.contains("500000"), "{shape}: {text}");
    }
}

#[test]
fn explain_reads_in_every_shape() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");
    for shape in ["human", "agent", "json"] {
        let out = m
            .cmd()
            .args(["explain", "--format", shape, "--session", "s-1"])
            .output()
            .expect("run");
        assert!(out.status.success(), "{shape}");
        let text = String::from_utf8(out.stdout).expect("utf-8");
        assert!(text.contains("terse"), "{shape}: {text}");
    }
    let out = m.cmd().args(["explain", "--json"]).output().expect("run");
    let parsed: Value = serde_json::from_slice(&out.stdout).expect("JSON");
    assert_eq!(parsed["session"], "s-1");
    assert_eq!(parsed["tokens"], 1_000);
    assert_eq!(parsed["clauses"][0]["mode"], "terse");
}

#[test]
fn explain_reports_a_mode_whose_file_is_gone() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");
    fs::remove_file(m.home.join(".claude/modes/terse.md")).expect("remove it");
    let out = m.cmd().arg("explain").output().expect("run");
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert!(text.contains("gone"), "{text}");
}

#[test]
fn explain_says_so_when_a_mode_stopped_reading_mid_session() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");
    m.mode("terse", "---\nnonsense: 1\n---\n\nBody.\n");
    let out = m.cmd().arg("explain").output().expect("run");
    let text = String::from_utf8(out.stdout).expect("utf-8");
    assert!(text.contains("no longer reads"), "{text}");
}

#[test]
fn colour_is_a_flag_and_not_a_guess() {
    let m = Machine::new();
    m.mode("terse", TERSE).window(1_000);
    m.prompt("+terse go").expect("activation");
    let plain = m
        .cmd()
        .args(["list", "--format", "human", "--color", "never"])
        .output()
        .expect("run");
    let painted = m
        .cmd()
        .args(["list", "--format", "human", "--color", "always"])
        .output()
        .expect("run");
    let plain = String::from_utf8(plain.stdout).expect("utf-8");
    let painted = String::from_utf8(painted.stdout).expect("utf-8");
    assert!(!plain.contains('\u{1b}'), "no escapes when asked for none");
    assert!(painted.contains('\u{1b}'), "escapes when asked for them");
    let painted = m
        .cmd()
        .args(["explain", "--format", "human", "--color", "always"])
        .output()
        .expect("run");
    assert!(
        String::from_utf8(painted.stdout)
            .expect("utf-8")
            .contains('\u{1b}')
    );
}

#[test]
fn a_project_override_resolves_modes_from_somewhere_else() {
    let m = Machine::new();
    let project = m.home.join("work");
    fs::create_dir_all(project.join(".claude/modes")).expect("mkdir");
    fs::write(
        project.join(".claude/modes/local.md"),
        "# Local\n\nProject only.\n",
    )
    .expect("write");
    let out = m
        .cmd()
        .args(["list", "--project", project.to_str().expect("utf-8")])
        .output()
        .expect("run");
    assert!(
        String::from_utf8(out.stdout)
            .expect("utf-8")
            .contains("local")
    );
}
