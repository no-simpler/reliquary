//! A scratch `HOME` no real relic, registry or dotfile repository can reach.
//!
//! Every root this binary reads hangs off `HOME`, which is what makes a sandbox
//! one environment variable rather than a flag on each command.

// One suppression, structural to what this file is: `clippy.toml`'s
// `allow-expect-in-tests` reaches `#[test]` functions and `#[cfg(test)]`
// modules, not a plain helper in an integration test's own module — and a
// harness that cannot build its world has nothing to report but that.
#![allow(
    dead_code,
    clippy::expect_used,
    reason = "a shared integration-test harness: unused from one binary, and panicking is its error contract"
)]

use camino::{Utf8Path, Utf8PathBuf};

/// The lifecycle document, carrying an external-relic list to parse.
const GRADUATION: &str = "\
# Graduation

### Known external relics

- `bb` — `~/Developer/bb`
- `halo` — `~/Developer/halo`

## Something else
";

/// One test's world.
pub struct Sandbox {
    _guard: tempfile::TempDir,
    /// Its `HOME`.
    pub home: Utf8PathBuf,
}

impl Sandbox {
    /// Build one, with the real publish helper and a template to scaffold from.
    ///
    /// # Panics
    ///
    /// When the scratch tree cannot be laid down.
    pub fn create() -> Self {
        let guard = tempfile::tempdir().expect("a scratch dir");
        let home = Utf8PathBuf::from_path_buf(
            guard
                .path()
                .canonicalize()
                .expect("a resolvable scratch dir"),
        )
        .expect("utf8 scratch path");

        for dir in [
            ".config/relics",
            ".config/attic",
            ".config/bin",
            ".config/reliquary/lib",
            ".local/bin",
            "work",
        ] {
            fs_err::create_dir_all(home.join(dir).as_std_path()).expect("a sandbox dir");
        }
        fs_err::write(
            home.join(".config/reliquary/GRADUATION.md").as_std_path(),
            GRADUATION,
        )
        .expect("a graduation doc");

        // The real helper: the sourced ABI two external repositories also call,
        // and the thing this binary must not have reimplemented.
        let real = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../reliquary/lib/install-on-path.sh");
        fs_err::copy(
            real.as_std_path(),
            home.join(".config/reliquary/lib/install-on-path.sh")
                .as_std_path(),
        )
        .expect("the publish helper");

        let sandbox = Self {
            _guard: guard,
            home,
        };
        sandbox.template();
        sandbox
    }

    /// A minimal relic skeleton for `scaffold` to copy.
    fn template(&self) {
        let dir = self.home.join(".config/reliquary/template");
        fs_err::create_dir_all(dir.join("src").as_std_path()).expect("a template");
        fs_err::create_dir_all(dir.join("entrypoints").as_std_path()).expect("a template");
        fs_err::write(
            dir.join("relic.toml").as_std_path(),
            "# Manifest for the CLI.\n\n[relic]\nname = \"\"                 # the published name\n\
             description = \"\"\nruntime = \"\"\nruntime-exemption = \"\"\n",
        )
        .expect("a template manifest");
        fs_err::write(dir.join("src/.gitkeep").as_std_path(), b"").expect("a keep");
        fs_err::write(dir.join("entrypoints/.gitkeep").as_std_path(), b"").expect("a keep");
    }

    /// A path under the sandbox.
    #[must_use]
    pub fn at(&self, rest: &str) -> Utf8PathBuf {
        self.home.join(rest)
    }

    /// The binary under test, pointed at this sandbox.
    ///
    /// **A deliberately bare `PATH`**: the sandbox's own lane plus the system
    /// directories, and nothing else. Two reasons, and both are the point.
    ///
    /// The sandbox lane has to be on it because the publish helper refuses to
    /// install anywhere `PATH` does not reach — the defect the fresh-machine
    /// proof found, which a sandbox must reproduce rather than paper over. And
    /// the *real* machine's lane must not be, or every relic this repository
    /// has published would answer inside a scratch `HOME` and the bare-machine
    /// paths would never be exercised.
    ///
    /// # Panics
    ///
    /// When the binary has not been built.
    pub fn relic(&self) -> assert_cmd::Command {
        let mut command = assert_cmd::Command::cargo_bin("relic").expect("the binary");
        command
            .env("HOME", self.home.as_str())
            .env("PATH", format!("{}:/usr/bin:/bin", self.at(".local/bin")))
            .env("NO_COLOR", "1")
            .env_remove("RELIC_UI")
            .current_dir(self.at("work").as_std_path());
        command
    }

    /// An interpreted relic with one entrypoint.
    ///
    /// # Panics
    ///
    /// When the tree cannot be written.
    pub fn interpreted(&self, lane: &str, name: &str, exempt: bool) -> Utf8PathBuf {
        let dir = self.at(lane).join(name);
        fs_err::create_dir_all(dir.join("src").as_std_path()).expect("a relic dir");
        fs_err::create_dir_all(dir.join("entrypoints").as_std_path()).expect("a relic dir");
        let why = if exempt {
            "runtime-exemption = \"a fixture, and the point is that it is not rust\"\n"
        } else {
            ""
        };
        fs_err::write(
            dir.join("relic.toml").as_std_path(),
            format!("[relic]\nname = \"{name}\"\nruntime = \"bash\"\n{why}"),
        )
        .expect("a manifest");
        let script = dir.join("src").join(name);
        fs_err::write(
            script.as_std_path(),
            format!("#!/usr/bin/env bash\necho {name}\n"),
        )
        .expect("a script");
        executable(&script);
        std::os::unix::fs::symlink(
            format!("../src/{name}"),
            dir.join("entrypoints").join(name).as_std_path(),
        )
        .expect("an entrypoint");
        dir
    }

    /// A directory holding a manifest that will not parse.
    ///
    /// # Panics
    ///
    /// When it cannot be written.
    pub fn broken(&self, lane: &str, name: &str) -> Utf8PathBuf {
        let dir = self.at(lane).join(name);
        fs_err::create_dir_all(dir.as_std_path()).expect("a relic dir");
        fs_err::write(dir.join("relic.toml").as_std_path(), "[relic\n").expect("a manifest");
        dir
    }

    /// The registry's text, or the empty string.
    #[must_use]
    pub fn registry(&self) -> String {
        fs_err::read_to_string(self.at(".local/bin/.reliquary-managed").as_std_path())
            .unwrap_or_default()
    }

    /// One relic's manifest text.
    #[must_use]
    pub fn manifest(&self, lane: &str, name: &str) -> String {
        fs_err::read_to_string(self.at(lane).join(name).join("relic.toml").as_std_path())
            .unwrap_or_default()
    }
}

/// Make a file runnable.
///
/// # Panics
///
/// When the permissions cannot be set.
pub fn executable(path: &Utf8Path) {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path.as_std_path(), std::fs::Permissions::from_mode(0o755))
        .expect("an executable file");
}

/// Read a file, or the empty string.
#[must_use]
pub fn read(path: &Utf8Path) -> String {
    fs_err::read_to_string(path.as_std_path()).unwrap_or_default()
}
