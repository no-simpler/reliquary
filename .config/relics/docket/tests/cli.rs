use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::{env, fs};

/// A temporary depot and the binary that acts on it. Every invocation is pinned
/// to `DOCKET_ROOT` and to agent-shaped output, so no test can reach the real
/// depot or depend on a terminal.
struct Docket {
    base: PathBuf,
    root: PathBuf,
}

impl Docket {
    fn new() -> Docket {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let unique = format!(
            "docket-tests-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        );
        let base = env::temp_dir().join(unique);
        fs::create_dir_all(&base).expect("creating the test directory");
        // Canonical, because the CLI resolves every path it is handed, and the
        // paths it prints have to match the ones a test builds by hand.
        let base = fs::canonicalize(&base).expect("canonicalising the test directory");
        let root = base.join("depot");
        fs::create_dir_all(&root).expect("creating the depot root");
        Docket { base, root }
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

    /// The same invocation with extra environment, for the seams: `DOCKET_GIT`
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
        let output = Command::new("git")
            .arg("-C")
            .arg(cwd)
            .args(args)
            .env("HOME", &self.base)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "docket tests")
            .env("GIT_AUTHOR_EMAIL", "tests@localhost")
            .env("GIT_COMMITTER_NAME", "docket tests")
            .env("GIT_COMMITTER_EMAIL", "tests@localhost")
            .output()
            .expect("running git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
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
            .env_remove("DOCKET_GIT");
        for (key, value) in env {
            command.env(key, value);
        }
        let output = command.output().expect("running the docket binary");
        Run { output }
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
        assert!(run.ok(), "create failed: {}", run.stderr());
        id_and_path(&run.stdout())
    }
}

impl Drop for Docket {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.base);
    }
}

struct Run {
    output: Output,
}

impl Run {
    fn stdout(&self) -> String {
        String::from_utf8_lossy(&self.output.stdout).into_owned()
    }

    fn stderr(&self) -> String {
        String::from_utf8_lossy(&self.output.stderr).into_owned()
    }

    fn code(&self) -> i32 {
        self.output.status.code().unwrap_or(-1)
    }

    fn ok(&self) -> bool {
        self.output.status.success()
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
    rest[..=end].to_owned()
}

/// Ids in listing order, taken from the numbered lines of an agent-shaped
/// listing so a tagline can never be mistaken for a row.
fn listed_ids(stdout: &str) -> Vec<String> {
    stdout
        .lines()
        .filter(|line| !line.starts_with(' '))
        .filter_map(|line| {
            let mut fields = line.split_whitespace();
            let position = fields.next()?;
            if position != "!" && position.parse::<usize>().is_err() {
                return None;
            }
            fields.next().map(str::to_owned)
        })
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

/// Nothing an item occupied survives closing — the file, and a spec's whole
/// directory with it.
fn footprint_gone(path: &Path) -> bool {
    if path.file_name() == Some(OsStr::new("spec.md")) {
        return path.parent().map(|dir| !dir.exists()).unwrap_or(true);
    }
    !path.exists()
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

    assert!(!run.ok());
    assert_ne!(run.code(), 0);
    assert!(
        run.stderr().contains("--allow-missing"),
        "stderr should name the flag: {}",
        run.stderr()
    );
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
        assert!(!run.ok(), "expected a refusal: {}", run.stdout());
        assert!(run.stderr().contains(expected), "{}", run.stderr());
    }
    assert!(create("A_NAME", "A tagline.").ok());
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
        assert!(run.ok(), "create failed: {}", run.stderr());
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
            assert!(
                doctor.stdout().contains("repeated DREAM_RESIDUE"),
                "{}",
                doctor.stdout()
            );
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
        assert!(!run.ok(), "expected `set {flag}` to refuse {value:?}");
        assert!(run.stderr().contains(expected), "{}", run.stderr());
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
    assert!(!run.ok(), "expected `relay` to refuse an overlong tagline");
    assert!(
        run.stderr().contains("--tagline is 156 characters"),
        "{}",
        run.stderr()
    );
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
    assert!(run.ok(), "create failed: {}", run.stderr());
    let (_, path) = id_and_path(&run.stdout());
    assert!(
        front(&path).contains("tagline: Two lines of it.\n"),
        "{}",
        front(&path)
    );
}

#[test]
fn a_legacy_description_key_loads_and_is_rewritten_as_a_tagline() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "handoff", "A_HANDOFF");

    let text = fs::read_to_string(&path).expect("reading the item");
    let overlong = format!("description: {}\n", TOO_LONG);
    fs::write(
        &path,
        text.lines()
            .map(|line| {
                if line.starts_with("tagline:") {
                    overlong.clone()
                } else {
                    format!("{line}\n")
                }
            })
            .collect::<String>(),
    )
    .expect("rewriting the item");

    // Lenient on the way in: the item still parses and still lists.
    let listed = docket.run(&["list", "--project", &project, "-q"]);
    assert!(listed.ok());
    assert!(listed.stdout().contains(TOO_LONG), "{}", listed.stdout());

    // Reported on the way past, with the command that fixes it.
    let doctor = docket.run(&["doctor"]);
    assert!(!doctor.ok(), "doctor should fail: {}", doctor.stdout());
    assert!(
        doctor.stdout().contains("overlong") && doctor.stdout().contains("--tagline"),
        "{}",
        doctor.stdout()
    );

    let set = docket.run(&["set", &id, "--tagline", "Short enough now.", "-q"]);
    assert!(set.ok(), "set failed: {}", set.stderr());
    let front = front(&path);
    assert!(front.contains("tagline: Short enough now.\n"), "{front}");
    assert!(!front.contains("description:"), "{front}");
}

#[test]
fn a_legacy_title_key_loads_and_is_rewritten_as_a_name() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "handoff", "A_HANDOFF");

    let text = fs::read_to_string(&path).expect("reading the item");
    fs::write(
        &path,
        text.lines()
            .map(|line| match line.strip_prefix("name: ") {
                Some(_) => "title: Migrate the four legacy conventions\n".to_owned(),
                None => format!("{line}\n"),
            })
            .collect::<String>(),
    )
    .expect("rewriting the item");

    // Lenient on the way in: the item still parses and still lists.
    let listed = docket.run(&["list", "--project", &project, "-q"]);
    assert!(listed.ok(), "list failed: {}", listed.stderr());
    assert!(listed.stdout().contains(&id), "{}", listed.stdout());

    // Reported on the way past, with the command that fixes it.
    let doctor = docket.run(&["doctor"]);
    assert!(!doctor.ok(), "doctor should fail: {}", doctor.stdout());
    assert!(
        doctor.stdout().contains("malformed") && doctor.stdout().contains("--name"),
        "{}",
        doctor.stdout()
    );

    let set = docket.run(&["set", &id, "--name", "LEGACY_MIGRATION", "-q"]);
    assert!(set.ok(), "set failed: {}", set.stderr());
    let front = front(&path);
    assert!(front.contains("name: LEGACY_MIGRATION\n"), "{front}");
    assert!(!front.contains("title:"), "{front}");
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
        assert!(located.ok(), "path {typed} failed: {}", located.stderr());
        assert_eq!(located.stdout().trim_end(), path.to_str().unwrap());
    }

    let missing = docket.run(&["show", "NOT_OPEN"]);
    assert!(!missing.ok());
    assert!(
        missing.stderr().contains("no open item named NOT_OPEN"),
        "{}",
        missing.stderr()
    );

    let nonsense = docket.run(&["show", "not an id or a name"]);
    assert!(!nonsense.ok());
    assert!(
        nonsense.stderr().contains("neither an id nor a name"),
        "{}",
        nonsense.stderr()
    );

    // A name is not unique, so a second one is a refusal rather than a guess.
    let (twin, _) = docket.create(&beta, "spec", "ROSETTA");
    let ambiguous = docket.run(&["close", "ROSETTA", "-q"]);
    assert!(!ambiguous.ok(), "an ambiguous name should refuse");
    let stderr = ambiguous.stderr();
    for named in [&id, &twin] {
        assert!(stderr.contains(named.as_str()), "{stderr}");
    }
    assert!(path.exists(), "nothing was closed");

    // The id still discriminates, and the name resolves again once it is free.
    let closed = docket.run(&["close", &twin, "-q"]);
    assert!(closed.ok(), "close failed: {}", closed.stderr());
    let closed = docket.run(&["close", "ROSETTA", "-q"]);
    assert!(closed.ok(), "close failed: {}", closed.stderr());
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
    assert!(root.ok(), "guide failed: {}", root.stderr());
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
        assert!(run.ok(), "guide {topic} failed: {}", run.stderr());
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
    assert!(many.ok(), "guide failed: {}", many.stderr());
    let many = many.stdout();
    assert!(
        many.find("HANDOFF") < many.find("SPEC"),
        "topics are ordered by the ladder, not by the argument list: {many}"
    );

    let unknown = docket.run(&["guide", "nonsense"]);
    assert!(
        !unknown.ok(),
        "unknown topic succeeded: {}",
        unknown.stdout()
    );
    let stderr = unknown.stderr();
    for topic in ["handoff", "relay", "spec"] {
        assert!(stderr.contains(topic), "{stderr}");
    }
}

#[test]
fn doctrine_lives_only_in_the_guide() {
    let docket = Docket::new();

    let agent = docket.run(&["help", "agent"]);
    assert!(!agent.ok(), "help agent still resolves: {}", agent.stdout());

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
        assert!(shown.ok(), "show failed: {}", shown.stderr());
        assert_eq!(shown.stdout(), format!("Body of {name}.\n"));

        let located = docket.run(&["path", id]);
        assert!(located.ok(), "path failed: {}", located.stderr());
        assert_eq!(located.stdout().trim_end(), path.to_str().unwrap());
    }
}

#[test]
fn promotion_climbs_the_ladder_and_stops_at_the_top() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, handoff_path) = docket.create(&project, "handoff", "CLIMB_LADDER");

    let to_relay = docket.run(&["promote", &id, "-q"]);
    assert!(to_relay.ok(), "promote failed: {}", to_relay.stderr());
    let relay_path = promoted_path(&to_relay.stdout());
    assert_eq!(tail(&relay_path, 2), format!("relays/{id}-CLIMB_LADDER.md"));
    assert!(relay_path.is_file());
    assert!(!handoff_path.exists(), "the handoff file should have moved");

    let to_spec = docket.run(&["promote", &id, "-q"]);
    assert!(to_spec.ok(), "promote failed: {}", to_spec.stderr());
    let spec_path = promoted_path(&to_spec.stdout());
    assert_eq!(
        tail(&spec_path, 3),
        format!("specs/{id}-CLIMB_LADDER/spec.md")
    );
    assert!(spec_path.is_file());
    assert!(!relay_path.exists(), "the relay file should have moved");

    let to_implementation = docket.run(&["promote", &id, "-q"]);
    assert!(
        to_implementation.ok(),
        "promote failed: {}",
        to_implementation.stderr()
    );
    assert_eq!(promoted_path(&to_implementation.stdout()), spec_path);
    assert!(front(&spec_path).contains("stage: implementation"));

    let past_the_top = docket.run(&["promote", &id, "-q"]);
    assert!(!past_the_top.ok());
    assert!(
        past_the_top.stderr().contains("docket close"),
        "stderr should point at close: {}",
        past_the_top.stderr()
    );
}

#[test]
fn promoting_a_handoff_straight_to_spec_carries_no_chain() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, _) = docket.create(&project, "handoff", "SKIP_RELAY_RUNG");

    let run = docket.run(&["promote", &id, "--to", "spec", "-q"]);
    assert!(run.ok(), "promote failed: {}", run.stderr());
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
    assert!(to_relay.ok(), "promote failed: {}", to_relay.stderr());
    let to_spec = docket.run(&["promote", &id, "-q"]);
    assert!(to_spec.ok(), "promote failed: {}", to_spec.stderr());

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
    assert!(second_run.ok(), "relay failed: {}", second_run.stderr());
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
    assert!(third_run.ok(), "relay failed: {}", third_run.stderr());
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

    assert!(!run.ok());
    assert!(
        run.stderr().contains("docket promote"),
        "stderr should point at promote: {}",
        run.stderr()
    );
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
    assert!(created.ok(), "create failed: {}", created.stderr());
    let (id, _) = id_and_path(&created.stdout());

    let renamed = docket.run(&["set", &id, "--name", "BODY_FIDELITY_TWO", "-q"]);
    assert!(renamed.ok(), "set failed: {}", renamed.stderr());

    let shown = docket.run(&["show", &id, "-q"]);
    assert!(shown.ok(), "show failed: {}", shown.stderr());
    assert_eq!(shown.stdout(), body);
}

#[test]
fn an_invalid_item_stays_listed_and_fails_doctor() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "spec", "DAMAGED_SPEC");
    drop_key(&path, "stage:");

    let listed = docket.run(&["list", "--project", &project]);
    assert!(listed.ok(), "list failed: {}", listed.stderr());
    assert!(
        listed
            .stdout()
            .lines()
            .any(|line| line.starts_with('!') && line.contains(&id) && line.contains("INVALID")),
        "an unparseable item is never hidden: {}",
        listed.stdout()
    );

    let doctor = docket.run(&["doctor"]);
    assert_ne!(doctor.code(), 0);
    assert!(doctor.stdout().contains("invalid"));
    assert!(doctor.stdout().contains(&id));
}

#[test]
fn set_repairs_invalid_frontmatter() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "spec", "DAMAGED_SPEC");
    drop_key(&path, "stage:");

    let healed = docket.run(&["set", &id, "-q"]);
    assert!(healed.ok(), "set failed: {}", healed.stderr());

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
    assert!(run.ok(), "reorder failed: {}", run.stderr());

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
        assert!(listed.ok(), "list failed: {}", listed.stderr());
        listed_ids(&listed.stdout())
    };
    assert_eq!(listing(), ids);

    let moved = docket.run(&["reorder", &ids[3], "--top", "--project", &project, "-q"]);
    assert!(moved.ok(), "reorder failed: {}", moved.stderr());
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
    assert!(moved.ok(), "reorder failed: {}", moved.stderr());
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
    assert!(moved.ok(), "reorder failed: {}", moved.stderr());
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
fn closing_removes_an_item_and_history_keeps_it() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "handoff", "FINISHED_WORK");

    let closed = docket.run(&["close", &id, "-q"]);
    assert!(closed.ok(), "close failed: {}", closed.stderr());
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

    let run = docket.run_with(&["close", &id, "-q"], &[("DOCKET_GIT", "")]);
    assert!(!run.ok(), "close should refuse without git");
    assert!(
        run.stderr().contains("git"),
        "the refusal names what is missing: {}",
        run.stderr()
    );
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
        assert!(closed.ok(), "close failed: {}", closed.stderr());
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
    assert!(run.ok(), "set failed: {}", run.stderr());

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
    assert!(run.ok(), "announce failed: {}", run.stderr());
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
    assert!(run.ok(), "announce failed: {}", run.stderr());
    assert!(!docket.root.exists(), "announce created {:?}", docket.root);
}

#[test]
fn announce_is_silent_when_empty_and_emits_hook_json_when_not() {
    let docket = Docket::new();
    let project = docket.project("proj");

    let silent = docket.run(&["announce", "--project", &project]);
    assert!(silent.ok());
    assert_eq!(silent.stdout(), "");

    let (id, _) = docket.create(&project, "handoff", "OUTSTANDING_WORK");

    let roster = docket.run(&["announce", "--project", &project]);
    assert!(roster.ok(), "announce failed: {}", roster.stderr());
    assert!(roster.stdout().contains(&id));
    assert!(roster.stdout().contains("OUTSTANDING_WORK"));
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
    assert!(hook.ok(), "announce failed: {}", hook.stderr());
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
    assert!(formatted.ok(), "list failed: {}", formatted.stderr());
    assert!(formatted.stdout().starts_with('{'));
    assert!(formatted.stdout().contains(&format!("\"id\": \"{id}\"")));

    let flagged = docket.run(&["list", "--json", "--project", &project]);
    assert!(flagged.ok(), "list failed: {}", flagged.stderr());
    assert_eq!(flagged.stdout(), formatted.stdout());
}

#[test]
fn agent_and_uncoloured_human_output_carry_no_escapes() {
    let docket = Docket::new();
    let project = docket.project("proj");
    docket.create(&project, "handoff", "PLAIN_TEXT_ONLY");

    let agent = docket.run(&["--project", &project]);
    assert!(agent.ok(), "the bare listing failed: {}", agent.stderr());
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
    assert!(human.ok(), "list failed: {}", human.stderr());
    assert!(!human.stdout().contains('\x1b'), "{}", human.stdout());
}

#[test]
fn help_serves_topics_and_commands_and_rejects_neither() {
    let docket = Docket::new();

    for topic in ["ladder", "metadata"] {
        let run = docket.run(&["help", topic]);
        assert!(run.ok(), "help {topic} failed: {}", run.stderr());
        assert!(!run.stdout().trim().is_empty(), "help {topic} said nothing");
    }

    // A name that is not a topic falls through to the subcommand of that name.
    let command = docket.run(&["help", "create"]);
    assert!(command.ok(), "help create failed: {}", command.stderr());
    assert!(command.stdout().contains("--allow-missing"));

    let unknown = docket.run(&["help", "nonsense"]);
    assert!(!unknown.ok());
    assert_ne!(unknown.code(), 0);
    assert!(
        unknown.stderr().contains("ladder, metadata"),
        "stderr should list the topics: {}",
        unknown.stderr()
    );
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
    assert!(run.ok(), "close failed: {}", run.stderr());
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
    assert!(docket.run(&["close", &id, "-q"]).ok());

    let located = docket.run(&["path", &id]);
    assert!(!located.ok(), "a closed item should not resolve");
    assert!(
        located.stderr().contains("diff-filter=D"),
        "the error points at history: {}",
        located.stderr()
    );
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
    assert!(created.ok(), "create failed: {}", created.stderr());
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
