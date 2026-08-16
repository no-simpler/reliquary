use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{env, fs};

/// A temporary corpus and the binary that acts on it. Every invocation is
/// pinned to `MIDDEN_ROOT` and to agent-shaped output, so no test can reach the
/// real corpus or depend on a terminal.
struct Midden {
    base: PathBuf,
    root: PathBuf,
}

impl Midden {
    fn new() -> Midden {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = format!(
            "midden-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let base = env::temp_dir().join(unique);
        fs::create_dir_all(&base).expect("creating the test directory");
        // Canonical, because the CLI resolves every path it is handed, and the
        // paths it prints have to match the ones a test builds by hand.
        let base = fs::canonicalize(&base).expect("canonicalising the test directory");
        let root = base.join("corpus");
        fs::create_dir_all(&root).expect("creating the corpus root");
        Midden { base, root }
    }

    fn run(&self, args: &[&str]) -> Run {
        let output = Command::new(env!("CARGO_BIN_EXE_midden"))
            .args(args)
            .current_dir(&self.base)
            .env("MIDDEN_ROOT", &self.root)
            .env("CLAUDECODE", "1")
            .env("CLAUDE_CODE_SESSION_ID", "test-session")
            .env("HOME", &self.base)
            .env_remove("MIDDEN_UI")
            .output()
            .expect("running the midden binary");
        Run { output }
    }

    /// Files a note and returns its id.
    fn file(&self, kind: &str, title: &str, extra: &[&str]) -> String {
        let mut args = vec!["file", "--kind", kind, "--title", title];
        args.extend_from_slice(extra);
        self.run(&args).ok().stdout().trim().to_owned()
    }

    fn path_of(&self, id: &str) -> PathBuf {
        PathBuf::from(self.run(&["path", id]).ok().stdout().trim())
    }

    fn read(&self, id: &str) -> String {
        fs::read_to_string(self.path_of(id)).expect("reading the note")
    }

    /// Backdates a note's clock, so retention can be exercised without waiting
    /// for it.
    fn backdate(&self, id: &str, days: i64) {
        let path = self.path_of(id);
        let text = fs::read_to_string(&path).expect("reading the note");
        let then = jiff_ago(days);
        let rewritten: String = text
            .lines()
            .map(|line| {
                if let Some(key) = ["created:", "updated:"]
                    .into_iter()
                    .find(|key| line.starts_with(key))
                {
                    format!("{key} {then}")
                } else {
                    line.to_owned()
                }
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(&path, format!("{rewritten}\n")).expect("writing the note");
    }

    fn live(&self) -> Vec<String> {
        names_in(&self.root.join("notes"))
    }

    fn archived(&self) -> Vec<String> {
        names_in(&self.root.join("archive"))
    }
}

impl Drop for Midden {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

fn names_in(dir: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .flatten()
        .filter(|entry| entry.path().extension().and_then(|e| e.to_str()) == Some("md"))
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

/// An RFC 3339 instant that many whole days in the past, in the shape the
/// metadata is written in.
fn jiff_ago(days: i64) -> String {
    (jiff::Timestamp::now() - jiff::SignedDuration::from_hours(24 * days))
        .round(jiff::Unit::Second)
        .expect("rounding to the second")
        .to_string()
}

struct Run {
    output: Output,
}

impl Run {
    fn ok(self) -> Run {
        assert!(
            self.output.status.success(),
            "expected success\nstdout: {}\nstderr: {}",
            self.stdout(),
            self.stderr()
        );
        self
    }

    fn fails(self) -> Run {
        assert!(
            !self.output.status.success(),
            "expected failure\nstdout: {}",
            self.stdout()
        );
        self
    }

    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    fn all(&self) -> String {
        format!("{}{}", self.stdout(), self.stderr())
    }
}

#[test]
fn a_filed_note_lands_in_the_corpus_with_what_it_could_capture() {
    let m = Midden::new();
    let id = m.file(
        "gap",
        "yadm wrapper resolution is unstated",
        &["--target", "~/.config/CLAUDE.md", "--detail", "Guessed."],
    );

    assert_eq!(m.live().len(), 1);
    let text = m.read(&id);
    assert!(text.contains("kind: gap"), "{text}");
    assert!(text.contains("status: open"), "{text}");
    assert!(text.contains("occurrences: 1"), "{text}");
    assert!(text.contains("session: test-session"), "{text}");
    assert!(text.contains("target: ~/.config/CLAUDE.md"), "{text}");
}

#[test]
fn the_same_cause_folds_instead_of_multiplying() {
    let m = Midden::new();
    let first = m.file(
        "hunt",
        "Brewfile scopes are undocumented",
        &["--target", "a.md"],
    );
    // Different wording, different case, a trailing separator on the target.
    let again = m.run(&[
        "file",
        "--kind",
        "hunt",
        "--title",
        "the brewfile scopes are undocumented.",
        "--target",
        "A.md/",
    ]);
    let again = again.ok();

    assert_eq!(again.stdout().trim(), first);
    assert!(again.stderr().contains("folded into"), "{}", again.stderr());
    assert_eq!(m.live().len(), 1);
    assert!(m.read(&first).contains("occurrences: 2"));
}

#[test]
fn a_different_kind_or_target_is_a_different_cause() {
    let m = Midden::new();
    m.file("hunt", "same claim", &["--target", "a.md"]);
    m.file("gap", "same claim", &["--target", "a.md"]);
    m.file("hunt", "same claim", &["--target", "b.md"]);
    m.file("hunt", "same claim", &[]);
    assert_eq!(m.live().len(), 4);
}

#[test]
fn recurrence_reopens_an_actioned_note_and_leaves_a_dismissal_alone() {
    let m = Midden::new();
    let actioned = m.file("gap", "one", &[]);
    let dismissed = m.file("gap", "two", &[]);
    m.run(&["resolve", &actioned, "--actioned"]).ok();
    m.run(&["resolve", &dismissed, "--dismissed"]).ok();

    m.file("gap", "one", &[]);
    m.file("gap", "two", &[]);

    assert!(m.read(&actioned).contains("status: open"));
    assert!(m.read(&dismissed).contains("status: dismissed"));
}

#[test]
fn an_archived_cause_that_returns_gets_a_fresh_note() {
    let m = Midden::new();
    let first = m.file("rework", "the same cause", &[]);
    m.run(&["archive", &first]).ok();
    let second = m.file("rework", "the same cause", &[]);

    assert_ne!(first, second);
    assert_eq!(m.live().len(), 1);
    assert_eq!(m.archived().len(), 1);
}

#[test]
fn field_caps_refuse_with_the_flag_that_needs_retyping() {
    let m = Midden::new();
    let long = "x".repeat(73);
    let run = m.run(&["file", "--kind", "gap", "--title", &long]).fails();
    assert!(run.stderr().contains("--title"), "{}", run.stderr());
    assert!(run.stderr().contains("73 characters"), "{}", run.stderr());

    let body = "y".repeat(1201);
    let run = m
        .run(&["file", "--kind", "gap", "--title", "fine", "--body", &body])
        .fails();
    assert!(run.stderr().contains("1201 bytes"), "{}", run.stderr());
    assert!(m.live().is_empty());
}

#[test]
fn a_kind_outside_the_taxonomy_is_refused() {
    let m = Midden::new();
    m.run(&["file", "--kind", "annoyance", "--title", "x"])
        .fails();
    assert!(m.live().is_empty());
}

#[test]
fn body_bytes_survive_a_metadata_rewrite() {
    let m = Midden::new();
    let id = m.file(
        "rebuff",
        "staged only some of the modified lines",
        &[
            "--body",
            "The user said: stage every M line.\n\n  indented   spacing",
        ],
    );
    let before = m.read(&id);
    let body_before = before.split("---\n").nth(2).expect("a body").to_owned();

    m.run(&["set", &id, "--detail", "House policy is bundle everything."])
        .ok();

    let after = m.read(&id);
    let body_after = after.split("---\n").nth(2).expect("a body");
    assert_eq!(body_before, body_after);
    assert!(after.contains("House policy"));
}

#[test]
fn show_prints_the_evidence_and_nothing_else() {
    let m = Midden::new();
    let id = m.file(
        "stale",
        "a path that moved",
        &["--body", "It pointed at X."],
    );
    let run = m.run(&["show", &id]).ok();
    assert_eq!(run.stdout(), "It pointed at X.\n");
}

#[test]
fn re_filing_a_claim_onto_an_existing_one_is_refused() {
    let m = Midden::new();
    let first = m.file("gap", "claim one", &[]);
    let second = m.file("gap", "claim two", &[]);
    let run = m.run(&["set", &second, "--title", "claim one"]).fails();
    assert!(run.stderr().contains(&first), "{}", run.stderr());
}

#[test]
fn an_invalid_note_stays_listed_and_fails_doctor() {
    let m = Midden::new();
    let id = m.file("gap", "readable", &[]);
    let path = m.path_of(&id);
    fs::write(&path, "---\nkind: gap\ntitle: readable\n---\nbody\n").expect("corrupting the note");

    let listing = m.run(&["list"]).ok();
    assert!(listing.stdout().contains(&id), "{}", listing.stdout());
    assert!(listing.stdout().contains("INVALID"), "{}", listing.stdout());

    let doctor = m.run(&["doctor"]).fails();
    assert!(doctor.stdout().contains("invalid"), "{}", doctor.stdout());
}

#[test]
fn doctor_reports_a_target_that_is_no_longer_there() {
    let m = Midden::new();
    fs::write(m.base.join("present.md"), "x").expect("writing a target");
    m.file(
        "stale",
        "points at what is there",
        &["--target", "~/present.md"],
    );
    m.file(
        "stale",
        "points at what is gone",
        &["--target", "~/absent.md"],
    );

    let doctor = m.run(&["doctor"]).fails();
    assert!(doctor.stdout().contains("moved"), "{}", doctor.stdout());
    assert!(doctor.stdout().contains("absent.md"), "{}", doctor.stdout());
    assert!(
        !doctor.stdout().contains("present.md"),
        "{}",
        doctor.stdout()
    );
}

#[test]
fn a_clean_corpus_passes_doctor() {
    let m = Midden::new();
    m.file(
        "friction",
        "a prompt on every yadm call",
        &["--target", "settings.json"],
    );
    m.run(&["doctor"]).ok();
}

#[test]
fn gc_holds_the_retention_boundaries() {
    let m = Midden::new();

    let fresh_dismissed = m.file("gap", "dismissed today", &[]);
    m.run(&["resolve", &fresh_dismissed, "--dismissed"]).ok();

    let old_dismissed = m.file("gap", "dismissed long ago", &[]);
    m.run(&["resolve", &old_dismissed, "--dismissed"]).ok();
    m.backdate(&old_dismissed, 40);

    let old_actioned = m.file("gap", "actioned long ago", &[]);
    m.run(&["resolve", &old_actioned, "--actioned"]).ok();
    m.backdate(&old_actioned, 120);

    let recent_actioned = m.file("gap", "actioned recently", &[]);
    m.run(&["resolve", &recent_actioned, "--actioned"]).ok();
    m.backdate(&recent_actioned, 89);

    let quiet_singleton = m.file("hunt", "seen once, long ago", &[]);
    m.backdate(&quiet_singleton, 200);

    let quiet_repeat = m.file("hunt", "seen twice, long ago", &[]);
    m.file("hunt", "seen twice, long ago", &[]);
    m.backdate(&quiet_repeat, 200);

    let before = m.live().len();
    let dry = m.run(&["gc", "--dry-run"]).ok();
    assert_eq!(m.live().len(), before, "a dry run must change nothing");
    assert!(dry.stdout().contains(&old_dismissed), "{}", dry.stdout());

    m.run(&["gc"]).ok();

    let live = m.live().join(" ");
    assert!(live.contains(&fresh_dismissed), "a fresh dismissal stays");
    assert!(!live.contains(&old_dismissed), "a spent dismissal goes");
    assert!(!live.contains(&old_actioned), "a spent fix goes");
    assert!(live.contains(&recent_actioned), "a recent fix stays");
    assert!(live.contains(&quiet_repeat), "a recurring cause stays");
    assert!(
        !live.contains(&quiet_singleton),
        "a quiet singleton is retired"
    );
    assert!(
        m.archived().join(" ").contains(&quiet_singleton),
        "retired, not deleted"
    );
}

#[test]
fn listing_shows_what_is_open_and_filters_narrow_it() {
    let m = Midden::new();
    let open = m.file("gap", "still open", &[]);
    let closed = m.file("friction", "already handled", &[]);
    m.run(&["resolve", &closed, "--actioned"]).ok();

    let plain = m.run(&["list"]).ok().stdout();
    assert!(plain.contains(&open));
    assert!(!plain.contains(&closed));

    assert!(m.run(&["list", "--all"]).ok().stdout().contains(&closed));
    assert!(
        m.run(&["list", "--status", "actioned"])
            .ok()
            .stdout()
            .contains(&closed)
    );
    assert!(
        !m.run(&["list", "--kind", "gap"])
            .ok()
            .stdout()
            .contains(&closed)
    );
}

#[test]
fn digest_groups_by_where_the_fix_would_land() {
    let m = Midden::new();
    m.file("gap", "first claim", &["--target", "heavy.md"]);
    m.file("gap", "second claim", &["--target", "heavy.md"]);
    m.file("hunt", "third claim", &["--target", "light.md"]);
    m.file("rework", "unplaced claim", &[]);

    let out = m.run(&["digest"]).ok().stdout();
    let heavy = out.find("heavy.md").expect("the heavy group");
    let light = out.find("light.md").expect("the light group");
    let none = out.find("(no target)").expect("the unplaced group");
    assert!(heavy < light, "heaviest group first\n{out}");
    assert!(light < none, "unplaced last\n{out}");
    assert!(out.contains("[2 notes]"), "{out}");
}

#[test]
fn json_is_available_for_every_listing() {
    let m = Midden::new();
    m.file("gap", "a claim", &["--target", "somewhere.md"]);

    for args in [vec!["list", "--json"], vec!["digest", "--json"]] {
        let out = m.run(&args).ok().stdout();
        let value: serde_json::Value = serde_json::from_str(&out).expect("valid json");
        assert!(value.is_object(), "{out}");
    }
}

#[test]
fn doctrine_lives_only_in_the_guide() {
    let m = Midden::new();
    let guide = m.run(&["guide", "file", "drain"]).ok().stdout();
    assert!(guide.contains("No quote, no note"), "{guide}");
    assert!(guide.contains("Resolving is not bookkeeping"), "{guide}");

    let help = m.run(&["help"]).ok().all();
    assert!(!help.contains("No quote, no note"), "{help}");
    assert!(!help.contains("Resolving is not bookkeeping"), "{help}");
}

#[test]
fn help_serves_topics_and_commands_from_one_verb() {
    let m = Midden::new();
    assert!(
        m.run(&["help", "metadata"])
            .ok()
            .stdout()
            .contains("fingerprint")
    );
    assert!(
        m.run(&["help", "retention"])
            .ok()
            .stdout()
            .contains("30 days")
    );
    assert!(m.run(&["help", "file"]).ok().stdout().contains("--kind"));
    m.run(&["help", "nonsense"]).fails();
}

#[test]
fn the_tool_describes_itself_before_a_corpus_exists() {
    let m = Midden::new();
    fs::remove_dir_all(&m.root).expect("removing the corpus root");
    m.run(&["guide"]).ok();
    m.run(&["help"]).ok();
    m.run(&["completions", "fish"]).ok();
}
