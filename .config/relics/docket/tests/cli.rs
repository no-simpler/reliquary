// Clippy's in-test carve-outs (see `clippy.toml`) reach `#[test]` functions and
// `#[cfg(test)]` modules — not the helpers beside them. An integration test crate
// is test code end to end, so the carve-out belongs at its root, where its scope
// is still exactly the tests.
#![allow(clippy::expect_used, clippy::panic, clippy::indexing_slicing)]

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

use assert_cmd::Command;
use assert_cmd::assert::{Assert, OutputAssertExt};
use predicates::str::contains;
use tempfile::TempDir;

/// A temporary depot and the binary that acts on it. Every invocation is pinned
/// to `DOCKET_ROOT` and to agent-shaped output, so no test can reach the real
/// depot or depend on a terminal.
struct Docket {
    /// Held for its `Drop`: the tree lives exactly as long as the fixture.
    _dir: TempDir,
    base: PathBuf,
    root: PathBuf,
}

impl Docket {
    fn new() -> Docket {
        let dir = tempfile::Builder::new()
            .prefix("docket-tests-")
            .tempdir()
            .expect("creating the test directory");
        // Canonical, because the CLI resolves every path it is handed, and the
        // paths it prints have to match the ones a test builds by hand — and on
        // macOS the temporary root is itself a symlink.
        let base = fs::canonicalize(dir.path()).expect("canonicalising the test directory");
        let root = base.join("depot");
        fs::create_dir_all(&root).expect("creating the depot root");
        Docket {
            _dir: dir,
            base,
            root,
        }
    }

    /// A project directory that deliberately does not exist: items are written
    /// for a path, never into it, so tests never depend on the machine's tree.
    fn project(&self, name: &str) -> String {
        self.base
            .join(name)
            .to_str()
            .expect("utf-8 path")
            .to_owned()
    }

    fn run(&self, args: &[&str]) -> Run {
        self.invoke(&self.base, args, &[])
    }

    /// The same invocation with extra environment, for the seams: `RELIC_GIT`
    /// set to nothing takes the ungit path.
    fn run_with(&self, args: &[&str], env: &[(&str, &str)]) -> Run {
        self.invoke(&self.base, args, env)
    }

    /// From a working directory of its own, which is what exercises keying:
    /// every other invocation passes --project and never asks git anything.
    fn run_in(&self, cwd: &Path, args: &[&str]) -> Run {
        self.invoke(cwd, args, &[])
    }

    /// Drives git against a tree the test built, never against the depot.
    fn git(&self, cwd: &Path, args: &[&str]) {
        Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .env("HOME", &self.base)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "docket tests")
            .env("GIT_AUTHOR_EMAIL", "tests@localhost")
            .env("GIT_COMMITTER_NAME", "docket tests")
            .env("GIT_COMMITTER_EMAIL", "tests@localhost")
            .assert()
            .success();
    }

    fn invoke(&self, cwd: &Path, args: &[&str], env: &[(&str, &str)]) -> Run {
        let mut command = Command::new(env!("CARGO_BIN_EXE_docket"));
        command
            .args(args)
            .current_dir(cwd)
            .env("DOCKET_ROOT", &self.root)
            .env("CLAUDECODE", "1")
            // HOME is where `doctor` looks for the session-start hook, and
            // where git would look for a global config, so it points at the
            // temporary tree as well.
            .env("HOME", &self.base)
            .env_remove("DOCKET_UI")
            .env_remove("RELIC_GIT");
        for (key, value) in env {
            command.env(key, value);
        }
        Run {
            output: command.output().expect("running the docket binary"),
        }
    }

    /// The depot's commit subjects, newest first.
    fn history(&self) -> Vec<String> {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["log", "--pretty=format:%s"])
            .env("HOME", &self.base)
            .output()
            .expect("reading the depot history");
        if !output.status.success() {
            return Vec::new();
        }
        String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Every path any commit ever removed.
    fn removed_paths(&self) -> String {
        let output = Command::new("git")
            .arg("-C")
            .arg(&self.root)
            .args(["log", "--diff-filter=D", "--name-only", "--pretty=format:"])
            .env("HOME", &self.base)
            .output()
            .expect("reading the depot history");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    /// Opens an item with a body, and returns its id and the path `create`
    /// printed for it.
    fn create(&self, project: &str, kind: &str, name: &str) -> (String, PathBuf) {
        let body = format!("Body of {name}.\n");
        let run = self.run(&[
            "create",
            kind,
            "--name",
            name,
            "--tagline",
            "What a future session reads first.",
            "--body",
            &body,
            "--project",
            project,
            "--allow-missing",
            "-q",
        ]);
        run.assert().success();
        id_and_path(&run.stdout())
    }

    /// The same, with a tagline and a body of the test's own, for the searches
    /// that have to tell one field from another.
    fn write(
        &self,
        project: &str,
        kind: &str,
        name: &str,
        tagline: &str,
        body: &str,
    ) -> (String, PathBuf) {
        let run = self.run(&[
            "create",
            kind,
            "--name",
            name,
            "--tagline",
            tagline,
            "--body",
            body,
            "--project",
            project,
            "--allow-missing",
            "-q",
        ]);
        run.assert().success();
        id_and_path(&run.stdout())
    }

    fn tag(&self, id: &str, tags: &str) {
        let run = self.run(&["set", id, "--tags", tags, "-q"]);
        run.assert().success();
    }
}

struct Run {
    output: std::process::Output,
}

impl Run {
    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    /// The status assertion. A failure prints the invocation, both streams and
    /// the code, so no call site has to carry a message of its own.
    fn assert(&self) -> Assert {
        self.output.clone().assert()
    }
}

/// `create` and `relay` both print `<id>\t<path>`.
fn id_and_path(stdout: &str) -> (String, PathBuf) {
    let line = stdout.strip_suffix('\n').unwrap_or(stdout);
    let (id, path) = line
        .split_once('\t')
        .unwrap_or_else(|| panic!("expected `<id>\\t<path>`, got {line:?}"));
    assert_eq!(id.len(), 4, "an id is four characters");
    (id.to_owned(), PathBuf::from(path))
}

/// `promote` prints `<id>\t<kind badge>\t<path>`.
fn promoted_path(stdout: &str) -> PathBuf {
    let line = stdout.strip_suffix('\n').unwrap_or(stdout);
    let (_, path) = line
        .rsplit_once('\t')
        .unwrap_or_else(|| panic!("expected `<id>\\t<kind>\\t<path>`, got {line:?}"));
    PathBuf::from(path)
}

/// The last `depth` components of a path, so an assertion can say where a file
/// landed without spelling out a temporary directory.
fn tail(path: &Path, depth: usize) -> String {
    let mut parts: Vec<String> = path
        .components()
        .rev()
        .take(depth)
        .map(|part| part.as_os_str().to_string_lossy().into_owned())
        .collect();
    parts.reverse();
    parts.join("/")
}

/// An item's metadata, without its body.
fn front(path: &Path) -> String {
    let text = fs::read_to_string(path).expect("reading the item");
    let rest = text
        .strip_prefix("---\n")
        .expect("the file opens with a `---` line");
    let end = rest.find("\n---\n").expect("the metadata is terminated");
    rest.get(..=end).unwrap_or(rest).to_owned()
}

/// Ids in listing order, taken from the numbered lines of an agent-shaped
/// listing so a tagline can never be mistaken for a row.
fn listed_ids(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with("docket "))
        .filter_map(|line| {
            // A listing across projects names each row's project ahead of the
            // cells every listing shows, so the position is not always first.
            let fields: Vec<&str> = line.split_whitespace().collect();
            let at = fields
                .iter()
                .position(|field| *field == "!" || field.parse::<usize>().is_ok())?;
            fields.get(at + 1).map(|id| (*id).to_owned())
        })
        .collect()
}

/// The positions a listing printed, in the order it printed them.
fn listed_positions(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| !line.starts_with(' ') && !line.starts_with("docket "))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split_whitespace().collect();
            let at = fields
                .iter()
                .position(|field| *field == "!" || field.parse::<usize>().is_ok())?;
            Some(fields[at].to_owned())
        })
        .collect()
}

/// The indented lines a listing hung under its rows.
fn notes(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| line.starts_with(' '))
        .map(|line| line.trim().to_owned())
        .collect()
}

/// Whether metadata carries one key at one value. An id of nothing but digits
/// is emitted quoted, because YAML would otherwise read it back as a number —
/// so an assertion that spells the pair out by hand fails on roughly one id in
/// a hundred.
fn holds(front: &str, key: &str, value: &str) -> bool {
    front
        .lines()
        .filter_map(|line| line.strip_prefix(&format!("{key}: ")))
        .any(|found| found.trim_matches(['\'', '"']) == value)
}

/// One metadata value, for the keys an assertion has to carry across a command
/// rather than spell out.
fn value(front: &str, key: &str) -> String {
    front
        .lines()
        .find_map(|line| line.strip_prefix(&format!("{key}: ")))
        .unwrap_or_else(|| panic!("{key} is written — {front}"))
        .trim_matches(['\'', '"'])
        .to_owned()
}

/// Nothing an item occupied survives closing — the file, and a spec's whole
/// directory with it.
fn footprint_gone(path: &Path) -> bool {
    if path.file_name() == Some(OsStr::new("spec.md")) {
        return path.parent().is_none_or(|dir| !dir.exists());
    }
    !path.exists()
}

/// Replaces the line carrying one metadata key, which is how an item written by
/// an older version of the tool differs from one written now.
fn swap_key(path: &Path, key: &str, line: &str) {
    let text = fs::read_to_string(path).expect("reading the item");
    let prefix = format!("{key}:");
    let mut found = false;
    let swapped: String = text
        .lines()
        .map(|existing| {
            if existing.starts_with(&prefix) {
                found = true;
                format!("{line}\n")
            } else {
                format!("{existing}\n")
            }
        })
        .collect();
    assert!(found, "{key} is not in {}", path.display());
    fs::write(path, swapped).expect("rewriting the item");
}

/// Deletes one metadata key, which is how an item falls out of schema in the
/// wild.
fn drop_key(path: &Path, key: &str) {
    let text = fs::read_to_string(path).expect("reading the item");
    let mut kept = String::new();
    for line in text.lines().filter(|line| !line.starts_with(key)) {
        kept.push_str(line);
        kept.push('\n');
    }
    fs::write(path, kept).expect("rewriting the item");
}

#[test]
fn creating_each_kind_lands_in_its_own_directory() {
    let docket = Docket::new();
    let project = docket.project("proj");

    let (handoff, handoff_path) = docket.create(&project, "handoff", "SETTLE_INTENT");
    let (relay, relay_path) = docket.create(&project, "relay", "CARRY_CHAIN");
    let (spec, spec_path) = docket.create(&project, "spec", "ROSETTA_MESSENGER");

    assert_eq!(
        tail(&handoff_path, 2),
        format!("handoffs/{handoff}-SETTLE_INTENT.md")
    );
    assert_eq!(
        tail(&relay_path, 2),
        format!("relays/{relay}-CARRY_CHAIN.md")
    );
    assert_eq!(
        tail(&spec_path, 3),
        format!("specs/{spec}-ROSETTA_MESSENGER/spec.md")
    );
    for path in [&handoff_path, &relay_path, &spec_path] {
        assert!(path.is_file(), "{} was not written", path.display());
    }
}

#[test]
fn creating_for_a_missing_target_demands_allow_missing() {
    let docket = Docket::new();
    let missing = docket.project("not-on-disk");

    let run = docket.run(&[
        "create",
        "handoff",
        "--name",
        "SETTLE_INTENT",
        "--tagline",
        "Two candidates, neither committed to.",
        "--to",
        &missing,
        "-q",
    ]);

    run.assert().failure();
    run.assert().failure().stderr(contains("--allow-missing"));
}

/// Longer than any limit, so one value serves every over-length assertion.
const TOO_LONG: &str = "\
Far past every limit this tool enforces, and written as prose because prose is \
exactly what does not belong in a field that has to be skimmed in one glance.";

#[test]
fn creating_rejects_a_malformed_name_or_an_overlong_tagline() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let create = |name: &str, tagline: &str| {
        docket.run(&[
            "create",
            "handoff",
            "--name",
            name,
            "--tagline",
            tagline,
            "--project",
            &project,
            "--allow-missing",
            "-q",
        ])
    };

    for (name, tagline, expected) in [
        ("   ", "A tagline.", "--name is required"),
        ("A_NAME", "", "--tagline is required"),
        ("one two three four", "A tagline.", "is 4 words"),
        ("SOMETHING_RATHER_LONGER", "A tagline.", "the limit is 20"),
        ("A_NAME.md", "A tagline.", "drop the .md"),
        (TOO_LONG, "A tagline.", "A-Z, 0-9 and underscore"),
        ("A_NAME", TOO_LONG, "--tagline is 156 characters"),
    ] {
        let run = create(name, tagline);
        run.assert().failure().stderr(contains(expected));
    }
    create("A_NAME", "A tagline.").assert().success();
}

#[test]
fn a_name_is_stored_in_one_spelling_however_it_was_typed() {
    let docket = Docket::new();
    let project = docket.project("proj");

    for typed in ["dream residue", "dream-residue", "DREAM_RESIDUE"] {
        let run = docket.run(&[
            "create",
            "handoff",
            "--name",
            typed,
            "--tagline",
            "One spelling, whatever was typed.",
            "--project",
            &project,
            "--allow-missing",
            "-q",
        ]);
        run.assert().success();
        let (id, path) = id_and_path(&run.stdout());
        assert!(
            front(&path).contains("name: DREAM_RESIDUE\n"),
            "{typed:?} stored as {}",
            front(&path)
        );
        assert_eq!(tail(&path, 2), format!("handoffs/{id}-DREAM_RESIDUE.md"));
        // Only the first is closed, so the rest exercise the duplicate report.
        if typed == "DREAM_RESIDUE" {
            let doctor = docket.run(&["doctor"]);
            doctor.assert().stdout(contains("repeated DREAM_RESIDUE"));
        }
    }
}

#[test]
fn set_and_relay_hold_the_same_limits_as_create() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (handoff, _) = docket.create(&project, "handoff", "A_HANDOFF");
    let (relay, _) = docket.create(&project, "relay", "A_RELAY");

    for (flag, value, expected) in [
        ("--name", TOO_LONG, "A-Z, 0-9 and underscore"),
        ("--tagline", TOO_LONG, "--tagline is 156 characters"),
        ("--name", "  ", "--name is required"),
        ("--blocked", "  ", "--clear-blocked"),
        ("--blocked", TOO_LONG, "--blocked is 156 characters"),
    ] {
        let run = docket.run(&["set", &handoff, flag, value, "-q"]);
        run.assert().failure().stderr(contains(expected));
    }

    let run = docket.run(&[
        "relay",
        &relay,
        "--name",
        "A_SUCCESSOR",
        "--tagline",
        TOO_LONG,
        "-q",
    ]);
    run.assert()
        .failure()
        .stderr(contains("--tagline is 156 characters"));
    // The refusal happened before anything was written: the relay is intact.
    let listed = docket.run(&["list", "--project", &project, "-q"]);
    assert_eq!(listed_ids(&listed.stdout()), vec![handoff, relay]);
}

#[test]
fn a_wrapped_tagline_is_stored_as_one_line() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let run = docket.run(&[
        "create",
        "handoff",
        "--name",
        "A title",
        "--tagline",
        "Two   lines\nof it.",
        "--project",
        &project,
        "--allow-missing",
        "-q",
    ]);
    run.assert().success();
    let (_, path) = id_and_path(&run.stdout());
    assert!(
        front(&path).contains("tagline: Two lines of it.\n"),
        "{}",
        front(&path)
    );
}

#[test]
fn an_overlong_value_on_disk_loads_and_is_reported() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "handoff", "A_HANDOFF");
    swap_key(&path, "tagline", &format!("tagline: {TOO_LONG}"));

    // Lenient on the way in: length never keeps an item off a listing.
    let listed = docket.run(&["list", "--project", &project, "-q"]);
    listed.assert().success().stdout(contains(TOO_LONG));

    // Reported on the way past, with the command that fixes it.
    let doctor = docket.run(&["doctor"]);
    doctor.assert().failure().stdout(contains("overlong"));

    let set = docket.run(&["set", &id, "--tagline", "Short enough now.", "-q"]);
    set.assert().success();
    assert!(
        front(&path).contains("tagline: Short enough now.\n"),
        "{}",
        front(&path)
    );
}

/// Leniency covers values, never keys. A renamed key carries no alias, so an
/// item still holding the old one does not quietly load under two spellings —
/// it fails to parse, says which key it does not know, and is rebuilt by `set`.
#[test]
fn a_superseded_key_is_not_read() {
    let docket = Docket::new();
    let project = docket.project("proj");

    for (superseded, current, line) in [
        (
            "title",
            "name",
            "title: Migrate the four legacy conventions",
        ),
        (
            "description",
            "tagline",
            "description: What it used to say.",
        ),
    ] {
        let (id, path) = docket.create(&project, "handoff", "A_HANDOFF");
        swap_key(&path, current, line);

        // Still listed — parsing is total — but as invalid, naming the key.
        let listed = docket.run(&["list", "--project", &project, "-q"]);
        listed.assert().success();
        let stdout = listed.stdout();
        assert!(stdout.contains("INVALID"), "{stdout}");
        assert!(
            stdout.contains(superseded),
            "the error names the key: {stdout}"
        );

        let doctor = docket.run(&["doctor"]);
        doctor.assert().failure().stdout(contains("invalid"));

        let set = docket.run(&[
            "set",
            &id,
            "--name",
            "REBUILT",
            "--tagline",
            "Rebuilt under the current keys.",
            "-q",
        ]);
        set.assert().success();
        let front = front(&path);
        assert!(front.contains("name: REBUILT\n"), "{front}");
        assert!(
            !front.contains(&format!("{superseded}:")),
            "the old key survived: {front}"
        );

        docket.run(&["close", &id, "-q"]);
    }
}

#[test]
fn a_name_resolves_an_item_wherever_an_id_does() {
    let docket = Docket::new();
    let alpha = docket.project("alpha");
    let beta = docket.project("beta");
    let (id, path) = docket.create(&alpha, "handoff", "ROSETTA");

    // Normalised on the way in, so a name resolves however it was typed.
    for typed in ["ROSETTA", "rosetta", "Rosetta"] {
        let located = docket.run(&["path", typed]);
        located.assert().success();
        assert_eq!(located.stdout().trim_end(), path.to_str().unwrap());
    }

    let missing = docket.run(&["show", "NOT_OPEN"]);
    missing
        .assert()
        .failure()
        .stderr(contains("no open item named NOT_OPEN"));

    let nonsense = docket.run(&["show", "not an id or a name"]);
    nonsense
        .assert()
        .failure()
        .stderr(contains("neither an id nor a name"));

    // A name is not unique, so a second one is a refusal rather than a guess.
    let (twin, _) = docket.create(&beta, "spec", "ROSETTA");
    let ambiguous = docket.run(&["close", "ROSETTA", "-q"]);
    ambiguous.assert().failure();
    let stderr = ambiguous.stderr();
    for named in [&id, &twin] {
        assert!(stderr.contains(named.as_str()), "{stderr}");
    }
    assert!(path.exists(), "nothing was closed");

    // The id still discriminates, and the name resolves again once it is free.
    let closed = docket.run(&["close", &twin, "-q"]);
    closed.assert().success();
    let closed = docket.run(&["close", "ROSETTA", "-q"]);
    closed.assert().success();
    assert!(footprint_gone(&path));
}

#[test]
fn help_states_the_limits_it_enforces() {
    let docket = Docket::new();
    let metadata = docket.run(&["help", "metadata"]).stdout();
    assert!(metadata.contains("20"), "{metadata}");
    assert!(metadata.contains("80"), "{metadata}");
}

#[test]
fn every_guide_topic_prints_and_an_unknown_one_lists_them() {
    let docket = Docket::new();

    let root = docket.run(&["guide"]);
    root.assert().success();
    let root = root.stdout();
    assert!(root.contains("DOCKET"), "{root}");
    assert!(
        root.contains("docket help"),
        "the usage block is always last: {root}"
    );
    for absent in ["HANDOFF", "RELAY", "SPEC"] {
        assert!(!root.contains(absent), "unasked topic leaked: {root}");
    }

    for (topic, anchor) in [("handoff", "HANDOFF"), ("relay", "RELAY"), ("spec", "SPEC")] {
        let run = docket.run(&["guide", topic]);
        run.assert().success();
        let out = run.stdout();
        assert!(
            out.contains("DOCKET"),
            "every guide carries the frame: {out}"
        );
        let body = out.find(anchor).unwrap_or_else(|| panic!("{out}"));
        let usage = out
            .find("  docket create")
            .unwrap_or_else(|| panic!("{out}"));
        assert!(body < usage, "a topic sits above the usage block: {out}");
    }

    // Several topics render in canonical order, whatever order they were asked
    // for, so one guide always reads the same way.
    let many = docket.run(&["guide", "spec", "handoff"]);
    many.assert().success();
    let many = many.stdout();
    assert!(
        many.find("HANDOFF") < many.find("SPEC"),
        "topics are ordered by the ladder, not by the argument list: {many}"
    );

    let unknown = docket.run(&["guide", "nonsense"]);
    unknown.assert().failure();
    let stderr = unknown.stderr();
    for topic in ["handoff", "relay", "spec"] {
        assert!(stderr.contains(topic), "{stderr}");
    }
}

#[test]
fn doctrine_lives_only_in_the_guide() {
    let docket = Docket::new();

    let agent = docket.run(&["help", "agent"]);
    agent.assert().failure();

    let ladder = docket.run(&["help", "ladder"]).stdout();
    assert!(!ladder.contains("Reach for"), "{ladder}");
}

#[test]
fn ids_resolve_from_any_project() {
    let docket = Docket::new();
    let alpha = docket.project("alpha");
    let beta = docket.project("beta");

    let (first, first_path) = docket.create(&alpha, "handoff", "ALPHA_WORK");
    let (second, second_path) = docket.create(&beta, "spec", "BETA_WORK");
    assert_ne!(first, second);

    // No --project anywhere below: an id is enough to find an item.
    for (id, path, name) in [
        (&first, &first_path, "ALPHA_WORK"),
        (&second, &second_path, "BETA_WORK"),
    ] {
        let shown = docket.run(&["show", id]);
        shown.assert().success();
        assert_eq!(shown.stdout(), format!("Body of {name}.\n"));

        let located = docket.run(&["path", id]);
        located.assert().success();
        assert_eq!(located.stdout().trim_end(), path.to_str().unwrap());
    }
}

#[test]
fn promotion_climbs_the_ladder_and_stops_at_the_top() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, handoff_path) = docket.create(&project, "handoff", "CLIMB_LADDER");

    let to_relay = docket.run(&["promote", &id, "-q"]);
    to_relay.assert().success();
    let relay_path = promoted_path(&to_relay.stdout());
    assert_eq!(tail(&relay_path, 2), format!("relays/{id}-CLIMB_LADDER.md"));
    assert!(relay_path.is_file());
    assert!(!handoff_path.exists(), "the handoff file should have moved");

    let to_spec = docket.run(&["promote", &id, "-q"]);
    to_spec.assert().success();
    let spec_path = promoted_path(&to_spec.stdout());
    assert_eq!(
        tail(&spec_path, 3),
        format!("specs/{id}-CLIMB_LADDER/spec.md")
    );
    assert!(spec_path.is_file());
    assert!(!relay_path.exists(), "the relay file should have moved");

    let to_implementation = docket.run(&["promote", &id, "-q"]);
    to_implementation.assert().success();
    assert_eq!(promoted_path(&to_implementation.stdout()), spec_path);
    assert!(front(&spec_path).contains("stage: implementation"));

    let past_the_top = docket.run(&["promote", &id, "-q"]);
    past_the_top
        .assert()
        .failure()
        .stderr(contains("docket close"));
}

#[test]
fn promoting_a_handoff_straight_to_spec_carries_no_chain() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, _) = docket.create(&project, "handoff", "SKIP_RELAY_RUNG");

    let run = docket.run(&["promote", &id, "--to", "spec", "-q"]);
    run.assert().success();
    let spec_path = promoted_path(&run.stdout());
    assert_eq!(
        tail(&spec_path, 3),
        format!("specs/{id}-SKIP_RELAY_RUNG/spec.md")
    );

    let front = front(&spec_path);
    assert!(front.contains("kind: spec"));
    assert!(front.contains("stage: design"));
    for absent in ["chain:", "hop:", "supersedes:"] {
        assert!(
            !front.contains(absent),
            "a skipped relay rung leaves no {absent} — {front}"
        );
    }
}

#[test]
fn promotion_is_additive() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, _) = docket.create(&project, "handoff", "ADDITIVE_CLIMB");

    let to_relay = docket.run(&["promote", &id, "-q"]);
    to_relay.assert().success();
    let to_spec = docket.run(&["promote", &id, "-q"]);
    to_spec.assert().success();

    let front = front(&promoted_path(&to_spec.stdout()));
    assert!(front.contains("kind: spec"));
    assert!(front.contains("stage: design"));
    assert!(
        holds(&front, "chain", &id),
        "the chain minted at the relay rung survives — {front}"
    );
    assert!(front.contains("hop: 1"), "the hop survives — {front}");
}

#[test]
fn relaying_mints_a_successor_and_closes_the_predecessor() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (first, first_path) = docket.create(&project, "relay", "WAVE_ONE");

    let second_run = docket.run(&[
        "relay",
        &first,
        "--name",
        "WAVE_TWO",
        "--tagline",
        "Wave one landed green.",
        "-q",
    ]);
    second_run.assert().success();
    let (second, second_path) = id_and_path(&second_run.stdout());
    let second_front = front(&second_path);
    assert!(holds(&second_front, "chain", &first));
    assert!(second_front.contains("hop: 2"));
    assert!(holds(&second_front, "supersedes", &first));
    assert!(footprint_gone(&first_path));

    let third_run = docket.run(&[
        "relay",
        &second,
        "--name",
        "WAVE_THREE",
        "--tagline",
        "Wave two landed green.",
        "-q",
    ]);
    third_run.assert().success();
    let (third, third_path) = id_and_path(&third_run.stdout());
    let third_front = front(&third_path);
    assert!(
        holds(&third_front, "chain", &first),
        "the chain is stable across hops — {third_front}"
    );
    assert!(third_front.contains("hop: 3"));
    assert!(holds(&third_front, "supersedes", &second));
    assert!(footprint_gone(&second_path));

    // A relay is one exchange: the successor and the predecessor's removal
    // belong to the same commit, or a chain could lose a hop.
    let subjects = docket.history();
    assert_eq!(
        subjects.first().map(String::as_str),
        Some(format!("relay {second} to {third}").as_str()),
        "the successor and the removal share a commit — {subjects:?}"
    );
}

#[test]
fn relaying_a_handoff_points_at_promote() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, _) = docket.create(&project, "handoff", "NOT_A_RELAY");

    let run = docket.run(&[
        "relay",
        &id,
        "--name",
        "Successor",
        "--tagline",
        "Owed by nothing.",
        "-q",
    ]);

    run.assert().failure().stderr(contains("docket promote"));
}

#[test]
fn body_bytes_survive_a_metadata_rewrite() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let body = "# Heading\n\ntrailing whitespace here   \n\nlast line\n";

    let created = docket.run(&[
        "create",
        "handoff",
        "--name",
        "BODY_FIDELITY",
        "--tagline",
        "The body is not the CLI's to touch.",
        "--body",
        body,
        "--project",
        &project,
        "--allow-missing",
        "-q",
    ]);
    created.assert().success();
    let (id, _) = id_and_path(&created.stdout());

    let renamed = docket.run(&["set", &id, "--name", "BODY_FIDELITY_TWO", "-q"]);
    renamed.assert().success();

    let shown = docket.run(&["show", &id, "-q"]);
    shown.assert().success();
    assert_eq!(shown.stdout(), body);
}

#[test]
fn an_invalid_item_stays_listed_and_fails_doctor() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "spec", "DAMAGED_SPEC");
    drop_key(&path, "stage:");

    let listed = docket.run(&["list", "--project", &project]);
    listed.assert().success();
    assert!(
        listed
            .stdout()
            .lines()
            .any(|line| line.starts_with('!') && line.contains(&id) && line.contains("INVALID")),
        "an unparseable item is never hidden: {}",
        listed.stdout()
    );

    let doctor = docket.run(&["doctor"]);
    doctor.assert().failure().stdout(contains("invalid"));
    doctor.assert().stdout(contains(id));
}

#[test]
fn set_repairs_invalid_frontmatter() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "spec", "DAMAGED_SPEC");
    drop_key(&path, "stage:");

    let healed = docket.run(&["set", &id, "-q"]);
    healed.assert().success();

    let listed = docket.run(&["list", "--project", &project]);
    assert!(!listed.stdout().contains("INVALID"), "{}", listed.stdout());
    assert_eq!(listed_ids(&listed.stdout()), vec![id]);
    assert!(front(&path).contains("stage: "));
}

#[test]
fn reorder_sequence_moves_named_ids_to_the_front() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let ids: Vec<String> = ["FIRST", "SECOND", "THIRD", "FOURTH"]
        .iter()
        .map(|name| docket.create(&project, "handoff", name).0)
        .collect();

    let sequence = format!("{},{}", ids[2], ids[0]);
    let run = docket.run(&[
        "reorder",
        "--sequence",
        &sequence,
        "--project",
        &project,
        "-q",
    ]);
    run.assert().success();

    let listed = docket.run(&["list", "--project", &project]);
    assert_eq!(
        listed_ids(&listed.stdout()),
        vec![
            ids[2].clone(),
            ids[0].clone(),
            ids[1].clone(),
            ids[3].clone()
        ]
    );
}

#[test]
fn reorder_places_one_item_top_bottom_or_at_a_position() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let ids: Vec<String> = ["FIRST", "SECOND", "THIRD", "FOURTH"]
        .iter()
        .map(|name| docket.create(&project, "handoff", name).0)
        .collect();
    let listing = || {
        let listed = docket.run(&["list", "--project", &project]);
        listed.assert().success();
        listed_ids(&listed.stdout())
    };
    assert_eq!(listing(), ids);

    let moved = docket.run(&["reorder", &ids[3], "--top", "--project", &project, "-q"]);
    moved.assert().success();
    assert_eq!(
        listing(),
        vec![
            ids[3].clone(),
            ids[0].clone(),
            ids[1].clone(),
            ids[2].clone()
        ]
    );

    let moved = docket.run(&["reorder", &ids[3], "--bottom", "--project", &project, "-q"]);
    moved.assert().success();
    assert_eq!(listing(), ids);

    let moved = docket.run(&[
        "reorder",
        &ids[3],
        "--position",
        "2",
        "--project",
        &project,
        "-q",
    ]);
    moved.assert().success();
    assert_eq!(
        listing(),
        vec![
            ids[0].clone(),
            ids[3].clone(),
            ids[1].clone(),
            ids[2].clone()
        ]
    );
}

#[test]
fn moving_re_targets_an_item_and_keeps_its_identity() {
    let docket = Docket::new();
    let from = docket.project("from");
    let to = docket.project("to");
    let (id, path) = docket.create(&from, "handoff", "WRONG_PROJECT");
    let created = value(&front(&path), "created");

    let run = docket.run(&["move", &id, "--to", &to, "--allow-missing", "-q"]);
    run.assert().success();
    let (moved_id, moved) = id_and_path(&run.stdout());

    assert_eq!(moved_id, id, "an id survives a move");
    assert!(!path.exists(), "the item left the docket it was on");
    assert_eq!(tail(&moved, 2), format!("handoffs/{id}-WRONG_PROJECT.md"));
    assert_eq!(
        value(&front(&moved), "created"),
        created,
        "a move re-targets an item, it does not mint a new one"
    );
    assert_eq!(
        docket.run(&["show", &id, "-q"]).stdout(),
        "Body of WRONG_PROJECT.\n",
        "the body is not the command's to touch"
    );

    let listed =
        |project: &str| listed_ids(&docket.run(&["list", "--project", project, "-q"]).stdout());
    assert_eq!(listed(&to), vec![id.clone()]);
    assert!(listed(&from).is_empty(), "nothing is left behind");

    let subject = docket.history().first().cloned().unwrap_or_default();
    assert!(
        subject.starts_with(&format!("move {id}: ")),
        "a move is one typed commit — {subject:?}"
    );
}

#[test]
fn moving_a_spec_carries_its_supporting_files() {
    let docket = Docket::new();
    let from = docket.project("from");
    let to = docket.project("to");
    let (id, path) = docket.create(&from, "spec", "HAS_ATTACHMENTS");
    let directory = path
        .parent()
        .expect("a spec sits in a directory")
        .to_owned();
    fs::write(directory.join("schema.md"), "The shape.\n").expect("writing a supporting file");

    let run = docket.run(&["move", &id, "--to", &to, "--allow-missing", "-q"]);
    run.assert().success();
    let (_, moved) = id_and_path(&run.stdout());
    let landed = moved.parent().expect("a spec sits in a directory");

    assert_eq!(
        tail(&moved, 3),
        format!("specs/{id}-HAS_ATTACHMENTS/spec.md")
    );
    assert_eq!(
        fs::read_to_string(landed.join("schema.md")).ok(),
        Some("The shape.\n".to_owned()),
        "a spec moves as the directory it is, not as its entrypoint alone"
    );
    assert!(!directory.exists(), "nothing is left on the shelf it left");
}

#[test]
fn a_moved_relay_carries_its_chain() {
    let docket = Docket::new();
    let from = docket.project("from");
    let to = docket.project("to");
    let (first, _) = docket.create(&from, "relay", "CHAIN_HEAD");

    let successor = docket.run(&[
        "relay",
        &first,
        "--name",
        "CHAIN_NEXT",
        "--tagline",
        "Hop one landed green.",
        "-q",
    ]);
    successor.assert().success();
    let (second, _) = id_and_path(&successor.stdout());

    let run = docket.run(&["move", &second, "--to", &to, "--allow-missing", "-q"]);
    run.assert().success();
    let front = front(&id_and_path(&run.stdout()).1);

    assert!(
        holds(&front, "chain", &first),
        "a chain crosses projects — {front}"
    );
    assert!(front.contains("hop: 2"), "the hop survives — {front}");
    assert!(
        holds(&front, "supersedes", &first),
        "provenance survives — {front}"
    );
}

#[test]
fn a_moved_item_lands_at_the_bottom_of_its_new_docket() {
    let docket = Docket::new();
    let from = docket.project("from");
    let to = docket.project("to");
    let (resident, _) = docket.create(&to, "handoff", "ALREADY_THERE");
    let (arriving, _) = docket.create(&from, "handoff", "ARRIVING");

    let run = docket.run(&["move", &arriving, "--to", &to, "--allow-missing", "-q"]);
    run.assert().success();
    assert_eq!(
        listed_ids(&docket.run(&["list", "--project", &to, "-q"]).stdout()),
        vec![resident, arriving],
        "an arrival is new on that docket, so it lands under what is already on it"
    );
}

/// Origin says where an item was written, when that is not where it sits. A
/// move changes the second, so it is what makes the two differ — or agree.
#[test]
fn moving_records_where_an_item_was_written() {
    let docket = Docket::new();
    let home = docket.project("home");
    let away = docket.project("away");
    let third = docket.project("third");
    let (id, path) = docket.create(&home, "handoff", "TRAVELS");
    assert!(
        !front(&path).contains("origin:"),
        "written where it sits, so nothing differs"
    );

    let out = docket.run(&["move", &id, "--to", &away, "--allow-missing", "-q"]);
    out.assert().success();
    let front_away = front(&id_and_path(&out.stdout()).1);
    assert!(
        holds(&front_away, "origin", &home),
        "the docket it left is where it was written — {front_away}"
    );

    let on = docket.run(&["move", &id, "--to", &third, "--allow-missing", "-q"]);
    on.assert().success();
    let front_third = front(&id_and_path(&on.stdout()).1);
    assert!(
        holds(&front_third, "origin", &home),
        "a second move does not rewrite where it was written — {front_third}"
    );

    let back = docket.run(&["move", &id, "--to", &home, "--allow-missing", "-q"]);
    back.assert().success();
    let front_home = front(&id_and_path(&back.stdout()).1);
    assert!(
        !front_home.contains("origin:"),
        "home again, so the two agree — {front_home}"
    );
}

#[test]
fn moving_refuses_a_target_it_cannot_stand_behind() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let elsewhere = docket.project("elsewhere");
    let (id, path) = docket.create(&project, "spec", "STAYS_PUT");

    let same = docket.run(&["move", &id, "--to", &project, "-q"]);
    same.assert().failure().stderr(contains("already"));

    let missing = docket.run(&["move", &id, "--to", &elsewhere, "-q"]);
    missing
        .assert()
        .failure()
        .stderr(contains("--allow-missing"));

    drop_key(&path, "stage:");
    let damaged = docket.run(&["move", &id, "--to", &elsewhere, "--allow-missing", "-q"]);
    damaged
        .assert()
        .failure()
        .stderr(contains(format!("docket set {id}")));
}

#[test]
fn closing_removes_an_item_and_history_keeps_it() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "handoff", "FINISHED_WORK");

    let closed = docket.run(&["close", &id, "-q"]);
    closed.assert().success();
    assert!(footprint_gone(&path));

    let open = docket.run(&["list", "--project", &project]);
    assert!(!listed_ids(&open.stdout()).contains(&id));

    let subjects = docket.history();
    assert_eq!(
        subjects.first().map(String::as_str),
        Some(format!("close {id}: FINISHED_WORK").as_str()),
        "the close is the top commit — {subjects:?}"
    );
    assert!(
        docket.removed_paths().contains(&id),
        "history should name what it removed"
    );
}

/// Closing is the one irreversible act, so it is gated on history holding the
/// item — and refused outright when there is no history to hold it.
#[test]
fn closing_is_refused_without_git() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "handoff", "UNRECORDED_WORK");

    let run = docket.run_with(&["close", &id, "-q"], &[("RELIC_GIT", "")]);
    run.assert().failure().stderr(contains("git"));
    assert!(path.is_file(), "the item survives a refused close");
}

/// An id is minted against history, not against the disk, so nothing a closed
/// item was addressed by ever comes back.
#[test]
fn a_closed_id_is_never_minted_again() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let mut seen = Vec::new();
    for round in 0..8 {
        let (id, _) = docket.create(&project, "handoff", &format!("ROUND_{round}"));
        assert!(!seen.contains(&id), "{id} was minted twice");
        seen.push(id.clone());
        let closed = docket.run(&["close", &id, "-q"]);
        closed.assert().success();
    }
}

/// Bodies are authored outside docket, through the path it prints. The next
/// command that writes records that work before adding its own.
#[test]
fn outside_edits_are_recorded_before_the_next_change() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (first, path) = docket.create(&project, "handoff", "HAS_A_BODY");
    let (second, _) = docket.create(&project, "handoff", "WRITTEN_LATER");

    let text = fs::read_to_string(&path).expect("reading the item");
    fs::write(&path, format!("{text}\nAn agent wrote this.\n")).expect("editing the body");

    let run = docket.run(&["set", &second, "--tagline", "Retagged.", "-q"]);
    run.assert().success();

    let subjects = docket.history();
    assert_eq!(
        subjects.first().map(String::as_str),
        Some(format!("set {second}").as_str()),
        "the command's own change is the top commit — {subjects:?}"
    );
    assert_eq!(
        subjects.get(1).map(String::as_str),
        Some(format!("edit: {first}").as_str()),
        "the outside edit is recorded under it — {subjects:?}"
    );
}

/// The session-start hook is the only bracket guaranteed to arrive, so it
/// records drift too — without saying anything about it.
#[test]
fn announce_records_drift_and_stays_quiet_about_it() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "handoff", "DRIFTS");

    let text = fs::read_to_string(&path).expect("reading the item");
    fs::write(&path, format!("{text}\nEdited between sessions.\n")).expect("editing the body");

    let run = docket.run(&["announce", "--project", &project]);
    run.assert().success();
    assert!(
        !run.stdout().contains("edit:") && !run.stderr().contains("edit:"),
        "announce says nothing about history"
    );
    assert_eq!(
        docket.history().first().map(String::as_str),
        Some(format!("edit: {id}").as_str())
    );
}

/// A depot that has never been opened gets no repository from a session merely
/// starting.
#[test]
fn announce_does_not_create_a_depot() {
    let docket = Docket::new();
    fs::remove_dir_all(&docket.root).expect("clearing the depot");

    let run = docket.run(&["announce"]);
    run.assert().success();
    assert!(!docket.root.exists(), "announce created {:?}", docket.root);
}

#[test]
fn announce_is_silent_when_empty_and_emits_hook_json_when_not() {
    let docket = Docket::new();
    let project = docket.project("proj");

    let silent = docket.run(&["announce", "--project", &project]);
    silent.assert().success();
    assert_eq!(silent.stdout(), "");

    let (id, _) = docket.create(&project, "handoff", "OUTSTANDING_WORK");

    let roster = docket.run(&["announce", "--project", &project]);
    roster.assert().success().stdout(contains(id));
    roster.assert().stdout(contains("OUTSTANDING_WORK"));
    // A name says which item; only the tagline says what it is, so the banner
    // carries both on the row itself.
    let row = roster
        .stdout()
        .lines()
        .find(|line| line.contains("OUTSTANDING_WORK"))
        .expect("the item is listed")
        .to_owned();
    assert!(
        row.contains("What a future session reads first."),
        "the row carries its tagline: {row}"
    );

    let hook = docket.run(&["announce", "--hook", "--project", &project]);
    hook.assert().success();
    let emitted = hook.stdout();
    assert!(
        emitted.contains(r#""hookEventName":"SessionStart""#),
        "hook JSON: {emitted}"
    );
    let context = emitted
        .split_once(r#""additionalContext":""#)
        .expect("hook JSON carries additionalContext")
        .1;
    assert!(!context.starts_with('"'), "additionalContext: {emitted}");
}

#[test]
fn json_format_and_json_flag_agree() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, _) = docket.create(&project, "handoff", "MACHINE_READABLE");

    let formatted = docket.run(&["list", "--format", "json", "--project", &project]);
    formatted.assert().success();
    assert!(formatted.stdout().starts_with('{'));
    formatted
        .assert()
        .stdout(contains(format!("\"id\": \"{id}\"")));

    let flagged = docket.run(&["list", "--json", "--project", &project]);
    flagged.assert().success();
    assert_eq!(flagged.stdout(), formatted.stdout());
}

#[test]
fn agent_and_uncoloured_human_output_carry_no_escapes() {
    let docket = Docket::new();
    let project = docket.project("proj");
    docket.create(&project, "handoff", "PLAIN_TEXT_ONLY");

    let agent = docket.run(&["--project", &project]);
    agent.assert().success();
    assert!(!agent.stdout().contains('\x1b'), "{}", agent.stdout());

    let human = docket.run(&[
        "list",
        "--format",
        "human",
        "--color",
        "never",
        "--project",
        &project,
    ]);
    human.assert().success();
    assert!(!human.stdout().contains('\x1b'), "{}", human.stdout());
}

#[test]
fn help_serves_topics_and_commands_and_rejects_neither() {
    let docket = Docket::new();

    for topic in ["ladder", "metadata"] {
        let run = docket.run(&["help", topic]);
        run.assert().success();
        assert!(!run.stdout().trim().is_empty(), "help {topic} said nothing");
    }

    // A name that is not a topic falls through to the subcommand of that name.
    let command = docket.run(&["help", "create"]);
    command
        .assert()
        .success()
        .stdout(contains("--allow-missing"));

    let unknown = docket.run(&["help", "nonsense"]);
    unknown.assert().failure();
    unknown
        .assert()
        .failure()
        .stderr(contains("ladder, metadata"));
}

/// Kind is taken from the directory an item sits in, not from metadata that may
/// no longer parse. Otherwise closing a damaged spec treats it as a handoff and
/// removes the wrong footprint, leaving its directory behind.
#[test]
fn closing_an_invalid_spec_removes_its_directory() {
    let docket = Docket::new();
    let project = docket.project("damaged-spec");
    let (id, path) = docket.create(&project, "spec", "DAMAGED_SPEC_DIR");
    drop_key(&path, "stage");
    let spec_dir = path
        .parent()
        .expect("a spec lives in its own directory")
        .to_owned();

    let run = docket.run(&["close", &id, "--project", &project, "-q"]);
    run.assert().success();
    assert!(!spec_dir.exists(), "the spec directory was left behind");

    let removed = docket.removed_paths();
    assert!(
        removed.contains(&format!("specs/{id}-DAMAGED_SPEC_DIR/spec.md")),
        "history should hold the whole spec — {removed}"
    );
}

/// A closed item is no longer addressable, and the error says where it went.
#[test]
fn a_closed_id_points_at_history() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, _) = docket.create(&project, "handoff", "DONE_AND_GONE");
    docket.run(&["close", &id, "-q"]).assert().success();

    let located = docket.run(&["path", &id]);
    located.assert().failure().stderr(contains("diff-filter=D"));
}

/// Every keying entry point answers the same: a linked worktree folds into the
/// main checkout whether the path arrives as the working directory, as
/// --project, or as create --to. Without that, one session's items key to the
/// worktree and the next session's to the checkout.
#[test]
fn a_linked_worktree_keys_to_its_main_checkout() {
    let docket = Docket::new();
    let main = docket.base.join("repo");
    fs::create_dir_all(&main).expect("creating the repository");
    docket.git(
        &main,
        &["-c", "init.defaultBranch=main", "init", "--quiet", "."],
    );
    fs::write(main.join("README.md"), "seed\n").expect("seeding the repository");
    docket.git(&main, &["add", "-A"]);
    docket.git(&main, &["commit", "--quiet", "-m", "seed"]);

    let linked = docket.base.join("linked");
    docket.git(
        &main,
        &[
            "worktree",
            "add",
            "--quiet",
            "-b",
            "side",
            linked.to_str().expect("utf-8 path"),
        ],
    );

    // From inside the worktree, with no --project at all.
    let created = docket.run_in(
        &linked,
        &[
            "create",
            "handoff",
            "--name",
            "KEYED_FROM_WORKTREE",
            "--tagline",
            "Which docket does this land on?",
            "--body",
            "Body.\n",
            "-q",
        ],
    );
    created.assert().success();
    let (id, path) = id_and_path(&created.stdout());
    assert!(
        path.starts_with(&docket.root),
        "the item should sit in the depot: {path:?}"
    );
    assert!(
        holds(&front(&path), "project", &main.display().to_string()),
        "the worktree should key to its main checkout — {}",
        front(&path)
    );

    // And --project pointed at the worktree agrees with it.
    let listed = docket.run(&["list", "--project", linked.to_str().expect("utf-8 path")]);
    assert!(
        listed_ids(&listed.stdout()).contains(&id),
        "--project on the worktree should reach the same docket: {}",
        listed.stdout()
    );
}

#[test]
fn search_matches_a_name_a_tagline_and_a_body() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (named, _) = docket.write(
        &project,
        "handoff",
        "ROSETTA_TABLE",
        "One line about nothing.",
        "A body about nothing.\n",
    );
    let (taglined, _) = docket.write(
        &project,
        "handoff",
        "SECOND_ITEM",
        "The obelisk is the point.",
        "A body about nothing.\n",
    );
    let (bodied, _) = docket.write(
        &project,
        "handoff",
        "THIRD_ITEM",
        "One line about nothing.",
        "The demotic register is the one that mattered.\n",
    );

    for (needle, wanted) in [
        ("rosetta", &named),
        ("obelisk", &taglined),
        ("demotic", &bodied),
    ] {
        let run = docket.run(&["list", "--search", needle, "--project", &project]);
        run.assert().success();
        assert_eq!(listed_ids(&run.stdout()), vec![wanted.clone()], "{needle}");
    }
}

#[test]
fn search_does_not_reach_metadata() {
    let docket = Docket::new();
    let project = docket.project("proj");
    docket.write(
        &project,
        "handoff",
        "PLAIN_ITEM",
        "One line about nothing.",
        "A body about nothing.\n",
    );

    // Every item states its kind in its metadata, so a search that read it
    // would answer with all of them.
    for needle in ["handoff", "tagline", "created"] {
        let run = docket.run(&["list", "--search", needle, "--project", &project]);
        run.assert().success();
        assert!(listed_ids(&run.stdout()).is_empty(), "{needle}");
    }
}

#[test]
fn search_ignores_case() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, _) = docket.write(
        &project,
        "handoff",
        "MIXED_CASE",
        "One line about nothing.",
        "The Rosetta table moves first.\n",
    );

    for needle in ["rosetta", "ROSETTA", "RoSeTtA"] {
        let run = docket.run(&["list", "--search", needle, "--project", &project]);
        assert_eq!(listed_ids(&run.stdout()), vec![id.clone()], "{needle}");
    }
}

#[test]
fn search_reaches_an_item_whose_metadata_will_not_parse() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.write(
        &project,
        "spec",
        "DAMAGED_SPEC",
        "One line about nothing.",
        "The demotic register is the one that mattered.\n",
    );
    drop_key(&path, "stage");

    // The name comes from the filename and the body from what follows the
    // metadata, so both still answer.
    for needle in ["damaged", "demotic"] {
        let run = docket.run(&["list", "--search", needle, "--project", &project]);
        run.assert().success();
        assert_eq!(listed_ids(&run.stdout()), vec![id.clone()], "{needle}");
        run.assert().stdout(contains("INVALID"));
    }
}

#[test]
fn search_quotes_the_body_line_it_matched_and_nothing_else() {
    let docket = Docket::new();
    let project = docket.project("proj");
    docket.write(
        &project,
        "handoff",
        "QUOTED_LINE",
        "One line about nothing.",
        "A first line.\nThe demotic register is the one that mattered.\nA third.\n",
    );

    let hit = docket.run(&["list", "--search", "demotic", "--project", &project]);
    hit.assert().success();
    assert_eq!(
        notes(&hit.stdout()),
        vec!["match: The demotic register is the one that mattered."]
    );

    // A name or a tagline is already on the row, so quoting it would say
    // nothing the reader cannot see.
    let seen = docket.run(&["list", "--search", "quoted", "--project", &project]);
    assert!(notes(&seen.stdout()).is_empty(), "{}", seen.stdout());
}

#[test]
fn tag_filters_demand_every_tag_named() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (both, _) = docket.create(&project, "handoff", "BOTH_TAGS");
    let (one, _) = docket.create(&project, "handoff", "ONE_TAG");
    docket.create(&project, "handoff", "NO_TAGS");
    docket.tag(&both, "ci,release");
    docket.tag(&one, "ci");

    let by = |args: &[&str]| {
        let mut all = vec!["list"];
        all.extend_from_slice(args);
        all.extend_from_slice(&["--project", &project]);
        let run = docket.run(&all);
        run.assert().success();
        listed_ids(&run.stdout())
    };

    assert_eq!(by(&["--tag", "ci"]).len(), 2);
    assert_eq!(by(&["--tag", "ci", "--tag", "release"]), vec![both.clone()]);
    assert!(by(&["--tag", "absent"]).is_empty());
    assert!(!by(&["--tag", "ci"]).contains(&"NO_TAGS".to_owned()));
    assert!(by(&["--tag", "ci"]).contains(&one));
}

#[test]
fn tags_are_shown_on_the_row_that_carries_them() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (tagged, _) = docket.create(&project, "handoff", "TAGGED_ITEM");
    docket.create(&project, "handoff", "BARE_ITEM");
    docket.tag(&tagged, "ci,release");

    let run = docket.run(&["list", "--project", &project]);
    assert_eq!(notes(&run.stdout()), vec!["tags: ci release"]);
}

#[test]
fn kind_answers_from_the_shelf_when_metadata_will_not_parse() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (spec, path) = docket.create(&project, "spec", "DAMAGED_SPEC");
    let (handoff, _) = docket.create(&project, "handoff", "SOUND_HANDOFF");
    drop_key(&path, "stage");

    let specs = docket.run(&["list", "--kind", "spec", "--project", &project]);
    assert_eq!(listed_ids(&specs.stdout()), vec![spec]);
    specs.assert().stdout(contains("INVALID"));

    let handoffs = docket.run(&["list", "--kind", "handoff", "--project", &project]);
    assert_eq!(listed_ids(&handoffs.stdout()), vec![handoff]);
}

#[test]
fn invalid_selects_only_what_will_not_parse() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (damaged, path) = docket.create(&project, "spec", "DAMAGED_SPEC");
    docket.create(&project, "handoff", "SOUND_HANDOFF");
    drop_key(&path, "stage");

    let only = docket.run(&["list", "--invalid", "--project", &project]);
    assert_eq!(listed_ids(&only.stdout()), vec![damaged.clone()]);

    let narrowed = docket.run(&["list", "--invalid", "--kind", "spec", "--project", &project]);
    assert_eq!(listed_ids(&narrowed.stdout()), vec![damaged]);

    // Nothing that will not parse can answer a block, so the two together are
    // empty rather than a refusal.
    let none = docket.run(&["list", "--invalid", "--blocked", "--project", &project]);
    none.assert().success();
    assert!(listed_ids(&none.stdout()).is_empty());
}

#[test]
fn every_filter_narrows_the_same_listing() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (wanted, _) = docket.write(
        &project,
        "spec",
        "EVERY_FLAG",
        "One line about nothing.",
        "The demotic register is the one that mattered.\n",
    );
    docket.tag(&wanted, "ci");
    let held = docket.run(&["set", &wanted, "--blocked", "the schema review", "-q"]);
    held.assert().success();
    docket.write(
        &project,
        "handoff",
        "OTHER_ITEM",
        "One line about nothing.",
        "The demotic register is the one that mattered.\n",
    );

    let all = docket.run(&[
        "list",
        "--kind",
        "spec",
        "--tag",
        "ci",
        "--blocked",
        "--search",
        "demotic",
        "--project",
        &project,
    ]);
    all.assert().success();
    assert_eq!(listed_ids(&all.stdout()), vec![wanted.clone()]);

    // Dropping the one flag that only this item answers widens the result.
    let wider = docket.run(&["list", "--search", "demotic", "--project", &project]);
    assert_eq!(listed_ids(&wider.stdout()).len(), 2);
}

#[test]
fn an_empty_result_says_whether_it_was_narrowed() {
    let docket = Docket::new();
    let project = docket.project("proj");

    let bare = docket.run(&["list", "--project", &project]);
    bare.assert().stdout(contains("(empty)"));

    docket.create(&project, "handoff", "SOUND_HANDOFF");
    let filtered = docket.run(&["list", "--kind", "spec", "--project", &project]);
    filtered.assert().success().stdout(contains("(no match)"));
}

#[test]
fn a_listing_across_projects_names_each_row_s_project() {
    let docket = Docket::new();
    let alpha = docket.project("alpha");
    let beta = docket.project("beta");
    let (first, _) = docket.create(&alpha, "handoff", "ALPHA_WORK");
    let (second, _) = docket.create(&beta, "handoff", "BETA_WORK");

    let run = docket.run(&["list", "--all"]);
    run.assert().success();
    let ids = listed_ids(&run.stdout());
    assert!(ids.contains(&first) && ids.contains(&second), "{ids:?}");
    assert_eq!(run.stdout().matches("~/alpha").count(), 1);
    assert_eq!(run.stdout().matches("~/beta").count(), 1);
    assert!(
        run.stdout().starts_with("docket 2 projects\n"),
        "{}",
        run.stdout()
    );
}

#[test]
fn projects_rank_by_the_head_that_has_waited_longest() {
    let docket = Docket::new();
    let alpha = docket.project("alpha");
    let beta = docket.project("beta");
    let (early, early_path) = docket.create(&alpha, "handoff", "EARLY_HEAD");
    let (late, late_path) = docket.create(&beta, "handoff", "LATE_HEAD");
    swap_key(&early_path, "created", "created: 2020-01-01T00:00:00Z");
    swap_key(&late_path, "created", "created: 2024-01-01T00:00:00Z");

    let run = docket.run(&["list", "--all"]);
    assert_eq!(listed_ids(&run.stdout()), vec![early.clone(), late.clone()]);

    // A head that cannot answer its age ranks last, whatever its path.
    let (damaged, damaged_path) = docket.create(&docket.project("aardvark"), "spec", "NO_AGE");
    drop_key(&damaged_path, "stage");
    let again = docket.run(&["list", "--all"]);
    assert_eq!(listed_ids(&again.stdout()), vec![early, late, damaged]);
}

#[test]
fn a_listing_across_projects_keeps_each_project_in_its_own_order() {
    let docket = Docket::new();
    let alpha = docket.project("alpha");
    let beta = docket.project("beta");
    let (first, first_path) = docket.create(&alpha, "handoff", "ALPHA_ONE");
    let (second, second_path) = docket.create(&alpha, "handoff", "ALPHA_TWO");
    let (other, other_path) = docket.create(&beta, "handoff", "BETA_ONE");
    swap_key(&first_path, "created", "created: 2020-01-01T00:00:00Z");
    swap_key(&second_path, "created", "created: 2019-01-01T00:00:00Z");
    swap_key(&other_path, "created", "created: 2024-01-01T00:00:00Z");

    let moved = docket.run(&["reorder", &second, "--top", "--project", &alpha, "-q"]);
    moved.assert().success();

    let run = docket.run(&["list", "--all"]);
    assert_eq!(listed_ids(&run.stdout()), vec![second, first, other]);
}

#[test]
fn a_listing_across_projects_drops_a_project_that_answers_nothing() {
    let docket = Docket::new();
    let alpha = docket.project("alpha");
    let beta = docket.project("beta");
    let (spec, _) = docket.create(&alpha, "spec", "ALPHA_SPEC");
    docket.create(&beta, "handoff", "BETA_WORK");

    let run = docket.run(&["list", "--all", "--kind", "spec"]);
    assert_eq!(listed_ids(&run.stdout()), vec![spec]);
    assert!(!run.stdout().contains("~/beta"), "{}", run.stdout());
    assert!(
        run.stdout().starts_with("docket 1 project\n"),
        "{}",
        run.stdout()
    );
}

#[test]
fn a_printed_position_addresses_reorder_under_a_filter() {
    let docket = Docket::new();
    let project = docket.project("proj");
    docket.create(&project, "handoff", "FIRST_ITEM");
    let (second, _) = docket.create(&project, "spec", "SECOND_ITEM");
    docket.create(&project, "handoff", "THIRD_ITEM");
    let (fourth, _) = docket.create(&project, "spec", "FOURTH_ITEM");

    // A narrowed listing shows where each item sits on the whole docket, not
    // where it sits in the answer.
    let specs = docket.run(&["list", "--kind", "spec", "--project", &project]);
    assert_eq!(listed_ids(&specs.stdout()), vec![second, fourth.clone()]);
    assert_eq!(listed_positions(&specs.stdout()), vec!["2", "4"]);

    // So a position read off it means the same thing to reorder.
    let held = docket.run(&["reorder", "--position", "4", "--project", &project, "-q"]);
    held.assert().failure();
    let moved = docket.run(&[
        "reorder",
        &fourth,
        "--position",
        "1",
        "--project",
        &project,
        "-q",
    ]);
    moved.assert().success();
    let after = docket.run(&["list", "--project", &project]);
    assert_eq!(listed_ids(&after.stdout())[0], fourth);
}

#[test]
fn announce_carries_the_block_and_not_the_tags() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, _) = docket.create(&project, "handoff", "BANNER_ITEM");
    docket.tag(&id, "ci");
    let held = docket.run(&["set", &id, "--blocked", "the schema review", "-q"]);
    held.assert().success();

    let run = docket.run(&["announce", "--project", &project]);
    run.assert()
        .success()
        .stdout(contains("blocked: the schema review"));
    assert!(!run.stdout().contains("tags:"), "{}", run.stdout());
}

#[test]
fn json_is_one_shape_whichever_scope_produced_it() {
    let docket = Docket::new();
    let alpha = docket.project("alpha");
    let beta = docket.project("beta");
    let (_, path) = docket.write(
        &alpha,
        "spec",
        "ALPHA_SPEC",
        "One line about nothing.",
        "The demotic register is the one that mattered.\n",
    );
    docket.create(&beta, "handoff", "BETA_WORK");
    drop_key(&path, "stage");

    for args in [
        vec!["list", "--json", "--project", &alpha],
        vec!["list", "--json", "--all"],
    ] {
        let run = docket.run(&args);
        run.assert().success();
        let out = run.stdout();
        assert_eq!(out.matches("\"items\"").count(), 1, "{out}");
        assert!(out.starts_with("{\n  \"items\": ["), "{out}");
        // Every item names its own project, whether or not it parsed.
        assert!(out.contains("\"valid\": false"), "{out}");
        assert!(out.contains(&format!("\"project\": \"{alpha}\"")), "{out}");
        assert!(out.contains("\"kind\": \"spec\""), "{out}");
    }

    let searched = docket.run(&["list", "--json", "--search", "demotic", "--project", &alpha]);
    searched.assert().stdout(contains("\"excerpt\""));
}

#[test]
fn list_help_states_how_the_filters_compose() {
    let docket = Docket::new();
    let run = docket.run(&["help", "list"]);
    run.assert().success();
    let out = run.stdout();
    for phrase in ["narrow", "--invalid", "--tag", "--search"] {
        assert!(out.contains(phrase), "{phrase} is missing from {out}");
    }
    assert!(!out.contains('`'), "{out}");
}

#[test]
fn an_empty_search_and_a_malformed_tag_are_refused() {
    let docket = Docket::new();
    let project = docket.project("proj");
    docket.create(&project, "handoff", "SOUND_HANDOFF");

    let blank = docket.run(&["list", "--search", "   ", "--project", &project]);
    blank.assert().failure().stderr(contains("--search"));

    let spaced = docket.run(&["list", "--tag", "two words", "--project", &project]);
    spaced.assert().failure().stderr(contains("tag"));
}
