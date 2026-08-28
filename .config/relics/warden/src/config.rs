//! What the guard is told to leave alone.
//!
//! Public, unlike the definition: it names paths that are already visible in
//! the tree, and the reason each is skipped is worth reading.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;

/// Where the configuration sits when nothing says otherwise.
const DEFAULT: &str = ".config/warden/config.toml";

/// Why the configuration could not be used. An absent file is not one of
/// these — it is the default, and the default guards everything.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// It is there but could not be read.
    #[error("reading {0}")]
    Unreadable(Utf8PathBuf, #[source] std::io::Error),
    /// It is there but is not the shape this expects.
    #[error("{0}: {1}")]
    Malformed(Utf8PathBuf, #[source] toml::de::Error),
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    #[serde(default)]
    warden: Table,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Table {
    /// Paths whose bytes the guard may not read as text, allowed anyway.
    #[serde(default)]
    binary_allowed: Vec<Utf8PathBuf>,
}

/// The paths this machine has decided about.
#[derive(Debug, Default)]
pub struct Config {
    binary_allowed: Vec<Utf8PathBuf>,
}

impl Config {
    /// The configuration at its default place under `home`, or the default
    /// when there is none.
    ///
    /// # Errors
    ///
    /// Whatever [`Config::load`] reports.
    pub fn discover(home: &Utf8Path) -> Result<Self, Error> {
        Self::load(&home.join(DEFAULT))
    }

    /// The configuration at `path`, or the default when it is absent.
    ///
    /// # Errors
    ///
    /// [`Error`], both variants of which name the file. Absence is not an
    /// error: a machine that has decided nothing guards everything, which is
    /// the safe direction to default in.
    pub fn load(path: &Utf8Path) -> Result<Self, Error> {
        let text = match fs_err::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(Error::Unreadable(path.to_owned(), e)),
        };
        let document: Document =
            toml::from_str(&text).map_err(|e| Error::Malformed(path.to_owned(), e))?;
        Ok(Self {
            binary_allowed: document.warden.binary_allowed,
        })
    }

    /// Whether this path may hold content the guard cannot read.
    #[must_use]
    pub fn allows_binary(&self, path: &Utf8Path) -> bool {
        self.binary_allowed.iter().any(|allowed| allowed == path)
    }
}
