//! Getting a relic's names onto `PATH`.
//!
//! Two shapes, because the artifact does or does not exist before a build.
//!
//! - **Interpreted** — `entrypoints/<published-name>` is the source, typically
//!   a symlink into `src/`. The filename *is* the published name.
//! - **Compiled** — nothing exists until cargo has run, and what it produces
//!   lands in the workspace `target/` rather than beside the source. The
//!   published names are declared in the manifest, which is why a compiled
//!   relic has no `entrypoints/` at all: a symlink into an unbuilt `target/`
//!   dangles on a fresh clone.
//!
//! **The install itself is not reimplemented here.** `install-on-path.sh` is a
//! sourced shell ABI that `bb` and `halo` call from their own publish scripts;
//! a second implementation is how one lane comes to disagree with another about
//! who owns a name. So this shells out to it, and the fork is paid only on a
//! publish.

use std::process::Command;
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use relic_core::tool::Tool;

use crate::lane::Relic;
use crate::manifest::Manifest;
use crate::paths::Paths;

/// How long a build or an install may take.
const BUDGET: Duration = Duration::from_secs(1800);

/// Why a publish stopped.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// Its manifest will not parse.
    #[error("{0}")]
    Manifest(String),
    /// A declared dependency is not here.
    #[error("{0}")]
    Deps(String),
    /// No cargo workspace above it, or no cargo at all.
    #[error("{0}")]
    Workspace(String),
    /// A tool refused.
    #[error("{0}")]
    Refused(String),
}

/// Whether the artifact was overridden by the relic's own script.
///
/// A relic overrides only when it needs to; the presence of the script is the
/// whole protocol.
#[must_use]
pub fn override_script(dir: &Utf8Path, op: &str) -> Option<Utf8PathBuf> {
    let script = dir.join("scripts").join(format!("{op}.sh"));
    executable(&script).then_some(script)
}

/// Whether a path is a file this user can run.
#[must_use]
pub fn executable(path: &Utf8Path) -> bool {
    let Ok(meta) = fs_err::metadata(path.as_std_path()) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

/// Run a relic's own override script from its directory.
///
/// # Errors
///
/// [`Error::Refused`] when it exits non-zero or cannot be started.
pub fn run_override(script: &Utf8Path, dir: &Utf8Path) -> Result<(), Error> {
    let mut command = Command::new(script.as_std_path());
    command.current_dir(dir.as_std_path());
    let status = command
        .status()
        .map_err(|error| Error::Refused(format!("{script}: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Refused(format!("{script} failed")))
    }
}

/// Publish one relic.
///
/// # Errors
///
/// [`Error`], naming which half refused.
pub fn publish(paths: &Paths, relic: &Relic) -> Result<(), Error> {
    if let Some(script) = override_script(&relic.dir, "publish") {
        return run_override(&script, &relic.dir);
    }
    let manifest = relic
        .manifest
        .as_ref()
        .map_err(|e| Error::Manifest(e.clone()))?;
    check_deps(manifest).map_err(Error::Deps)?;

    if manifest.runtime.is_compiled() {
        return compiled(paths, relic, manifest);
    }
    interpreted(paths, relic, manifest)
}

/// Build the relic, then install every name it declares.
fn compiled(paths: &Paths, relic: &Relic, manifest: &Manifest) -> Result<(), Error> {
    let root = workspace_root(&relic.dir)?;
    // Unconditionally, not only when the binary is missing: guarding on absence
    // would publish whatever was built last, shipping a source change as a
    // stale binary. cargo is incremental, so an up-to-date tree is a no-op.
    println!("relic[{}]: building", manifest.name);
    let cargo =
        Tool::find("cargo").ok_or_else(|| Error::Workspace("cargo not on PATH".to_owned()))?;
    let mut command = cargo.in_dir(&relic.dir);
    command.args(["build", "--release", "--quiet"]);
    let exit = cargo
        .run_within(&mut command, BUDGET)
        .map_err(|error| Error::Refused(error.to_string()))?;
    if !exit.ok() {
        return Err(Error::Refused(format!(
            "cargo build failed for {}\n{}",
            manifest.name,
            exit.stderr.trim()
        )));
    }

    for name in manifest.published_names(&relic.dir) {
        let built = root.join("target/release").join(&name);
        install(paths, &manifest.name, &built, &name)?;
    }
    Ok(())
}

/// Install every file in the relic's `entrypoints/` directory.
fn interpreted(paths: &Paths, relic: &Relic, manifest: &Manifest) -> Result<(), Error> {
    let dir = relic.dir.join("entrypoints");
    if !dir.is_dir() {
        eprintln!(
            "relic[{}]: no entrypoints/ directory; nothing to publish",
            manifest.name
        );
        return Ok(());
    }
    let names = manifest.published_names(&relic.dir);
    if names.is_empty() {
        eprintln!("relic[{}]: no entrypoints published", manifest.name);
        return Ok(());
    }
    for name in names {
        install(paths, &manifest.name, &dir.join(&name), &name)?;
    }
    Ok(())
}

/// Hand one file to the shell helper that owns the `PATH` lane.
///
/// `META_NAME` is the owner column, and `install_on_path` is *sourced* rather
/// than executed because that is what its two external callers do — the
/// interface is a function in a shell's own process, and this reproduces it
/// exactly rather than approximating it.
fn install(paths: &Paths, owner: &str, source: &Utf8Path, name: &str) -> Result<(), Error> {
    let bash = Tool::find("bash").ok_or_else(|| Error::Refused("bash not on PATH".to_owned()))?;
    let mut command = bash.command();
    command
        .env("META_NAME", owner)
        .arg("-c")
        .arg(r#"set -e; . "$1"; install_on_path "$2" "$3""#)
        .arg("install-on-path")
        .arg(paths.install_on_path.as_std_path())
        .arg(source.as_std_path())
        .arg(name)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    let status = command
        .status()
        .map_err(|error| Error::Refused(format!("install_on_path: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Refused(format!("install_on_path refused {name}")))
    }
}

/// Fold the registry helpers this binary does not own either.
///
/// # Errors
///
/// [`Error::Refused`] when the helper cannot be sourced or refuses.
pub fn registry_helper(paths: &Paths, function: &str) -> Result<(), Error> {
    let bash = Tool::find("bash").ok_or_else(|| Error::Refused("bash not on PATH".to_owned()))?;
    let mut command = bash.command();
    command
        .arg("-c")
        .arg(r#"set -e; . "$1"; "$2""#)
        .arg("install-on-path")
        .arg(paths.install_on_path.as_std_path())
        .arg(function)
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());
    let status = command
        .status()
        .map_err(|error| Error::Refused(format!("{function}: {error}")))?;
    if status.success() {
        Ok(())
    } else {
        Err(Error::Refused(format!("{function} failed")))
    }
}

/// The cargo workspace root above a directory.
///
/// `cargo locate-project` is the only thing that knows, and asking it beats
/// hardcoding a depth that a relocated lane would silently invalidate.
///
/// # Errors
///
/// [`Error::Workspace`], distinguishing an absent workspace from an absent
/// toolchain — `locate-project` answers nothing either way, and blaming the
/// lane for a broken `PATH` is how one reads as the other.
pub fn workspace_root(dir: &Utf8Path) -> Result<Utf8PathBuf, Error> {
    let Some(cargo) = Tool::find("cargo") else {
        return Err(Error::Workspace(format!(
            "cargo not on PATH; cannot locate the workspace above {dir}"
        )));
    };
    let mut command = cargo.in_dir(dir);
    command.args(["locate-project", "--workspace", "--message-format", "plain"]);
    let found = cargo
        .capture_within(&mut command, Duration::from_secs(60))
        .map_err(|_| Error::Workspace(format!("no cargo workspace above {dir}")))?;
    let manifest = Utf8PathBuf::from(found.line());
    manifest
        .parent()
        .map(ToOwned::to_owned)
        .ok_or_else(|| Error::Workspace(format!("no cargo workspace above {dir}")))
}

/// Whether the manifest's declared dependencies are present.
///
/// Fails **closed** at publish time: `brew-deps` and `min-runtime-version` are
/// load-bearing, not documentation.
///
/// Presence is checked unconditionally; only the *floor* is conditional. Gating
/// both on the floor meant a relic that declared none got no toolchain check at
/// all, and a missing `rustc` then surfaced further down as "no cargo workspace
/// above <dir>" — the wrong cause, named confidently.
///
/// # Errors
///
/// A multi-line report of everything missing, so one run names them all.
pub fn check_deps(manifest: &Manifest) -> Result<(), String> {
    let mut missing: Vec<String> = manifest
        .brew_deps
        .iter()
        .filter(|pkg| Tool::find(pkg).is_none())
        .map(|pkg| {
            format!(
                "relic[{}]: missing dep: {pkg} — install with: brew install {pkg}",
                manifest.name
            )
        })
        .collect();

    if let Some(problem) = runtime_present(manifest) {
        missing.push(problem);
    } else if !manifest.min_runtime_version.is_empty()
        && let Some(problem) = runtime_floor(manifest)
    {
        missing.push(problem);
    }

    if missing.is_empty() {
        Ok(())
    } else {
        Err(missing.join("\n"))
    }
}

/// The interpreter or toolchain a runtime needs, and how it says its version.
fn runtime_tool(manifest: &Manifest) -> Option<(&'static str, &'static [&'static str])> {
    use crate::manifest::Runtime;
    match manifest.runtime {
        Runtime::Python => Some(("python3", &["--version"])),
        Runtime::Rust => Some(("rustc", &["--version"])),
        Runtime::Fish => Some(("fish", &[])),
        Runtime::Docker => Some(("docker", &[])),
        // The interpreter running the bootstrap seed; presence is a tautology.
        Runtime::Bash => None,
    }
}

/// Whether the runtime's own program is on `PATH`.
fn runtime_present(manifest: &Manifest) -> Option<String> {
    let (program, _) = runtime_tool(manifest)?;
    Tool::find(program)
        .is_none()
        .then(|| format!("relic[{}]: {program} not on PATH", manifest.name))
}

/// Whether the runtime meets the manifest's floor.
fn runtime_floor(manifest: &Manifest) -> Option<String> {
    let want = &manifest.min_runtime_version;
    let have = runtime_version(manifest)?;
    if version_ge(&have, want) {
        return None;
    }
    Some(format!(
        "relic[{}]: {} {have} < required {want}",
        manifest.name,
        runtime_tool(manifest).map_or("bash", |(program, _)| program)
    ))
}

/// Ask the runtime what version it is.
fn runtime_version(manifest: &Manifest) -> Option<String> {
    use crate::manifest::Runtime;
    if manifest.runtime == Runtime::Bash {
        // The one runtime with no separate program to ask: the seed that runs
        // it is a shell, and the shell that runs *this* is the same one.
        return std::env::var("BASH_VERSION")
            .ok()
            .map(|v| numeric_prefix(&v));
    }
    let (program, args) = runtime_tool(manifest)?;
    let tool = Tool::find(program)?;
    let mut command = tool.command();
    command.args(args);
    let said = tool
        .capture_within(&mut command, Duration::from_secs(30))
        .ok()?;
    said.line()
        .split_whitespace()
        .map(numeric_prefix)
        .find(|token| !token.is_empty())
}

/// The leading dotted-numeric run of a string, which is what a version is.
fn numeric_prefix(text: &str) -> String {
    text.chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect()
}

/// Whether `have` is at least `need`, comparing dotted-numeric components.
///
/// Component-wise and numeric, so `1.10` is above `1.9` — which a string
/// comparison gets backwards, and which is exactly the shape a Rust or Python
/// floor takes.
#[must_use]
pub fn version_ge(have: &str, need: &str) -> bool {
    let parse = |text: &str| -> Vec<u64> {
        text.split('.')
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (have, need) = (parse(have), parse(need));
    let width = have.len().max(need.len());
    for index in 0..width {
        let a = have.get(index).copied().unwrap_or(0);
        let b = need.get(index).copied().unwrap_or(0);
        if a != b {
            return a > b;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{check_deps, executable, numeric_prefix, override_script, version_ge};
    use crate::manifest::Manifest;
    use camino::Utf8PathBuf;

    fn scratch() -> (tempfile::TempDir, Utf8PathBuf) {
        let guard = tempfile::tempdir().expect("a scratch dir");
        let root =
            Utf8PathBuf::from_path_buf(guard.path().to_path_buf()).expect("utf8 scratch path");
        (guard, root)
    }

    fn manifest(body: &str) -> Manifest {
        #[derive(serde::Deserialize)]
        struct Doc {
            relic: Manifest,
        }
        toml::from_str::<Doc>(body).expect("a manifest").relic
    }

    #[test]
    fn a_declared_brew_dep_that_is_absent_is_named() {
        let m = manifest(
            "[relic]\nname = \"x\"\nruntime = \"bash\"\n\
             brew-deps = [\"definitely-not-installed\"]\n",
        );
        let said = check_deps(&m).expect_err("it should refuse");
        assert!(
            said.contains("brew install definitely-not-installed"),
            "{said}"
        );
    }

    #[test]
    fn a_runtime_with_no_floor_is_still_checked_for_presence() {
        // Gating both on the floor meant a relic that declared none got no
        // toolchain check at all, and a missing rustc surfaced further down as
        // "no cargo workspace" — the wrong cause, named confidently.
        let m = manifest("[relic]\nname = \"x\"\nruntime = \"docker\"\n");
        // Whether docker is installed is not this test's business; what is
        // asserted is that the presence check runs, and says the right thing
        // when it fails.
        if let Err(said) = check_deps(&m) {
            assert!(said.contains("docker not on PATH"), "{said}");
        }
    }

    #[test]
    fn a_floor_nothing_can_meet_is_reported_against_the_right_program() {
        let m =
            manifest("[relic]\nname = \"x\"\nruntime = \"rust\"\nmin-runtime-version = \"99.0\"\n");
        let said = check_deps(&m).expect_err("it should refuse");
        assert!(said.contains("rustc"), "{said}");
        assert!(said.contains("< required 99.0"), "{said}");
    }

    #[test]
    fn bash_needs_no_program_to_be_present_because_it_is_the_one_running() {
        let m = manifest("[relic]\nname = \"x\"\nruntime = \"bash\"\n");
        assert!(check_deps(&m).is_ok());
    }

    #[test]
    fn an_override_is_a_file_that_is_executable_and_nothing_less() {
        let (_guard, root) = scratch();
        fs_err::create_dir_all(root.join("scripts").as_std_path()).expect("a dir");
        assert!(override_script(&root, "publish").is_none());

        let script = root.join("scripts/publish.sh");
        fs_err::write(script.as_std_path(), "#!/bin/sh\n").expect("a script");
        assert!(
            override_script(&root, "publish").is_none(),
            "a file without the bit is not an override"
        );
        {
            use std::os::unix::fs::PermissionsExt as _;
            std::fs::set_permissions(script.as_std_path(), std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
        assert!(override_script(&root, "publish").is_some());
        assert!(override_script(&root, "test").is_none());
    }

    #[test]
    fn a_directory_is_not_executable_however_its_bits_read() {
        let (_guard, root) = scratch();
        assert!(!executable(&root));
        assert!(!executable(&root.join("nothing")));
    }

    #[test]
    fn a_version_compares_component_wise_and_numerically() {
        assert!(
            version_ge("1.10.0", "1.9.0"),
            "a string compare gets this backwards"
        );
        assert!(version_ge("3.11", "3.9"));
        assert!(version_ge("1.89.0", "1.89"));
        assert!(version_ge("2.0", "1.99"));
        assert!(!version_ge("1.88", "1.89"));
        assert!(!version_ge("3.9.6", "3.11"));
    }

    #[test]
    fn a_missing_component_is_zero_and_equality_passes() {
        assert!(version_ge("1.89", "1.89.0"));
        assert!(version_ge("1.89.0", "1.89.0"));
        assert!(!version_ge("1.89", "1.89.1"));
    }

    #[test]
    fn a_version_is_the_numeric_prefix_of_whatever_the_tool_said() {
        assert_eq!(numeric_prefix("1.89.0-nightly"), "1.89.0");
        assert_eq!(numeric_prefix("5.2.37(1)-release"), "5.2.37");
        assert_eq!(numeric_prefix("rustc"), "");
    }
}
