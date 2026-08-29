//! The binary against a daemon that is a shell script.
//!
//! What these pin is the half a unit test cannot reach: that the flags this
//! program builds are the flags it thinks it builds, and that removal happens
//! in the one order that can work. The fake is written at run time rather than
//! committed, so the fixture sits beside the assertion that reads it.

// Clippy's in-test carve-outs (see `clippy.toml`) reach `#[test]` functions and
// `#[cfg(test)]` modules — not the helpers beside them. An integration test
// crate is test code end to end, so the carve-out belongs at its root, where its
// scope is still exactly the tests.
#![allow(clippy::expect_used)]

use std::path::Path;

use assert_cmd::Command;
use tempfile::TempDir;

/// A daemon that answers from files and records what it was told to remove.
struct Fake {
    home: TempDir,
}

impl Fake {
    fn new() -> Self {
        let home = tempfile::tempdir().expect("a temporary directory");
        let script = home.path().join("docker");
        std::fs::write(
            &script,
            r#"#!/bin/sh
log() { printf '%s\n' "$*" >> "$FAKE_LOG"; }
say() { cat "$FAKE_FIX/$1" 2>/dev/null; exit 0; }
case "$1$2" in
  "ps-q") exit "${FAKE_DAEMON:-0}" ;;
  "ps-a") say containers.ids ;;
esac
case "$1" in
  inspect) say containers.inspect ;;
  volume) case "$2" in
      ls) say volumes ;;
      rm)
        log "volume rm $3"
        [ -z "$FAKE_REFUSE" ] || { echo "volume is in use" >&2; exit 1; }
        exit 0 ;;
    esac ;;
  network) case "$2" in
      ls) say networks ;;
      rm) log "network rm $3"; exit 0 ;;
    esac ;;
  rm) log "container rm $4"; exit 0 ;;
  compose)
    log "compose $*"
    dir=""
    prev=""
    for arg in "$@"; do
      [ "$prev" = "--project-directory" ] && dir="$arg"
      prev="$arg"
    done
    for arg in "$@"; do
      if [ "$arg" = "config" ]; then
        answer="$FAKE_FIX/config.$(basename "$dir")"
        [ -f "$answer" ] || exit 1
        cat "$answer"
        exit 0
      fi
    done
    # A teardown takes the containers with it; whatever the fixture still
    # lists afterwards is what outlived it.
    : > "$FAKE_FIX/containers.ids"
    : > "$FAKE_FIX/containers.inspect"
    exit "${FAKE_COMPOSE:-1}"
    ;;
esac
exit 0
"#,
        )
        .expect("the fake daemon");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755))
                .expect("the fake daemon is executable");
        }
        Self { home }
    }

    fn docker(&self) -> std::path::PathBuf {
        self.home.path().join("docker")
    }

    fn fixtures(&self) -> std::path::PathBuf {
        let dir = self.home.path().join("fix");
        std::fs::create_dir_all(&dir).expect("a fixture directory");
        dir
    }

    fn answer(&self, name: &str, body: &str) {
        std::fs::write(self.fixtures().join(name), body).expect("a fixture");
    }

    fn log(&self) -> std::path::PathBuf {
        self.home.path().join("log")
    }

    /// What it was told to remove, in order. Questions are not removals.
    fn removals(&self) -> Vec<String> {
        self.said()
            .into_iter()
            .filter(|line| !line.starts_with("compose "))
            .collect()
    }

    /// Every invocation, verbatim — the argv contract this program relies on.
    fn said(&self) -> Vec<String> {
        std::fs::read_to_string(self.log())
            .unwrap_or_default()
            .lines()
            .map(ToOwned::to_owned)
            .collect()
    }

    fn command(&self, cwd: &Path) -> Command {
        let mut command = Command::cargo_bin("compose-gc").expect("the binary under test");
        command
            .current_dir(cwd)
            .env("COMPOSE_GC_DOCKER", self.docker())
            .env("FAKE_FIX", self.fixtures())
            .env("FAKE_LOG", self.log())
            .env_remove("COMPOSE_FILE");
        command
    }
}

/// A repository with one abandoned nested worktree holding a two-service stack.
fn repository() -> (TempDir, std::path::PathBuf) {
    let root = tempfile::tempdir().expect("a temporary directory");
    let repo = root
        .path()
        .canonicalize()
        .expect("the real path of the temporary root")
        .join("gmrepo");
    std::fs::create_dir_all(&repo).expect("the repository directory");
    Command::new("git")
        .args(["init", "-q"])
        .current_dir(&repo)
        .assert()
        .success();
    (root, repo)
}

fn abandoned(fake: &Fake, repo: &Path) {
    let dead = repo.join(".claude/worktrees/wt-a").display().to_string();
    fake.answer("containers.ids", "a1\na2\n");
    fake.answer(
        "containers.inspect",
        &format!(
            "a1 {{\"com.docker.compose.project\":\"wt-a\",\"com.docker.compose.project.config_files\":\"{dead}/compose.yaml\"}}\n\
             a2 {{\"com.docker.compose.project\":\"wt-a\",\"com.docker.compose.project.config_files\":\"{dead}/compose.yaml\"}}\n"
        ),
    );
    fake.answer("volumes", "wt-a_data wt-a\nwt-a_cache wt-a\n");
    fake.answer("networks", "net-a wt-a\n");
}

#[test]
fn a_machine_without_docker_holds_no_compose_state() {
    let (_root, repo) = repository();
    let mut command = Command::cargo_bin("compose-gc").expect("the binary under test");
    // An empty override disables the tool outright — the seam that reaches the
    // absent path without uninstalling anything.
    command
        .current_dir(&repo)
        .env("COMPOSE_GC_DOCKER", "")
        .assert()
        .success()
        .stdout(predicates::str::contains("docker not installed"));
}

#[test]
fn an_unreachable_daemon_is_not_a_failure() {
    let (_root, repo) = repository();
    let fake = Fake::new();
    fake.command(&repo)
        .env("FAKE_DAEMON", "1")
        .assert()
        .success()
        .stdout(predicates::str::contains("daemon unreachable"));
}

#[test]
fn every_container_goes_before_the_first_volume() {
    // The retired script reclaimed after each container in turn, so a
    // two-service stack asked for its volumes back while its own second
    // container still held them — and reported the refusal as a failure.
    let (_root, repo) = repository();
    let fake = Fake::new();
    abandoned(&fake, &repo);

    fake.command(&repo)
        .assert()
        .success()
        .stdout(predicates::str::contains("swept 1 orphaned project"));

    let removals = fake.removals();
    let first_volume = removals
        .iter()
        .position(|line| line.starts_with("volume rm"))
        .expect("volumes were removed");
    let last_container = removals
        .iter()
        .rposition(|line| line.starts_with("container rm"))
        .expect("containers were removed");
    assert!(last_container < first_volume, "{removals:?}");
    assert_eq!(
        removals
            .iter()
            .filter(|l| l.starts_with("container rm"))
            .count(),
        2
    );
    assert_eq!(
        removals
            .iter()
            .filter(|l| l.starts_with("volume rm"))
            .count(),
        2
    );
    assert_eq!(
        removals
            .iter()
            .filter(|l| l.starts_with("network rm"))
            .count(),
        1
    );
}

#[test]
fn a_dry_run_removes_nothing_and_still_counts() {
    let (_root, repo) = repository();
    let fake = Fake::new();
    abandoned(&fake, &repo);

    fake.command(&repo)
        .arg("-n")
        .assert()
        .success()
        .stdout(predicates::str::contains("would sweep 1 orphaned project"))
        .stdout(predicates::str::contains(
            "would remove 2 containers, 2 volumes, 1 network",
        ));
    assert!(fake.removals().is_empty());
}

#[test]
fn a_live_worktree_is_never_swept() {
    let (_root, repo) = repository();
    let fake = Fake::new();
    abandoned(&fake, &repo);
    std::fs::create_dir_all(repo.join(".claude/worktrees/wt-a")).expect("the live worktree");

    fake.command(&repo)
        .assert()
        .success()
        .stdout(predicates::str::contains("swept 0 orphaned projects"));
    assert!(fake.removals().is_empty());
}

#[test]
fn outside_a_repository_there_is_nothing_to_be_scoped_to() {
    let elsewhere = tempfile::tempdir().expect("a temporary directory");
    let fake = Fake::new();
    fake.command(elsewhere.path())
        .assert()
        .success()
        .stdout(predicates::str::contains("not inside a git repository"));
}

#[test]
fn tearing_down_a_directory_that_is_not_there_is_a_misuse() {
    // Reported as "nothing to tear down" by the retired script, which let a
    // typo pass for a clean teardown.
    let (root, _repo) = repository();
    let fake = Fake::new();
    fake.command(root.path())
        .args(["down", "/no/such/place"])
        .assert()
        .code(2)
        .stderr(predicates::str::contains("no such directory"));
}

#[test]
fn a_directory_designating_no_stack_is_nothing_to_tear_down() {
    let (root, _repo) = repository();
    let empty = root.path().join("empty");
    std::fs::create_dir_all(&empty).expect("an empty directory");
    let fake = Fake::new();
    fake.command(root.path())
        .args(["down"])
        .arg(&empty)
        .assert()
        .success()
        .stdout(predicates::str::contains("nothing to tear down"));
}

#[test]
fn a_stack_that_will_not_load_is_a_failure_not_an_absence() {
    // Both refusals look identical in Docker's exit status; the retired script
    // told them apart by matching its English.
    let (root, _repo) = repository();
    let broken = root.path().join("broken");
    std::fs::create_dir_all(&broken).expect("a directory");
    std::fs::write(broken.join("compose.yaml"), "services: [\n").expect("a broken compose file");
    let fake = Fake::new();
    fake.command(root.path())
        .args(["down"])
        .arg(&broken)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("failed"));
}

/// A worktree whose stack the fake will answer for.
fn stack(fake: &Fake, dir: &Path, project: &str) {
    let dir = dir.display();
    fake.answer("containers.ids", "c1\n");
    fake.answer(
        "containers.inspect",
        &format!(
            "c1 {{\"com.docker.compose.project\":\"{project}\",\"com.docker.compose.project.config_files\":\"{dir}/compose.yaml\"}}\n"
        ),
    );
    std::fs::create_dir_all(dir.to_string()).expect("the worktree");
    std::fs::write(
        format!("{dir}/compose.yaml"),
        "services:\n  x:\n    image: alpine\n",
    )
    .expect("a compose file");
}

#[test]
fn a_teardown_asks_for_every_profile_and_the_volumes() {
    // The whole reason `down` exists: a service behind a `profiles:` key is
    // invisible to a plain `down`, survives the teardown, and holds the
    // project network open so that leaks too.
    let (root, _repo) = repository();
    let fake = Fake::new();
    let worktree = root.path().join("wt-a");
    stack(&fake, &worktree, "wt-a");
    fake.answer("volumes", "");
    fake.answer("networks", "");

    fake.command(root.path())
        .env("FAKE_COMPOSE", "0")
        .args(["down"])
        .arg(&worktree)
        .assert()
        .success()
        .stdout(predicates::str::contains("tore down the compose project"));

    let asked = fake.said().join("\n");
    assert!(asked.contains("--profile *"), "{asked}");
    assert!(asked.contains("down -v --remove-orphans"), "{asked}");
}

#[test]
fn a_resource_that_outlives_a_teardown_is_named() {
    // Docker reports this in English on stdout, which is what the retired
    // script matched. Asked of the daemon instead: whatever is still labelled
    // to the project is what survived, and it can be named.
    let (root, _repo) = repository();
    let fake = Fake::new();
    let worktree = root.path().join("wt-a");
    stack(&fake, &worktree, "wt-a");
    fake.answer("volumes", "wt-a_data wt-a\n");
    fake.answer("networks", "");

    fake.command(root.path())
        .env("FAKE_COMPOSE", "0")
        .args(["down"])
        .arg(&worktree)
        .assert()
        .code(1)
        .stdout(predicates::str::contains(
            "volume wt-a_data outlived the teardown",
        ))
        .stderr(predicates::str::contains(
            "the reconcile sweep still has work",
        ));
}

#[test]
fn compose_names_the_main_project_when_its_stack_is_down() {
    // The directory name is only Compose's *default*. A `name:` in the file or
    // a `COMPOSE_PROJECT_NAME` in the environment outranks it, and a stranded
    // volume set is matched on name alone — so guessing here reaps the wrong
    // project's data.
    let (_root, repo) = repository();
    let fake = Fake::new();
    fake.answer("containers.ids", "");
    fake.answer("containers.inspect", "");
    fake.answer("volumes", "declared_data declared\nwt-b_data wt-b\n");
    fake.answer("networks", "");
    fake.answer("config.gmrepo", "{\"name\": \"declared\"}\n");
    std::fs::write(
        repo.join("compose.yaml"),
        "name: declared\nservices:\n  x:\n    image: alpine\n",
    )
    .expect("a compose file");

    fake.command(&repo)
        .assert()
        .success()
        .stdout(predicates::str::contains("swept 1 orphaned project"));
    assert_eq!(fake.removals(), vec!["volume rm wt-b_data".to_owned()]);
}

#[test]
fn a_live_worktrees_declared_name_takes_it_out_of_range() {
    // Its directory says one thing and its compose file another. Matched on
    // the directory alone, this sweep would destroy a live worktree's data.
    let (_root, repo) = repository();
    let fake = Fake::new();
    let live = repo.join(".claude/worktrees/wt-live");
    std::fs::create_dir_all(&live).expect("the live worktree");
    fake.answer("containers.ids", "");
    fake.answer("containers.inspect", "");
    fake.answer("volumes", "gmrepo_data gmrepo\ndeclared_data declared\n");
    fake.answer("networks", "");
    fake.answer("config.wt-live", "{\"name\": \"declared\"}\n");

    fake.command(&repo)
        .assert()
        .success()
        .stdout(predicates::str::contains("swept 0 orphaned projects"));
    assert!(fake.removals().is_empty());
}

#[test]
fn a_removal_the_daemon_refuses_is_a_failure_that_names_itself() {
    let (_root, repo) = repository();
    let fake = Fake::new();
    abandoned(&fake, &repo);

    fake.command(&repo)
        .env("FAKE_REFUSE", "1")
        .assert()
        .code(1)
        .stdout(predicates::str::contains("survived"))
        .stdout(predicates::str::contains("volume is in use"));
}

#[test]
fn a_pipe_gets_no_escape_codes() {
    // `assert_cmd` never gives the child a terminal, which is the same
    // condition a caller redirecting to a file is in.
    let (_root, repo) = repository();
    let fake = Fake::new();
    abandoned(&fake, &repo);

    let output = fake.command(&repo).arg("-n").output().expect("a run");
    let text = String::from_utf8(output.stdout).expect("text");
    assert!(!text.contains('\x1b'), "{text}");
}

#[test]
fn a_designated_compose_file_makes_an_empty_directory_a_failure() {
    // `COMPOSE_FILE` names the files outright, wherever they are, so a
    // directory holding none of the four candidates still has a stack — and a
    // teardown that fails there is a failure rather than an absence.
    let (root, _repo) = repository();
    let empty = root.path().join("empty");
    std::fs::create_dir_all(&empty).expect("an empty directory");
    let fake = Fake::new();
    fake.command(root.path())
        .env("COMPOSE_FILE", "somewhere/compose.yaml")
        .args(["down"])
        .arg(&empty)
        .assert()
        .code(1)
        .stderr(predicates::str::contains("failed"));
}

#[test]
fn a_neighbour_that_is_not_a_worktree_spares_nothing() {
    // Everything beside the repository is a directory too. Only the two
    // worktree layouts count, or an unrelated neighbour's name would take a
    // project of the same name out of range.
    let (root, repo) = repository();
    std::fs::create_dir_all(root.path().join("other")).expect("a neighbour");
    let fake = Fake::new();
    fake.answer("containers.ids", "");
    fake.answer("containers.inspect", "");
    fake.answer("volumes", "gmrepo_data gmrepo\nother_data other\n");
    fake.answer("networks", "");

    fake.command(&repo)
        .assert()
        .success()
        .stdout(predicates::str::contains("swept 1 orphaned project"));
    assert_eq!(fake.removals(), vec!["volume rm other_data".to_owned()]);
}

#[test]
fn a_dry_run_teardown_reports_a_stack_it_can_see() {
    let (root, _repo) = repository();
    let worktree = root.path().join("wt-a");
    std::fs::create_dir_all(&worktree).expect("the worktree");
    std::fs::write(
        worktree.join("compose.yaml"),
        "services:\n  x:\n    image: alpine\n",
    )
    .expect("a compose file");
    let fake = Fake::new();
    fake.answer("containers.ids", "");
    fake.answer("containers.inspect", "");

    fake.command(root.path())
        .args(["down", "-n"])
        .arg(&worktree)
        .assert()
        .success()
        .stdout(predicates::str::contains("would tear down"));
    assert!(fake.removals().is_empty());
}

#[test]
fn a_dry_run_teardown_reports_a_stack_only_its_containers_reveal() {
    // The compose file went with the worktree's contents but the directory
    // survived — which is the shape this whole program exists for.
    let (root, _repo) = repository();
    let worktree = root.path().join("wt-a");
    std::fs::create_dir_all(&worktree).expect("the worktree");
    let fake = Fake::new();
    let dir = worktree.display();
    fake.answer("containers.ids", "c1\n");
    fake.answer(
        "containers.inspect",
        &format!(
            "c1 {{\"com.docker.compose.project\":\"wt-a\",\"com.docker.compose.project.config_files\":\"{dir}/compose.yaml\"}}\n"
        ),
    );

    fake.command(root.path())
        .args(["down", "-n"])
        .arg(&worktree)
        .assert()
        .success()
        .stdout(predicates::str::contains("would tear down"));
}
