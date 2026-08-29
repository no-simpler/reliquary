//! The gates a relic passes: format, lint, suite — and the two slow ones.
//!
//! **Fast loop, slow gate.** `relic test` is `fmt → clippy → nextest` and must
//! stay fast, because agents route around slow commands. Coverage and mutation
//! are separate, deliberate invocations run at wave boundaries.
//!
//! Fail-fast in ascending cost, so the cheapest station reports first.

use std::process::{Command, Stdio};

use camino::{Utf8Path, Utf8PathBuf};
use relic_core::tool::Tool;

use crate::lane::Relic;
use crate::manifest::{Manifest, Runtime};
use crate::paths::Paths;
use crate::publish::{self, Error};
use crate::ratchet::{Baseline, Verdict, suppressions};

/// Which gate is being run.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Op {
    /// Format, lint, suite.
    Test,
    /// Coverage, against the committed floor.
    Cover,
    /// Mutation testing, the real assertion-quality gate.
    Mutants,
}

impl Op {
    /// The name of its override script, and of its own subcommand.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Test => "test",
            Self::Cover => "cover",
            Self::Mutants => "mutants",
        }
    }
}

/// Run `cmd`, streaming its output, and report whether it succeeded.
fn run(command: &mut Command) -> Result<bool, Error> {
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    command
        .status()
        .map(|status| status.success())
        .map_err(|error| Error::Refused(error.to_string()))
}

/// A cargo invocation in a directory.
fn cargo(dir: &Utf8Path) -> Result<(Tool, Command), Error> {
    let tool =
        Tool::find("cargo").ok_or_else(|| Error::Workspace("cargo not on PATH".to_owned()))?;
    let mut command = tool.in_dir(dir);
    command.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    Ok((tool, command))
}

/// Whether a cargo subcommand is installed.
fn have(subcommand: &str) -> bool {
    Tool::find(&format!("cargo-{subcommand}")).is_some()
}

/// Run the test gate for one relic.
///
/// # Errors
///
/// [`Error`] naming which gate refused.
pub fn test(paths: &Paths, relic: &Relic) -> Result<(), Error> {
    if let Some(script) = publish::override_script(&relic.dir, "test") {
        return publish::run_override(&script, &relic.dir);
    }
    let manifest = relic
        .manifest
        .as_ref()
        .map_err(|e| Error::Manifest(e.clone()))?;
    if manifest.runtime.is_compiled() {
        return compiled(paths, relic, manifest);
    }
    interpreted(relic, manifest)
}

/// Format, lint, ratchet, suite — plus the reverse cross-lane gate.
fn compiled(paths: &Paths, relic: &Relic, manifest: &Manifest) -> Result<(), Error> {
    let root = publish::workspace_root(&relic.dir)?;
    let shared = shared_crates(&root);
    let mut packages: Vec<String> = vec![manifest.name.clone()];
    packages.extend(shared.iter().cloned());
    let flags: Vec<String> = packages
        .iter()
        .flat_map(|p| ["-p".to_owned(), p.clone()])
        .collect();

    let (_, mut fmt) = cargo(&relic.dir)?;
    fmt.arg("fmt").args(&flags).arg("--check");
    if !run(&mut fmt)? {
        eprintln!("\nformatting: run `cargo fmt --all`");
        return Err(Error::Refused("test: formatting".to_owned()));
    }

    allow_ratchet(&root)?;

    // No `-D warnings`: a command-line group flag outranks every entry in
    // [workspace.lints] and collapses `warn` and `deny` into one level. The
    // table denies what the flag used to — policy in a committed file, not in
    // an invocation.
    let (_, mut clippy) = cargo(&relic.dir)?;
    clippy
        .arg("clippy")
        .args(&flags)
        .args(["--all-targets", "--all-features"]);
    if !run(&mut clippy)? {
        return Err(Error::Refused("test: clippy found something".to_owned()));
    }

    let (_, mut suite) = cargo(&relic.dir)?;
    if have("nextest") {
        suite.args(["nextest", "run"]).args(&flags);
    } else {
        suite.arg("test").args(&flags);
    }
    if !run(&mut suite)? {
        return Err(Error::Refused("test: the suite failed".to_owned()));
    }

    // Only when a shared crate was covered. A trailing condition here used to
    // be the function's exit status, so a relic using no shared crate reported
    // every gate passing and still failed.
    if !shared.is_empty() {
        cross_lane(paths, manifest)?;
    }
    Ok(())
}

/// The reverse cross-lane gate.
///
/// A shared crate is covered from each of its *public* dependents by the
/// package set above. Its private dependents are covered by nothing: that is
/// the workspace property a lane boundary cannot carry, and the encrypted lane
/// is a second workspace precisely because a member's name and version land in
/// a lockfile. So when a public run covered a shared crate, run the private
/// lane's own format, lints and suite as well.
///
/// It names no private relic — only the lane, which is already public
/// knowledge. A failure prints private names to the terminal; that is local,
/// and must never be redirected into a tracked file.
fn cross_lane(paths: &Paths, manifest: &Manifest) -> Result<(), Error> {
    let attic = &paths.private;
    if !attic.join("Cargo.toml").is_file() {
        return Ok(());
    }
    // cargo refuses a memberless virtual workspace, so an ungated step would
    // fail every public relic's tests until the lane holds its first member.
    let populated = fs_err::read_dir(attic.as_std_path())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .any(|entry| entry.path().join("Cargo.toml").is_file());
    if !populated {
        return Ok(());
    }

    println!(
        "relic[{}]: shared crate changed — gating the private lane",
        manifest.name
    );
    let (_, mut fmt) = cargo(attic)?;
    fmt.args(["fmt", "--all", "--check"]);
    if !run(&mut fmt)? {
        return Err(Error::Refused("private lane formatting".to_owned()));
    }
    let (_, mut clippy) = cargo(attic)?;
    clippy.args(["clippy", "--workspace", "--all-targets", "--all-features"]);
    if !run(&mut clippy)? {
        return Err(Error::Refused("private lane lints".to_owned()));
    }
    let (_, mut suite) = cargo(attic)?;
    if have("nextest") {
        // An untested private relic is its own suite's finding, not a reason to
        // fail the public relic that triggered this step.
        suite.args(["nextest", "run", "--workspace", "--no-tests=pass"]);
    } else {
        suite.args(["test", "--workspace"]);
    }
    if !run(&mut suite)? {
        return Err(Error::Refused("private lane tests".to_owned()));
    }
    Ok(())
}

/// Lint the shell that stays shell, then run whatever suite there is.
fn interpreted(relic: &Relic, manifest: &Manifest) -> Result<(), Error> {
    if manifest.runtime == Runtime::Bash {
        shell_lint(manifest)?;
    }
    let tests = relic.dir.join("tests");
    if !tests.is_dir() {
        println!(
            "relic[{}]: no tests/ directory; nothing to run",
            manifest.name
        );
        return Ok(());
    }
    match manifest.runtime {
        Runtime::Python => {
            let program = if Tool::find("pytest").is_some() {
                "pytest"
            } else {
                "python3"
            };
            let tool = Tool::find(program)
                .ok_or_else(|| Error::Refused(format!("{program} not on PATH")))?;
            let mut command = tool.in_dir(&relic.dir);
            if program == "pytest" {
                command.arg("tests/");
            } else {
                command.args(["-m", "unittest", "discover", "tests/"]);
            }
            if run(&mut command)? {
                Ok(())
            } else {
                Err(Error::Refused("test: the suite failed".to_owned()))
            }
        }
        Runtime::Bash => bash_suite(relic),
        Runtime::Fish | Runtime::Docker | Runtime::Rust => {
            println!(
                "relic[{}]: no default test runner for runtime {}",
                manifest.name, manifest.runtime
            );
            Ok(())
        }
    }
}

/// `tests/run.sh` if there is one, else every `tests/*.sh`.
fn bash_suite(relic: &Relic) -> Result<(), Error> {
    let runner = relic.dir.join("tests/run.sh");
    let bash = Tool::find("bash").ok_or_else(|| Error::Refused("bash not on PATH".to_owned()))?;
    if publish::executable(&runner) {
        let mut command = Command::new(runner.as_std_path());
        command.current_dir(relic.dir.as_std_path());
        return if run(&mut command)? {
            Ok(())
        } else {
            Err(Error::Refused("test: the suite failed".to_owned()))
        };
    }
    let mut scripts: Vec<Utf8PathBuf> = fs_err::read_dir(relic.dir.join("tests").as_std_path())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| Utf8PathBuf::from_path_buf(entry.path()).ok())
        .filter(|path| path.extension() == Some("sh"))
        .collect();
    scripts.sort();
    let mut ok = true;
    for script in &scripts {
        let mut command = bash.in_dir(&relic.dir);
        command.arg(script.as_std_path());
        ok &= run(&mut command)?;
    }
    if ok {
        Ok(())
    } else {
        Err(Error::Refused("test: the suite failed".to_owned()))
    }
}

/// Format and lint the shell, through `assay`'s station.
///
/// The station has no subset mode and takes no directory: its third gate is an
/// equality against a committed per-file suppression count, and a subset cannot
/// tell a file carrying no directives from one it was not asked about. So a
/// bash relic's test runs the whole population — a superset, whose findings are
/// all true.
///
/// Absent, the relic is unlinted and **says so** rather than passing quietly: a
/// gate that silently does nothing is worse than no gate, because it also
/// carries the belief that it is on. That degrade is deliberate — this binary
/// publishes `assay`, so on a bare machine the linter does not exist yet and
/// testing must still work.
fn shell_lint(manifest: &Manifest) -> Result<(), Error> {
    let Some(assay) = Tool::find("assay") else {
        eprintln!(
            "relic[{}]: assay not on PATH — shell unlinted",
            manifest.name
        );
        return Ok(());
    };
    let mut command = assay.command();
    command
        .args(["--quiet", "shell-lint"])
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if run(&mut command)? {
        Ok(())
    } else {
        Err(Error::Refused(
            "test: shell-lint found something".to_owned(),
        ))
    }
}

/// The lint ratchet, over the **whole workspace** rather than the relic named.
///
/// A suppression slipped into a package nobody tested is exactly the one a
/// per-relic check would miss.
fn allow_ratchet(root: &Utf8Path) -> Result<(), Error> {
    let file = root.join("ratchets/allows.toml");
    let Some(baseline) = Baseline::load(&file) else {
        return Ok(());
    };
    let mut failed = false;
    for (package, dir) in workspace_packages(root) {
        let verdict = Verdict::of(suppressions(&dir), baseline.get(&package));
        if let Some(said) = verdict.report(&package, &file) {
            eprintln!("{said}");
            failed = true;
        }
    }
    if failed {
        Err(Error::Refused("test: the lint ratchet moved".to_owned()))
    } else {
        Ok(())
    }
}

/// Every package in a workspace root: `<name>` and its directory.
///
/// Read from each manifest rather than assumed from the directory name — the
/// first `name =` in a `Cargo.toml` is `[package]`'s by construction.
#[must_use]
pub fn workspace_packages(root: &Utf8Path) -> Vec<(String, Utf8PathBuf)> {
    let mut out = Vec::new();
    let mut roots = vec![root.to_owned()];
    if root.join("crates").is_dir() {
        roots.push(root.join("crates"));
    }
    for parent in roots {
        let Ok(entries) = fs_err::read_dir(parent.as_std_path()) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let Ok(dir) = Utf8PathBuf::from_path_buf(entry.path()) else {
                continue;
            };
            if let Some(name) = package_name(&dir.join("Cargo.toml")) {
                out.push((name, dir));
            }
        }
    }
    out.sort();
    out
}

/// The `[package] name` of a manifest, when it has one.
#[must_use]
pub fn package_name(manifest: &Utf8Path) -> Option<String> {
    let body = fs_err::read_to_string(manifest.as_std_path()).ok()?;
    body.lines()
        .map(str::trim)
        .find_map(|line| {
            line.strip_prefix("name")?
                .trim_start()
                .strip_prefix('=')
                .map(|rest| rest.trim().trim_matches('"').to_owned())
        })
        .filter(|name| !name.is_empty())
}

/// Package names of the workspace's shared crates.
#[must_use]
pub fn shared_crates(root: &Utf8Path) -> Vec<String> {
    let mut names: Vec<String> = fs_err::read_dir(root.join("crates").as_std_path())
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter_map(|entry| Utf8PathBuf::from_path_buf(entry.path()).ok())
        .filter_map(|dir| package_name(&dir.join("Cargo.toml")))
        .collect();
    names.sort();
    names
}

/// Coverage, the slow gate.
///
/// The **whole workspace**, not the named relic: the ratchet holds one baseline
/// per package, and a profile collected from one relic's run reports every
/// other package as uncovered. That was a real gate defect — `cover` measured
/// one relic and gated all four baselines.
///
/// # Errors
///
/// [`Error`] when the tool is missing, the run fails, or a package is under its
/// committed floor.
pub fn cover(relic: &Relic) -> Result<(), Error> {
    let manifest = relic
        .manifest
        .as_ref()
        .map_err(|e| Error::Manifest(e.clone()))?;
    if !manifest.runtime.is_compiled() {
        return Err(Error::Refused(format!(
            "cover: only rust relics ({} is {})",
            manifest.name, manifest.runtime
        )));
    }
    if !have("llvm-cov") {
        return Err(Error::Refused(
            "cover: cargo-llvm-cov not installed (see ~/.config/cargo/crates.txt)".to_owned(),
        ));
    }
    let root = publish::workspace_root(&relic.dir)?;
    let (_, mut command) = cargo(&root)?;
    if have("nextest") {
        command.args(["llvm-cov", "nextest", "--workspace", "--summary-only"]);
    } else {
        command.args(["llvm-cov", "--workspace", "--summary-only"]);
    }
    if !run(&mut command)? {
        return Err(Error::Refused("cover: the coverage run failed".to_owned()));
    }
    coverage_ratchet(&root)
}

/// The coverage ratchet, read back through `cargo-llvm-cov`'s own gate.
///
/// **Regions, not lines**, and never a percentage scraped out of the summary
/// table: an exit code is the machine-readable interface and a table is
/// human-facing output. That is house rule 1, and this is where it would be
/// easiest to break.
///
/// Inert until the baselines are committed — a baseline computed on the fly is
/// not a ratchet, it is a moving target.
fn coverage_ratchet(root: &Utf8Path) -> Result<(), Error> {
    let file = root.join("ratchets/coverage.toml");
    let Some(baseline) = Baseline::load(&file) else {
        println!("coverage ratchet: no baseline at {file} — reported, not gated");
        return Ok(());
    };
    let mut failed = false;
    for (package, want) in baseline.packages() {
        let (_, mut command) = cargo(root)?;
        command
            .args(["llvm-cov", "report", "-p", package, "--summary-only"])
            .arg("--fail-under-regions")
            .arg(want.to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        let passed = command
            .status()
            .map(|status| status.success())
            .map_err(|error| Error::Refused(error.to_string()))?;
        if !passed {
            eprintln!("coverage ratchet: {package} is under its baseline of {want}% regions");
            failed = true;
        }
    }
    if failed {
        Err(Error::Refused(
            "cover: a package is under its baseline".to_owned(),
        ))
    } else {
        Ok(())
    }
}

/// Mutation testing: the real assertion-quality gate.
///
/// It mutates the code and checks whether the tests *fail*. Coverage alone is
/// gameable by exactly the behaviour it guards against — a test that executes a
/// line and asserts nothing scores the same as a real one — and an
/// assertion-free test kills no mutants.
///
/// # Errors
///
/// [`Error`] when the relic is not compiled, the tool is missing, or a mutant
/// survived.
pub fn mutants(relic: &Relic, extra: &[String]) -> Result<(), Error> {
    let manifest = relic
        .manifest
        .as_ref()
        .map_err(|e| Error::Manifest(e.clone()))?;
    if !manifest.runtime.is_compiled() {
        return Err(Error::Refused(format!(
            "mutants: only rust relics ({} is {})",
            manifest.name, manifest.runtime
        )));
    }
    if !have("mutants") {
        return Err(Error::Refused(
            "mutants: cargo-mutants not installed (see ~/.config/cargo/crates.txt)".to_owned(),
        ));
    }
    let (_, mut command) = cargo(&relic.dir)?;
    command
        .args(["mutants", "--package", &manifest.name])
        .args(extra);
    if run(&mut command)? {
        Ok(())
    } else {
        Err(Error::Refused("mutants: a mutant survived".to_owned()))
    }
}

/// Rebuild and republish, or run the relic's own periodic job.
///
/// # Errors
///
/// [`Error`] when the publish or the override refuses.
pub fn update(paths: &Paths, relic: &Relic) -> Result<(), Error> {
    if let Some(script) = publish::override_script(&relic.dir, "update") {
        return publish::run_override(&script, &relic.dir);
    }
    let manifest = relic
        .manifest
        .as_ref()
        .map_err(|e| Error::Manifest(e.clone()))?;
    if manifest.runtime.is_compiled() {
        // Publishing builds, so there is nothing to do first.
        return publish::publish(paths, relic);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Op, package_name};
    use camino::Utf8PathBuf;

    #[test]
    fn a_package_name_is_the_first_name_assignment() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let path =
            Utf8PathBuf::from_path_buf(dir.path().join("Cargo.toml")).expect("utf8 scratch path");
        fs_err::write(
            path.as_std_path(),
            "[package]\nname = \"widget\"\nversion = \"0.1.0\"\n\n[[bin]]\nname = \"other\"\n",
        )
        .expect("a manifest");
        assert_eq!(package_name(&path).as_deref(), Some("widget"));
    }

    #[test]
    fn a_manifest_that_is_not_there_names_no_package() {
        assert!(package_name(camino::Utf8Path::new("/nowhere/Cargo.toml")).is_none());
    }

    #[test]
    fn a_workspace_names_its_members_and_its_shared_crates() {
        let guard = tempfile::tempdir().expect("a scratch dir");
        let root = camino::Utf8PathBuf::from_path_buf(guard.path().to_path_buf())
            .expect("utf8 scratch path");
        let package = |rest: &str, name: &str| {
            let dir = root.join(rest);
            fs_err::create_dir_all(dir.as_std_path()).expect("a dir");
            fs_err::write(
                dir.join("Cargo.toml").as_std_path(),
                format!("[package]\nname = \"{name}\"\n"),
            )
            .expect("a manifest");
        };
        package("widget", "widget");
        package("gadget", "gadget");
        package("crates/shared", "shared-thing");
        // A directory with no manifest is not a package.
        fs_err::create_dir_all(root.join("ratchets").as_std_path()).expect("a dir");

        let found = super::workspace_packages(&root);
        let names: Vec<&str> = found.iter().map(|(name, _)| name.as_str()).collect();
        // Sorted by directory, so `crates/shared` precedes the top-level ones.
        assert_eq!(names.len(), 3);
        assert!(names.contains(&"widget"));
        assert!(names.contains(&"shared-thing"));
        assert_eq!(super::shared_crates(&root), ["shared-thing"]);
    }

    #[test]
    fn a_root_with_no_crates_directory_shares_nothing() {
        let guard = tempfile::tempdir().expect("a scratch dir");
        let root = camino::Utf8PathBuf::from_path_buf(guard.path().to_path_buf())
            .expect("utf8 scratch path");
        assert!(super::shared_crates(&root).is_empty());
    }

    #[test]
    fn every_op_names_its_override_script() {
        assert_eq!(Op::Test.as_str(), "test");
        assert_eq!(Op::Cover.as_str(), "cover");
        assert_eq!(Op::Mutants.as_str(), "mutants");
    }
}
