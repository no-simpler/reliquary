//! `relic.toml`, parsed once.
//!
//! The manifest is the source of truth for a relic's name, its runtime, and the
//! names it publishes. It became TOML in this programme's W4 for one reason:
//! sourced bash is a data format only a shell can read, and it blocked every
//! consumer that was not one — including this binary.
//!
//! **`deny_unknown_fields` is on, and deliberately.** This is a schema we own,
//! where an unrecognised key is a typo worth failing on. The carve-out in
//! `HARDENING.md` is for third-party schemas that gain fields on upgrade, and
//! this is not one.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;

/// What a relic is written in.
///
/// **Relics are Rust by default.** Any other choice records a
/// [`Manifest::runtime_exemption`] saying why, and `doctor` lists the ones that
/// have not — informationally, never as a failure, because a relic awaiting its
/// rewrite has to keep publishing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Runtime {
    /// Compiled, built out of the lane's cargo workspace.
    Rust,
    /// A Python script.
    Python,
    /// A bash script.
    Bash,
    /// A fish script.
    Fish,
    /// A shim that runs a container.
    Docker,
}

impl Runtime {
    /// How the tables spell it.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rust => "rust",
            Self::Python => "python",
            Self::Bash => "bash",
            Self::Fish => "fish",
            Self::Docker => "docker",
        }
    }

    /// Whether the artifact exists before anything is built.
    ///
    /// The whole reason publishing splits in two. An interpreted relic's
    /// `entrypoints/<name>` **is** the artifact, so the filename is the
    /// published name. A compiled one has nothing on disk until cargo has run,
    /// and what it produces lands in the workspace `target/` rather than beside
    /// the source — which is why its published names are *declared* and why a
    /// symlink into an unbuilt `target/` dangles on a fresh clone.
    #[must_use]
    pub fn is_compiled(self) -> bool {
        match self {
            Self::Rust => true,
            Self::Python | Self::Bash | Self::Fish | Self::Docker => false,
        }
    }

    /// Every runtime a manifest may name, for a usage message.
    #[must_use]
    pub fn every() -> [Self; 5] {
        [
            Self::Python,
            Self::Bash,
            Self::Fish,
            Self::Rust,
            Self::Docker,
        ]
    }
}

impl std::fmt::Display for Runtime {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl std::str::FromStr for Runtime {
    type Err = BadRuntime;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        Self::every()
            .into_iter()
            .find(|r| r.as_str() == text)
            .ok_or_else(|| BadRuntime(text.to_owned()))
    }
}

/// Text that is not a runtime this repository knows.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid runtime: {0} (one of python|bash|fish|rust|docker)")]
pub struct BadRuntime(pub String);

/// The file's shape.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    relic: Manifest,
}

/// One relic's manifest.
#[derive(Clone, Debug, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Manifest {
    /// The published name, and the owner column of everything it publishes.
    pub name: String,
    /// One line, for the tables.
    #[serde(default)]
    pub description: String,
    /// What it is written in.
    pub runtime: Runtime,
    /// Why it is not Rust. Required in spirit, never enforced here: a relic
    /// awaiting its rewrite must keep publishing, so `doctor` reports the
    /// omission instead.
    #[serde(default)]
    pub runtime_exemption: String,
    /// A floor on the interpreter or toolchain — never a pin.
    #[serde(default)]
    pub min_runtime_version: String,
    /// The names a compiled relic publishes. Silent means "just `name`".
    #[serde(default)]
    pub entrypoints: Vec<String>,
    /// Homebrew packages that must be on `PATH` before it publishes.
    #[serde(default)]
    pub brew_deps: Vec<String>,
    /// Free-form notes about what else it reaches for. Never enforced.
    #[serde(default)]
    pub external_deps: Vec<String>,
    /// Whether it is a container shim.
    #[serde(default)]
    pub docker: bool,
}

/// Why a manifest could not be read.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// There is no readable manifest there.
    #[error("no manifest at {0}")]
    Absent(Utf8PathBuf),
    /// There is one, and it is not valid.
    ///
    /// Rendered on one line. `toml`'s own `Display` is a caret diagram several
    /// lines tall, which is right for a compiler and wrong for a `warn:` line
    /// in a table that is about to print ten more.
    #[error("{path}: {message}")]
    Invalid {
        /// Which file.
        path: Utf8PathBuf,
        /// What the parser said, and where.
        message: String,
        /// The parser's own error, for a caller that wants the diagram.
        /// Boxed: it carries the whole document, and every fallible read here
        /// would otherwise pay for that in its return type.
        #[source]
        source: Box<toml::de::Error>,
    },
}

/// One line naming what the parser objected to, and where.
///
/// Line and column rather than the byte offset `toml` reports: a person reading
/// this is about to open the file, and no editor takes a byte offset.
fn one_line(error: &toml::de::Error, body: &str) -> String {
    let Some(span) = error.span() else {
        return error.message().to_owned();
    };
    let (line, column) = position(body, span.start);
    format!("{} (at line {line}, column {column})", error.message())
}

/// The one-based line and column of a byte offset.
#[must_use]
fn position(body: &str, offset: usize) -> (usize, usize) {
    let head = body.get(..offset.min(body.len())).unwrap_or(body);
    let line = head.matches('\n').count() + 1;
    let column = head
        .rsplit('\n')
        .next()
        .map_or(1, |last| last.chars().count() + 1);
    (line, column)
}

/// Where a relic's manifest is.
///
/// **Nothing else in the tree may test for one by name.** A second predicate is
/// how one lane comes to disagree with another about which directories are
/// relics at all.
#[must_use]
pub fn path(dir: &Utf8Path) -> Utf8PathBuf {
    dir.join("relic.toml")
}

/// Whether a directory holds a *readable* manifest.
///
/// Readable, not merely present: an encrypted private lane must reveal nothing,
/// and a file this process cannot open is a directory it knows nothing about.
#[must_use]
pub fn present(dir: &Utf8Path) -> bool {
    fs_err::File::open(path(dir).as_std_path()).is_ok()
}

impl Manifest {
    /// Read one.
    ///
    /// # Errors
    ///
    /// [`Error::Absent`] when there is no readable file, and [`Error::Invalid`]
    /// when there is one that will not parse — reported rather than skipped,
    /// because silence is how a relic disappears.
    pub fn load(dir: &Utf8Path) -> Result<Self, Error> {
        let file = path(dir);
        let body =
            fs_err::read_to_string(file.as_std_path()).map_err(|_| Error::Absent(file.clone()))?;
        toml::from_str::<Document>(&body)
            .map(|document| document.relic)
            .map_err(|source| Error::Invalid {
                path: file,
                message: one_line(&source, &body),
                source: Box::new(source),
            })
    }

    /// The names this relic publishes.
    ///
    /// A compiled relic declares them (defaulting to its own name); an
    /// interpreted one *is* its `entrypoints/` directory, so the filenames
    /// answer.
    #[must_use]
    pub fn published_names(&self, dir: &Utf8Path) -> Vec<String> {
        if self.runtime.is_compiled() {
            return if self.entrypoints.is_empty() {
                vec![self.name.clone()]
            } else {
                self.entrypoints.clone()
            };
        }
        let Ok(entries) = fs_err::read_dir(dir.join("entrypoints").as_std_path()) else {
            return Vec::new();
        };
        let mut names: Vec<String> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| entry.file_name().to_str().map(ToOwned::to_owned))
            .filter(|name| !name.starts_with('.'))
            .collect();
        names.sort();
        names
    }
}

#[cfg(test)]
mod tests {
    use super::{Manifest, Runtime, present};
    use camino::Utf8PathBuf;

    fn scratch() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 scratch path");
        (dir, root)
    }

    fn write(root: &Utf8PathBuf, body: &str) {
        fs_err::write(root.join("relic.toml").as_std_path(), body).expect("a manifest");
    }

    #[test]
    fn a_parse_failure_names_a_line_and_a_column() {
        assert_eq!(super::position("abc", 0), (1, 1));
        assert_eq!(super::position("abc", 2), (1, 3));
        assert_eq!(super::position("ab\ncd", 3), (2, 1));
        assert_eq!(super::position("ab\ncd", 5), (2, 3));
        // Past the end is the end, rather than a panic on a slice.
        assert_eq!(super::position("ab", 99), (1, 3));
    }

    #[test]
    fn a_minimal_manifest_reads() {
        let (_guard, root) = scratch();
        write(&root, "[relic]\nname = \"x\"\nruntime = \"rust\"\n");
        let manifest = Manifest::load(&root).expect("readable");
        assert_eq!(manifest.name, "x");
        assert_eq!(manifest.runtime, Runtime::Rust);
        assert!(manifest.entrypoints.is_empty());
    }

    #[test]
    fn an_unknown_key_is_a_typo_worth_failing_on() {
        let (_guard, root) = scratch();
        write(
            &root,
            "[relic]\nname = \"x\"\nruntime = \"rust\"\nrunitme = \"y\"\n",
        );
        assert!(Manifest::load(&root).is_err());
    }

    #[test]
    fn an_unknown_runtime_is_refused_rather_than_carried() {
        let (_guard, root) = scratch();
        write(&root, "[relic]\nname = \"x\"\nruntime = \"perl\"\n");
        assert!(Manifest::load(&root).is_err());
    }

    #[test]
    fn a_missing_manifest_and_a_broken_one_are_different_answers() {
        let (_guard, root) = scratch();
        assert!(matches!(
            Manifest::load(&root),
            Err(super::Error::Absent(_))
        ));
        write(&root, "[relic\nname = \"x\"\n");
        assert!(matches!(
            Manifest::load(&root),
            Err(super::Error::Invalid { .. })
        ));
        // A broken manifest is still *present*: discovery must report it, not
        // pass over the directory as though nothing were there.
        assert!(present(&root));
    }

    #[test]
    fn a_compiled_relic_declares_its_names_and_an_interpreted_one_is_its_directory() {
        let (_guard, root) = scratch();
        write(&root, "[relic]\nname = \"x\"\nruntime = \"rust\"\n");
        let manifest = Manifest::load(&root).expect("readable");
        assert_eq!(manifest.published_names(&root), ["x"]);

        write(
            &root,
            "[relic]\nname = \"x\"\nruntime = \"rust\"\nentrypoints = [\"a\", \"b\"]\n",
        );
        let manifest = Manifest::load(&root).expect("readable");
        assert_eq!(manifest.published_names(&root), ["a", "b"]);

        write(
            &root,
            "[relic]\nname = \"x\"\nruntime = \"bash\"\nentrypoints = [\"ignored\"]\n",
        );
        let manifest = Manifest::load(&root).expect("readable");
        // No entrypoints/ directory: an interpreted relic publishes nothing,
        // whatever the manifest claims.
        assert!(manifest.published_names(&root).is_empty());
        fs_err::create_dir_all(root.join("entrypoints").as_std_path()).expect("a dir");
        for name in ["z", "a", ".hidden"] {
            fs_err::write(root.join("entrypoints").join(name).as_std_path(), b"")
                .expect("an entrypoint");
        }
        assert_eq!(manifest.published_names(&root), ["a", "z"]);
    }

    #[test]
    fn only_rust_is_compiled() {
        assert!(Runtime::Rust.is_compiled());
        for interpreted in [
            Runtime::Python,
            Runtime::Bash,
            Runtime::Fish,
            Runtime::Docker,
        ] {
            assert!(!interpreted.is_compiled(), "{interpreted}");
        }
    }
}
