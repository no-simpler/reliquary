//! `relic`, end to end.

mod harness;

use harness::{Sandbox, read};

#[test]
fn discovery_orders_the_public_lane_first_and_names_within_it() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "zeta", true);
    box_.interpreted(".config/relics", "alpha", true);
    box_.interpreted(".config/attic", "hidden", true);

    let assert = box_.relic().arg("list").assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    let alpha = text.find("alpha").unwrap_or(usize::MAX);
    let zeta = text.find("zeta").unwrap_or(0);
    let hidden = text.find("hidden").unwrap_or(0);
    assert!(alpha < zeta, "{text}");
    assert!(zeta < hidden, "the private lane came first:\n{text}");
    assert!(text.contains("Private relics"), "{text}");
}

#[test]
fn an_empty_private_lane_prints_no_section_at_all() {
    // Attic-safe: a lane with nothing readable in it must not announce itself.
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "alpha", true);
    let assert = box_.relic().arg("list").assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(!text.contains("Private relics"), "{text}");
}

#[test]
fn a_directory_with_no_manifest_is_not_a_relic() {
    let box_ = Sandbox::create();
    fs_err::create_dir_all(box_.at(".config/relics/notarelic").as_std_path()).expect("a dir");
    let assert = box_.relic().arg("list").assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(!text.contains("notarelic"), "{text}");
    assert!(text.contains("(none)"), "{text}");
}

#[test]
fn a_broken_manifest_is_reported_in_the_table_and_on_stderr() {
    // Warning about it and leaving it out of the table is how a relic
    // disappears: the operator reads the table.
    let box_ = Sandbox::create();
    box_.broken(".config/relics", "broken");
    let assert = box_.relic().arg("list").assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("broken"), "{text}");
    assert!(text.contains("unreadable manifest"), "{text}");
    let warned = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(warned.contains("line 1, column 7"), "{warned}");
}

#[test]
fn publishing_puts_a_name_on_path_with_its_owner() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "widget", true);
    box_.relic().args(["publish", "widget"]).assert().success();

    assert!(box_.at(".local/bin/widget").is_file());
    let registry = box_.registry();
    assert!(registry.contains("widget"), "{registry}");
    // The owner column is the relic's own name.
    assert!(
        registry
            .lines()
            .any(|l| l.split_whitespace().eq(["widget", "widget"])),
        "{registry}"
    );
}

#[test]
fn a_second_publish_is_a_no_op_rather_than_a_second_row() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "widget", true);
    box_.relic().args(["publish", "widget"]).assert().success();
    box_.relic().args(["publish", "widget"]).assert().success();
    assert_eq!(
        box_.registry()
            .lines()
            .filter(|l| l.starts_with("widget"))
            .count(),
        1
    );
}

#[test]
fn publish_all_is_what_bootstrap_hands_off_to_and_one_failure_does_not_stop_it() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "alpha", true);
    box_.interpreted(".config/attic", "hidden", true);
    box_.broken(".config/relics", "broken");

    let assert = box_.relic().args(["publish", "--all"]).assert().failure();
    let said = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(said.contains("broken"), "{said}");
    // The two that could publish, did.
    assert!(box_.at(".local/bin/alpha").is_file());
    assert!(box_.at(".local/bin/hidden").is_file());
}

#[test]
fn status_reports_the_wiring_and_the_deps() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "widget", true);

    let assert = box_.relic().args(["status", "widget"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("stage:     2 (in-house)"), "{text}");
    assert!(text.contains("runtime:   bash"), "{text}");
    assert!(text.contains("published: no"), "{text}");
    assert!(text.contains("not on PATH"), "{text}");
    assert!(text.contains("deps:      ok"), "{text}");

    box_.relic().args(["publish", "widget"]).assert().success();
    let assert = box_.relic().args(["status", "widget"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("published: yes"), "{text}");
    assert!(text.contains("(owner: widget)"), "{text}");
}

#[test]
fn a_missing_brew_dep_is_a_refusal_at_publish_and_a_line_in_status() {
    let box_ = Sandbox::create();
    let dir = box_.interpreted(".config/relics", "widget", true);
    fs_err::write(
        dir.join("relic.toml").as_std_path(),
        "[relic]\nname = \"widget\"\nruntime = \"bash\"\n\
         runtime-exemption = \"a fixture\"\nbrew-deps = [\"definitely-not-installed\"]\n",
    )
    .expect("a manifest");

    let assert = box_.relic().args(["status", "widget"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("deps:      missing"), "{text}");
    assert!(text.contains("definitely-not-installed"), "{text}");

    // Load-bearing, not documentation: it fails closed at publish time.
    box_.relic().args(["publish", "widget"]).assert().failure();
    assert!(!box_.at(".local/bin/widget").exists());
}

#[test]
fn a_runtime_floor_is_a_floor_and_not_a_pin() {
    let box_ = Sandbox::create();
    let dir = box_.interpreted(".config/relics", "widget", true);
    fs_err::write(
        dir.join("relic.toml").as_std_path(),
        "[relic]\nname = \"widget\"\nruntime = \"python\"\n\
         runtime-exemption = \"a fixture\"\nmin-runtime-version = \"99.0\"\n",
    )
    .expect("a manifest");
    let assert = box_.relic().args(["status", "widget"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("deps:      missing"), "{text}");
    assert!(text.contains("< required 99.0"), "{text}");
}

#[test]
fn the_cwd_names_the_relic_when_nothing_else_does() {
    let box_ = Sandbox::create();
    let dir = box_.interpreted(".config/relics", "widget", true);

    for from in [dir.clone(), dir.join("src")] {
        let assert = box_
            .relic()
            .current_dir(from.as_std_path())
            .arg("status")
            .assert()
            .success();
        let text = String::from_utf8_lossy(&assert.get_output().stdout);
        assert!(text.contains("widget"), "from {from}:\n{text}");
    }

    // And outside any relic, it says so rather than guessing.
    box_.relic().arg("status").assert().failure();
}

#[test]
fn an_external_relic_is_reported_by_status_and_refused_by_everything_else() {
    let box_ = Sandbox::create();
    let assert = box_.relic().args(["status", "bb"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("stage:     3 (external)"), "{text}");
    assert!(text.contains("not on this machine"), "{text}");

    let assert = box_.relic().args(["publish", "bb"]).assert().failure();
    let said = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(said.contains("own publish flow"), "{said}");
    let assert = box_.relic().args(["test", "bb"]).assert().failure();
    let said = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(said.contains("own test flow"), "{said}");
}

#[test]
fn an_external_relic_that_is_present_reports_its_git_state() {
    let box_ = Sandbox::create();
    let repo = box_.at("Developer/bb");
    fs_err::create_dir_all(repo.as_std_path()).expect("a repo dir");
    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(repo.as_std_path())
        .status()
        .expect("git ran");

    let assert = box_.relic().args(["status", "bb"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("present:   yes"), "{text}");
    assert!(text.contains("git:       clean"), "{text}");

    fs_err::write(repo.join("dirty.txt").as_std_path(), b"x").expect("a file");
    let assert = box_.relic().args(["status", "bb"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("git:       dirty"), "{text}");
}

#[test]
fn the_doctor_sees_declared_names_that_never_reached_path() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "widget", true);

    let assert = box_.relic().arg("doctor").assert().failure();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("Unpublished entrypoints"), "{text}");
    assert!(text.contains("widget"), "{text}");
    assert!(text.contains("issue(s) found"), "{text}");

    box_.relic().args(["publish", "widget"]).assert().success();
    let assert = box_.relic().arg("doctor").assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("healthy"), "{text}");
}

#[test]
fn the_doctor_sees_a_registry_row_whose_file_went_away() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "widget", true);
    box_.relic().args(["publish", "widget"]).assert().success();
    fs_err::remove_file(box_.at(".local/bin/widget").as_std_path()).expect("gone");

    let assert = box_.relic().arg("doctor").assert().failure();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("Orphan registry entries"), "{text}");
    assert!(text.contains("registry --prune"), "{text}");
}

#[test]
fn the_runtime_stance_is_the_worklist_and_never_a_failure() {
    let box_ = Sandbox::create();
    // Not rust, and no reason given.
    box_.interpreted(".config/relics", "bare", false);
    box_.relic().args(["publish", "bare"]).assert().success();

    let assert = box_.relic().arg("doctor").assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("Runtime stance"), "{text}");
    assert!(text.contains("bare (bash)"), "{text}");
    // Informational: a relic awaiting its rewrite has to keep publishing.
    assert!(text.contains("healthy"), "{text}");
}

#[test]
fn an_unmanaged_binary_in_the_lane_is_reported_and_does_not_grade() {
    let box_ = Sandbox::create();
    let foreign = box_.at(".local/bin/foreign");
    fs_err::write(foreign.as_std_path(), "#!/bin/sh\n").expect("a binary");
    harness::executable(&foreign);

    let assert = box_.relic().arg("doctor").assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("Unmanaged files"), "{text}");
    assert!(text.contains("foreign"), "{text}");
    assert!(text.contains("healthy"), "{text}");
}

#[test]
fn an_empty_registry_says_it_does_not_exist_yet() {
    let box_ = Sandbox::create();
    let assert = box_.relic().arg("registry").assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("does not exist yet"), "{text}");
}

#[test]
fn scaffolding_a_rust_relic_writes_a_member_and_leaves_it_unpublished() {
    let box_ = Sandbox::create();
    fs_err::write(
        box_.at(".config/relics/Cargo.toml").as_std_path(),
        "[workspace]\nmembers = [\n    \"alpha\",\n    \"zulu\",\n]\n",
    )
    .expect("a workspace");

    let assert = box_
        .relic()
        .args(["scaffold", "newthing"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("relics are Rust unless exempted"), "{text}");

    let dir = box_.at(".config/relics/newthing");
    assert!(dir.join("Cargo.toml").is_file());
    assert!(dir.join("src/main.rs").is_file());
    assert!(
        !dir.join("entrypoints").exists(),
        "a compiled relic has none"
    );
    assert!(
        box_.manifest(".config/relics", "newthing")
            .contains("runtime = \"rust\"")
    );

    let workspace = read(&box_.at(".config/relics/Cargo.toml"));
    assert_eq!(
        workspace,
        "[workspace]\nmembers = [\n    \"alpha\",\n    \"newthing\",\n    \"zulu\",\n]\n"
    );
    // Nothing is published until there is something to publish.
    assert!(box_.registry().is_empty());
}

#[test]
fn scaffolding_promotes_a_stage_one_util_and_infers_its_runtime() {
    let box_ = Sandbox::create();
    let util = box_.at(".config/bin/promote-me");
    fs_err::write(util.as_std_path(), "#!/usr/bin/env bash\necho hi\n").expect("a util");
    harness::executable(&util);

    let assert = box_
        .relic()
        .args(["scaffold", "promote-me", "-e", "a fixture"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("inferred runtime 'bash'"), "{text}");
    assert!(text.contains("published: yes"), "{text}");

    assert!(!util.exists(), "the Stage-1 source was left behind");
    assert!(
        box_.at(".config/relics/promote-me/src/promote-me")
            .is_file()
    );
    assert!(box_.at(".local/bin/promote-me").is_file());
    assert!(
        box_.manifest(".config/relics", "promote-me")
            .contains("a fixture")
    );
}

#[test]
fn a_rust_scaffold_keeps_a_promoted_script_beside_the_skeleton() {
    // A Stage-1 script cannot be promoted into a compiled relic as-is, and
    // deleting it would be the one thing nobody could undo.
    let box_ = Sandbox::create();
    let util = box_.at(".config/bin/portme");
    fs_err::write(util.as_std_path(), "#!/usr/bin/env python3\nprint('x')\n").expect("a util");
    harness::executable(&util);

    let assert = box_
        .relic()
        .args(["scaffold", "portme", "-r", "rust"])
        .assert()
        .success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("port-me"), "{text}");
    assert!(
        box_.at(".config/relics/portme/src/portme.port-me")
            .is_file()
    );
}

#[test]
fn the_stance_is_enforced_where_it_is_cheap_to_follow() {
    let box_ = Sandbox::create();
    let assert = box_
        .relic()
        .args(["scaffold", "thing", "-r", "bash"])
        .assert()
        .failure();
    let said = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(said.contains("--exempt"), "{said}");
    assert!(
        !box_.at(".config/relics/thing").exists(),
        "it was laid down anyway"
    );
}

#[test]
fn scaffolding_over_an_existing_relic_is_refused() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "widget", true);
    let assert = box_.relic().args(["scaffold", "widget"]).assert().failure();
    let said = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(said.contains("already exists"), "{said}");
}

#[test]
fn a_name_that_is_not_a_filename_is_refused_by_the_parser() {
    let box_ = Sandbox::create();
    for bad in ["../evil", "a/b", ".hidden"] {
        box_.relic().args(["scaffold", bad]).assert().code(2);
    }
    assert!(!box_.at(".config/relics/evil").exists());
}

#[test]
fn an_unambiguous_prefix_resolves_and_an_ambiguous_one_names_the_candidates() {
    let box_ = Sandbox::create();
    box_.relic().arg("li").assert().success();
    box_.relic().arg("doc").assert().success();
    let assert = box_.relic().arg("m").assert().code(2);
    let said = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        said.contains("mutants") || said.contains("migrate"),
        "{said}"
    );
}

#[test]
fn an_unknown_relic_is_a_refusal_and_an_unknown_command_is_misuse() {
    let box_ = Sandbox::create();
    box_.relic().args(["status", "nosuch"]).assert().code(1);
    box_.relic().arg("frobnicate").assert().code(2);
}

#[test]
fn a_bash_relic_runs_its_own_suite() {
    let box_ = Sandbox::create();
    let dir = box_.interpreted(".config/relics", "widget", true);
    fs_err::create_dir_all(dir.join("tests").as_std_path()).expect("a tests dir");
    let runner = dir.join("tests/run.sh");
    fs_err::write(
        runner.as_std_path(),
        "#!/usr/bin/env bash\necho 'the suite ran'\nexit ${SUITE_RC:-0}\n",
    )
    .expect("a runner");
    harness::executable(&runner);

    // `assay` is not in this sandbox, so shell-lint degrades loudly and the
    // suite still runs — the bare-machine case, asserted rather than assumed.
    let assert = box_.relic().args(["test", "widget"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("the suite ran"), "{text}");
    let said = String::from_utf8_lossy(&assert.get_output().stderr);
    assert!(
        said.contains("shell unlinted"),
        "an unlinted run said nothing: {said}"
    );

    box_.relic()
        .args(["test", "widget"])
        .env("SUITE_RC", "1")
        .assert()
        .failure();
}

#[test]
fn a_relic_with_no_tests_says_so_rather_than_passing_silently() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "widget", true);
    let assert = box_.relic().args(["test", "widget"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("no tests/ directory"), "{text}");
}

#[test]
fn an_override_script_replaces_the_default_for_its_operation_only() {
    let box_ = Sandbox::create();
    let dir = box_.interpreted(".config/relics", "widget", true);
    fs_err::create_dir_all(dir.join("scripts").as_std_path()).expect("a scripts dir");
    let script = dir.join("scripts/publish.sh");
    fs_err::write(
        script.as_std_path(),
        "#!/usr/bin/env bash\necho 'the override ran'\n",
    )
    .expect("an override");
    harness::executable(&script);

    let assert = box_.relic().args(["publish", "widget"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("the override ran"), "{text}");
    // It replaced the publish, so nothing reached PATH.
    assert!(!box_.at(".local/bin/widget").exists());
    // And `test` is untouched by it.
    let assert = box_.relic().args(["test", "widget"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(!text.contains("the override ran"), "{text}");
}

#[test]
fn cover_and_mutants_refuse_a_relic_that_is_not_compiled() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "widget", true);
    for args in [vec!["test", "widget", "--cover"], vec!["mutants", "widget"]] {
        let assert = box_.relic().args(&args).assert().failure();
        let said = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(said.contains("only rust relics"), "{args:?}: {said}");
    }
}

#[test]
fn update_republishes_a_compiled_relic_and_leaves_an_interpreted_one_alone() {
    let box_ = Sandbox::create();
    box_.interpreted(".config/relics", "widget", true);
    // Publishing is what update does for a compiled relic; an interpreted one
    // is already its own artifact, so there is nothing to rebuild.
    box_.relic().args(["update", "widget"]).assert().success();
    assert!(box_.registry().is_empty());
}

#[test]
fn an_update_override_is_the_periodic_slot_and_runs_instead() {
    let box_ = Sandbox::create();
    let dir = box_.interpreted(".config/relics", "widget", true);
    fs_err::create_dir_all(dir.join("scripts").as_std_path()).expect("a scripts dir");
    let script = dir.join("scripts/update.sh");
    fs_err::write(
        script.as_std_path(),
        "#!/usr/bin/env bash\necho 'the periodic job ran'\n",
    )
    .expect("an override");
    harness::executable(&script);
    let assert = box_.relic().args(["update", "widget"]).assert().success();
    let text = String::from_utf8_lossy(&assert.get_output().stdout);
    assert!(text.contains("the periodic job ran"), "{text}");
}

/// The compiled path, for real: a cargo workspace in the sandbox, a relic
/// scaffolded into it, and the gates run over it.
///
/// Everything else here reaches the interpreted branch. This is the branch that
/// publishes every binary on the machine, so it is worth one real build.
mod compiled {
    use super::harness::Sandbox;

    /// The sandbox's `PATH`, plus wherever cargo lives on this machine.
    ///
    /// Kept out of the default harness deliberately: a bare `PATH` is what the
    /// other tests are for, and this is the one that needs a toolchain.
    fn with_cargo(box_: &Sandbox) -> assert_cmd::Command {
        let mut command = box_.relic();
        let cargo = which::which("cargo").ok();
        if let Some(dir) = cargo.as_ref().and_then(|p| p.parent()) {
            command.env(
                "PATH",
                format!("{}:{}:/usr/bin:/bin", box_.at(".local/bin"), dir.display()),
            );
        }
        command
    }

    /// Lay down a workspace root the scaffolded relic can be a member of.
    fn workspace(box_: &Sandbox) -> std::io::Result<()> {
        fs_err::write(
            box_.at(".config/relics/Cargo.toml").as_std_path(),
            "[workspace]\nresolver = \"3\"\nmembers = [\n]\n\n\
             [workspace.package]\nedition = \"2024\"\nrust-version = \"1.89\"\n\
             license = \"MIT\"\npublish = false\n",
        )?;
        fs_err::write(
            box_.at(".config/relics/rustfmt.toml").as_std_path(),
            "edition = \"2024\"\n",
        )
    }

    #[test]
    fn a_compiled_relic_builds_tests_and_publishes() {
        let box_ = Sandbox::create();
        workspace(&box_).expect("a workspace root");
        with_cargo(&box_)
            .args(["scaffold", "widget"])
            .assert()
            .success();

        with_cargo(&box_)
            .args(["test", "widget"])
            .assert()
            .success();
        with_cargo(&box_)
            .args(["publish", "widget"])
            .assert()
            .success();
        assert!(box_.at(".local/bin/widget").is_file());
        assert!(box_.registry().contains("widget"));

        // And `update` republishes it, which is what `up` runs.
        with_cargo(&box_)
            .args(["update", "widget"])
            .assert()
            .success();
    }

    #[test]
    fn the_lint_ratchet_gates_the_whole_workspace() {
        let box_ = Sandbox::create();
        workspace(&box_).expect("a workspace root");
        with_cargo(&box_)
            .args(["scaffold", "widget"])
            .assert()
            .success();
        fs_err::create_dir_all(box_.at(".config/relics/ratchets").as_std_path())
            .expect("a ratchets dir");
        let baseline = box_.at(".config/relics/ratchets/allows.toml");
        fs_err::write(baseline.as_std_path(), "widget = 0\n").expect("a baseline");
        with_cargo(&box_)
            .args(["test", "widget"])
            .assert()
            .success();

        // A suppression nobody accounted for.
        let main = box_.at(".config/relics/widget/src/main.rs");
        let body = super::read(&main);
        fs_err::write(
            main.as_std_path(),
            format!("#[allow(dead_code)]\nfn unused() {{}}\n{body}"),
        )
        .expect("a suppression");
        let assert = with_cargo(&box_)
            .args(["test", "widget"])
            .assert()
            .failure();
        let said = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(said.contains("lint ratchet"), "{said}");
        assert!(said.contains("baseline 0"), "{said}");

        // And it fails downward too: slack is suppressions that can be added
        // back unseen.
        fs_err::write(baseline.as_std_path(), "widget = 5\n").expect("a baseline");
        let assert = with_cargo(&box_)
            .args(["test", "widget"])
            .assert()
            .failure();
        let said = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(said.contains("lower it in"), "{said}");
    }

    #[test]
    fn a_package_with_no_baseline_fails_rather_than_passing_unwatched() {
        let box_ = Sandbox::create();
        workspace(&box_).expect("a workspace root");
        with_cargo(&box_)
            .args(["scaffold", "widget"])
            .assert()
            .success();
        fs_err::create_dir_all(box_.at(".config/relics/ratchets").as_std_path())
            .expect("a ratchets dir");
        fs_err::write(
            box_.at(".config/relics/ratchets/allows.toml").as_std_path(),
            "somethingelse = 0\n",
        )
        .expect("a baseline");
        let assert = with_cargo(&box_)
            .args(["test", "widget"])
            .assert()
            .failure();
        let said = String::from_utf8_lossy(&assert.get_output().stderr);
        assert!(said.contains("has no baseline"), "{said}");
    }
}
