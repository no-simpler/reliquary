//! Where coop keeps its declarations and its state.

use anyhow::{Context, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};

/// The state root override.
pub const ROOT_VAR: &str = "COOP_ROOT";
/// The config directory override.
pub const CONFIG_VAR: &str = "COOP_CONFIG";
/// The relic's own format variable.
pub const UI_VAR: &str = "COOP_UI";
/// The per-shell identity the hook exports.
pub const SESSION_VAR: &str = "COOP_SESSION";
/// The kill switch.
pub const DISABLE_VAR: &str = "COOP_DISABLE";

/// The two trees: one tracked, one machine-local.
#[derive(Clone, Debug)]
pub struct Paths {
    /// `~/.config/coop` — declarations and rendering preferences.
    pub config_dir: Utf8PathBuf,
    /// `~/.local/state/coop` — caches, per-session marks, the badge.
    pub state_dir: Utf8PathBuf,
}

impl Paths {
    /// Resolve both trees: an override first, then the default under `$HOME`.
    ///
    /// # Errors
    ///
    /// When a default is needed and there is no home directory.
    pub fn resolve() -> Result<Self> {
        let config_dir = match std::env::var_os(CONFIG_VAR) {
            Some(value) => from_os(value)?,
            None => home()?.join(".config").join("coop"),
        };
        let state_dir = match std::env::var_os(ROOT_VAR) {
            Some(value) => from_os(value)?,
            None => home()?.join(".local").join("state").join("coop"),
        };
        Ok(Self {
            config_dir,
            state_dir,
        })
    }

    /// Rendering preferences.
    pub fn config(&self) -> Utf8PathBuf {
        self.config_dir.join("config.toml")
    }

    /// The drop-in directory every declaration is read from.
    pub fn sources_dir(&self) -> Utf8PathBuf {
        self.config_dir.join("sources.d")
    }

    /// One asked source's cached report.
    pub fn cache(&self, id: &str) -> Utf8PathBuf {
        self.state_dir.join("cache").join(format!("{id}.json"))
    }

    /// The lock that keeps N shells from refreshing one source at once.
    pub fn refresh_lock(&self, id: &str) -> Utf8PathBuf {
        self.state_dir.join("cache").join(format!("{id}.lock"))
    }

    /// What one shell has already been shown.
    pub fn seen(&self, session: &str) -> Utf8PathBuf {
        self.state_dir.join("seen").join(session)
    }

    /// The directory behind [`Paths::seen`].
    pub fn seen_dir(&self) -> Utf8PathBuf {
        self.state_dir.join("seen")
    }

    /// The prompt segment, written by `tick` and read by the shell with a
    /// builtin. A file rather than stdout because the card owns stdout, and one
    /// exec has to carry both.
    pub fn badge(&self) -> Utf8PathBuf {
        self.state_dir.join("badge")
    }

    /// When each outstanding notice was first seen, so `doctor` can tell a nag
    /// from furniture.
    pub fn first_seen(&self) -> Utf8PathBuf {
        self.state_dir.join("first-seen.json")
    }

    /// The last measured tick, for the budget finding.
    pub fn timing(&self) -> Utf8PathBuf {
        self.state_dir.join("timing.json")
    }
}

/// `$HOME` as a UTF-8 path.
///
/// # Errors
///
/// When there is no home directory, or it is not UTF-8.
pub fn home() -> Result<Utf8PathBuf> {
    relic_core::path::home().ok_or_else(|| anyhow!("no home directory"))
}

/// Expand a leading `~` against `$HOME`, and leave every other path alone.
///
/// Declarations are tracked and travel between machines, so an absolute path
/// under one machine's home would be wrong on the next one.
///
/// # Errors
///
/// When expansion is needed and there is no home directory.
pub fn expand(path: &Utf8Path) -> Result<Utf8PathBuf> {
    let text = path.as_str();
    if text == "~" {
        return home();
    }
    match text.strip_prefix("~/") {
        Some(rest) => Ok(home()?.join(rest)),
        None => Ok(path.to_owned()),
    }
}

fn from_os(value: std::ffi::OsString) -> Result<Utf8PathBuf> {
    relic_core::path::utf8(std::path::PathBuf::from(value)).context("reading a coop path override")
}

#[cfg(test)]
mod tests {
    use super::expand;
    use camino::Utf8Path;

    #[test]
    fn an_absolute_path_is_left_alone() {
        let path = Utf8Path::new("/var/log/system.log");
        assert_eq!(expand(path).unwrap(), path);
    }

    #[test]
    fn a_tilde_inside_a_path_is_not_a_home_reference() {
        let path = Utf8Path::new("/tmp/~/not-home");
        assert_eq!(expand(path).unwrap(), path);
    }

    #[test]
    fn a_leading_tilde_becomes_the_home_directory() {
        let expanded = expand(Utf8Path::new("~/.config/coop")).unwrap();
        assert!(expanded.is_absolute());
        assert!(expanded.as_str().ends_with("/.config/coop"));
        assert!(!expanded.as_str().contains('~'));
    }
}
