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
        let output = Command::new(env!("CARGO_BIN_EXE_docket"))
            .args(args)
            .current_dir(&self.base)
            .env("DOCKET_ROOT", &self.root)
            .env("CLAUDECODE", "1")
            // HOME is where `doctor` looks for the session-start hook, so it
            // points at the temporary tree as well.
            .env("HOME", &self.base)
            .env_remove("DOCKET_UI")
            .output()
            .expect("running the docket binary");
        Run { output }
    }

    /// Opens an item with a body, and returns its id and the path `create`
    /// printed for it.
    fn create(&self, project: &str, kind: &str, title: &str) -> (String, PathBuf) {
        let body = format!("Body of {title}.\n");
        let run = self.run(&[
            "create",
            kind,
            "--title",
            title,
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

/// An item's frontmatter, without its body.
fn front(path: &Path) -> String {
    let text = fs::read_to_string(path).expect("reading the item");
    let rest = text
        .strip_prefix("---\n")
        .expect("the file opens with a `---` line");
    let end = rest.find("\n---\n").expect("the frontmatter is terminated");
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

/// Where closing an item puts it: same filename, one level down under
/// `archive/`.
fn archived_beside(path: &Path) -> PathBuf {
    let kind_dir = path.parent().expect("an item sits in a kind directory");
    kind_dir
        .parent()
        .expect("a kind directory sits in a project directory")
        .join("archive")
        .join(kind_dir.file_name().expect("the kind directory is named"))
        .join(path.file_name().expect("the item has a filename"))
}

/// Deletes one frontmatter key, which is how an item falls out of schema in the
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

    let (handoff, handoff_path) = docket.create(&project, "handoff", "Settle the intent");
    let (relay, relay_path) = docket.create(&project, "relay", "Carry the chain");
    let (spec, spec_path) = docket.create(&project, "spec", "Rosetta messenger");

    assert_eq!(
        tail(&handoff_path, 2),
        format!("handoffs/{handoff}-settle-the-intent.md")
    );
    assert_eq!(
        tail(&relay_path, 2),
        format!("relays/{relay}-carry-the-chain.md")
    );
    assert_eq!(
        tail(&spec_path, 3),
        format!("specs/{spec}-rosetta-messenger/spec.md")
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
        "--title",
        "Settle the intent",
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
fn creating_rejects_an_empty_or_overlong_title_or_tagline() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let create = |title: &str, tagline: &str| {
        docket.run(&[
            "create",
            "handoff",
            "--title",
            title,
            "--tagline",
            tagline,
            "--project",
            &project,
            "--allow-missing",
            "-q",
        ])
    };

    for (title, tagline, expected) in [
        ("   ", "A tagline.", "--title is required"),
        ("A title", "", "--tagline is required"),
        (TOO_LONG, "A tagline.", "--title is 156 characters"),
        ("A title", TOO_LONG, "--tagline is 156 characters"),
    ] {
        let run = create(title, tagline);
        assert!(!run.ok(), "expected a refusal: {}", run.stdout());
        assert!(run.stderr().contains(expected), "{}", run.stderr());
    }
    assert!(create("A title", "A tagline.").ok());
}

#[test]
fn set_and_relay_hold_the_same_limits_as_create() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (handoff, _) = docket.create(&project, "handoff", "A handoff");
    let (relay, _) = docket.create(&project, "relay", "A relay");

    for (flag, value, expected) in [
        ("--title", TOO_LONG, "--title is 156 characters"),
        ("--tagline", TOO_LONG, "--tagline is 156 characters"),
        ("--title", "  ", "--title is required"),
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
        "--title",
        "A successor",
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
        "--title",
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
    let (id, path) = docket.create(&project, "handoff", "A handoff");

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
fn help_states_the_limits_it_enforces() {
    let docket = Docket::new();
    let metadata = docket.run(&["help", "metadata"]).stdout();
    assert!(metadata.contains("72"), "{metadata}");
    assert!(metadata.contains("80"), "{metadata}");
}

#[test]
fn ids_resolve_from_any_project() {
    let docket = Docket::new();
    let alpha = docket.project("alpha");
    let beta = docket.project("beta");

    let (first, first_path) = docket.create(&alpha, "handoff", "Alpha work");
    let (second, second_path) = docket.create(&beta, "spec", "Beta work");
    assert_ne!(first, second);

    // No --project anywhere below: an id is enough to find an item.
    for (id, path, title) in [
        (&first, &first_path, "Alpha work"),
        (&second, &second_path, "Beta work"),
    ] {
        let shown = docket.run(&["show", id]);
        assert!(shown.ok(), "show failed: {}", shown.stderr());
        assert_eq!(shown.stdout(), format!("Body of {title}.\n"));

        let located = docket.run(&["path", id]);
        assert!(located.ok(), "path failed: {}", located.stderr());
        assert_eq!(located.stdout().trim_end(), path.to_str().unwrap());
    }
}

#[test]
fn promotion_climbs_the_ladder_and_stops_at_the_top() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, handoff_path) = docket.create(&project, "handoff", "Climb the ladder");

    let to_relay = docket.run(&["promote", &id, "-q"]);
    assert!(to_relay.ok(), "promote failed: {}", to_relay.stderr());
    let relay_path = promoted_path(&to_relay.stdout());
    assert_eq!(
        tail(&relay_path, 2),
        format!("relays/{id}-climb-the-ladder.md")
    );
    assert!(relay_path.is_file());
    assert!(!handoff_path.exists(), "the handoff file should have moved");

    let to_spec = docket.run(&["promote", &id, "-q"]);
    assert!(to_spec.ok(), "promote failed: {}", to_spec.stderr());
    let spec_path = promoted_path(&to_spec.stdout());
    assert_eq!(
        tail(&spec_path, 3),
        format!("specs/{id}-climb-the-ladder/spec.md")
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
    let (id, _) = docket.create(&project, "handoff", "Skip the relay rung");

    let run = docket.run(&["promote", &id, "--to", "spec", "-q"]);
    assert!(run.ok(), "promote failed: {}", run.stderr());
    let spec_path = promoted_path(&run.stdout());
    assert_eq!(
        tail(&spec_path, 3),
        format!("specs/{id}-skip-the-relay-rung/spec.md")
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
    let (id, _) = docket.create(&project, "handoff", "Additive climb");

    let to_relay = docket.run(&["promote", &id, "-q"]);
    assert!(to_relay.ok(), "promote failed: {}", to_relay.stderr());
    let to_spec = docket.run(&["promote", &id, "-q"]);
    assert!(to_spec.ok(), "promote failed: {}", to_spec.stderr());

    let front = front(&promoted_path(&to_spec.stdout()));
    assert!(front.contains("kind: spec"));
    assert!(front.contains("stage: design"));
    assert!(
        front.contains(&format!("chain: {id}")),
        "the chain minted at the relay rung survives — {front}"
    );
    assert!(front.contains("hop: 1"), "the hop survives — {front}");
}

#[test]
fn relaying_mints_a_successor_and_archives_the_predecessor() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (first, first_path) = docket.create(&project, "relay", "Wave one");

    let second_run = docket.run(&[
        "relay",
        &first,
        "--title",
        "Wave two",
        "--tagline",
        "Wave one landed green.",
        "-q",
    ]);
    assert!(second_run.ok(), "relay failed: {}", second_run.stderr());
    let (second, second_path) = id_and_path(&second_run.stdout());
    let second_front = front(&second_path);
    assert!(second_front.contains(&format!("chain: {first}")));
    assert!(second_front.contains("hop: 2"));
    assert!(second_front.contains(&format!("supersedes: {first}")));
    assert!(!first_path.exists());
    assert!(archived_beside(&first_path).is_file());

    let third_run = docket.run(&[
        "relay",
        &second,
        "--title",
        "Wave three",
        "--tagline",
        "Wave two landed green.",
        "-q",
    ]);
    assert!(third_run.ok(), "relay failed: {}", third_run.stderr());
    let (_, third_path) = id_and_path(&third_run.stdout());
    let third_front = front(&third_path);
    assert!(
        third_front.contains(&format!("chain: {first}")),
        "the chain is stable across hops — {third_front}"
    );
    assert!(third_front.contains("hop: 3"));
    assert!(third_front.contains(&format!("supersedes: {second}")));
    assert!(!second_path.exists());
    assert!(archived_beside(&second_path).is_file());
}

#[test]
fn relaying_a_handoff_points_at_promote() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, _) = docket.create(&project, "handoff", "Not a relay");

    let run = docket.run(&[
        "relay",
        &id,
        "--title",
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
        "--title",
        "Body fidelity",
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

    let retitled = docket.run(&["set", &id, "--title", "Body fidelity, retitled", "-q"]);
    assert!(retitled.ok(), "set failed: {}", retitled.stderr());

    let shown = docket.run(&["show", &id, "-q"]);
    assert!(shown.ok(), "show failed: {}", shown.stderr());
    assert_eq!(shown.stdout(), body);
}

#[test]
fn an_invalid_item_stays_listed_and_fails_doctor() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "spec", "Damaged spec");
    drop_key(&path, "stage:");

    let listed = docket.run(&["list", "--project", &project]);
    assert!(listed.ok(), "list failed: {}", listed.stderr());
    assert!(
        listed.stdout().contains(&format!("! {id} INVALID")),
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
    let (id, path) = docket.create(&project, "spec", "Damaged spec");
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
    let ids: Vec<String> = ["First", "Second", "Third", "Fourth"]
        .iter()
        .map(|title| docket.create(&project, "handoff", title).0)
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
    let ids: Vec<String> = ["First", "Second", "Third", "Fourth"]
        .iter()
        .map(|title| docket.create(&project, "handoff", title).0)
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
fn close_archives_an_item() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "handoff", "Finished work");

    let closed = docket.run(&["close", &id, "-q"]);
    assert!(closed.ok(), "close failed: {}", closed.stderr());
    assert!(!path.exists());
    assert!(archived_beside(&path).is_file());

    let open = docket.run(&["list", "--project", &project]);
    assert!(!listed_ids(&open.stdout()).contains(&id));
    let archived = docket.run(&["list", "--archived", "--project", &project]);
    assert_eq!(listed_ids(&archived.stdout()), vec![id]);
}

#[test]
fn delete_removes_an_item_outright() {
    let docket = Docket::new();
    let project = docket.project("proj");
    let (id, path) = docket.create(&project, "handoff", "Opened by mistake");

    let deleted = docket.run(&["delete", &id, "--force", "-q"]);
    assert!(deleted.ok(), "delete failed: {}", deleted.stderr());
    assert!(!path.exists());
    assert!(!archived_beside(&path).exists(), "delete leaves no copy");

    let open = docket.run(&["list", "--project", &project]);
    assert!(!listed_ids(&open.stdout()).contains(&id));
    let archived = docket.run(&["list", "--archived", "--project", &project]);
    assert!(!listed_ids(&archived.stdout()).contains(&id));
}

#[test]
fn announce_is_silent_when_empty_and_emits_hook_json_when_not() {
    let docket = Docket::new();
    let project = docket.project("proj");

    let silent = docket.run(&["announce", "--project", &project]);
    assert!(silent.ok());
    assert_eq!(silent.stdout(), "");

    let (id, _) = docket.create(&project, "handoff", "Outstanding work");

    let roster = docket.run(&["announce", "--project", &project]);
    assert!(roster.ok(), "announce failed: {}", roster.stderr());
    assert!(roster.stdout().contains(&format!("[{id}]")));
    assert!(roster.stdout().contains("Outstanding work"));

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
    let (id, _) = docket.create(&project, "handoff", "Machine readable");

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
    docket.create(&project, "handoff", "Plain text only");

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

    for topic in ["ladder", "metadata", "keys", "agent"] {
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
        unknown.stderr().contains("ladder, metadata, keys, agent"),
        "stderr should list the topics: {}",
        unknown.stderr()
    );
}

/// Kind is taken from the directory an item sits in, not from frontmatter that
/// may no longer parse. Otherwise closing a damaged spec files it as a handoff,
/// which loses the id from the filename and makes the item unreachable.
#[test]
fn closing_an_invalid_spec_keeps_it_reachable() {
    let docket = Docket::new();
    let project = docket.project("damaged-spec");
    let (id, path) = docket.create(&project, "spec", "A spec that will be damaged");
    drop_key(&path, "stage");

    let closed = docket.run(&["close", &id, "--project", &project, "-q"]);
    assert!(closed.ok(), "close failed: {}", closed.stderr());

    let located = docket.run(&["path", &id]);
    assert!(
        located.ok(),
        "the item became unreachable: {}",
        located.stderr()
    );
    assert_eq!(
        tail(&PathBuf::from(located.stdout().trim()), 4),
        format!("archive/specs/{id}-a-spec-that-will-be-damaged/spec.md")
    );

    let archived = docket.run(&["list", "--archived", "--project", &project]);
    assert!(
        archived.stdout().contains(&id),
        "vanished from the archive listing"
    );
    assert!(
        archived.stdout().contains("INVALID"),
        "should still read as invalid"
    );
}

/// Deleting a damaged spec takes its whole directory, not just the file inside
/// it, for the same reason.
#[test]
fn deleting_an_invalid_spec_removes_its_directory() {
    let docket = Docket::new();
    let project = docket.project("damaged-spec-delete");
    let (id, path) = docket.create(&project, "spec", "Another damaged spec");
    drop_key(&path, "stage");
    let spec_dir = path
        .parent()
        .expect("a spec lives in its own directory")
        .to_owned();

    let removed = docket.run(&["delete", &id, "--force", "--project", &project, "-q"]);
    assert!(removed.ok(), "delete failed: {}", removed.stderr());
    assert!(!spec_dir.exists(), "the spec directory was left behind");
}
