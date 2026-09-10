//! Optional configuration. Absent means defaults, which is the ordinary case.

use camino::Utf8PathBuf;
use serde::Deserialize;

/// How the daily session is composed beyond what is due.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Filler {
    /// Offer, as unscored practice, every slug still climbing the ladder. A
    /// slug at the cap drops out, so it can earn a cold entry at the cap
    /// interval rather than a warm one wearing the same label.
    #[default]
    BelowCap,
    /// Offer every active slug, every day. The ritual, at the cost of the
    /// cutover gate: nothing ever goes a week untouched, and `status` will say
    /// so rather than pretend otherwise.
    All,
    /// Offer nothing beyond what is due.
    None,
}

/// The hour a drill day begins, in local time. Drilling at one in the morning
/// belongs to the day before.
pub const DEFAULT_ROLLOVER_HOUR: i8 = 4;

/// Tries allowed per slug per sitting. Only the first scores.
pub const DEFAULT_MAX_ATTEMPTS: u8 = 3;

/// The file, as read.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Where the log lives. Defaults to `~/Trove/ark/rote`.
    pub root: Option<Utf8PathBuf>,
    /// Where the verifiers and the reminder cache live. Defaults to
    /// `~/.local/state/rote`.
    pub state: Option<Utf8PathBuf>,
    /// The hour a drill day begins, 0 to 23.
    pub rollover_hour: Option<i8>,
    /// Tries per slug per sitting.
    pub max_attempts: Option<u8>,
    /// How the daily session is composed.
    pub filler: Option<Filler>,
}

impl Config {
    /// Read a config file, treating an absent one as empty.
    ///
    /// # Errors
    ///
    /// When the file exists and will not parse. A malformed config is a loud
    /// refusal rather than a silent fall back to defaults: the defaults might
    /// point at a different tree than the one the log is in.
    pub fn read(path: &camino::Utf8Path) -> anyhow::Result<Self> {
        match fs_err::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// The rollover hour, clamped into a day.
    pub fn rollover_hour(&self) -> i8 {
        self.rollover_hour
            .unwrap_or(DEFAULT_ROLLOVER_HOUR)
            .clamp(0, 23)
    }

    /// Tries per slug per sitting, at least one.
    pub fn max_attempts(&self) -> u8 {
        self.max_attempts.unwrap_or(DEFAULT_MAX_ATTEMPTS).max(1)
    }

    /// How the daily session is composed.
    pub fn filler(&self) -> Filler {
        self.filler.unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::{Config, DEFAULT_MAX_ATTEMPTS, DEFAULT_ROLLOVER_HOUR, Filler};

    #[test]
    fn an_absent_file_reads_as_defaults() {
        let config = Config::read(&Utf8PathBuf::from("/nowhere/at/all/rote.toml")).unwrap();
        assert_eq!(config.rollover_hour(), DEFAULT_ROLLOVER_HOUR);
        assert_eq!(config.max_attempts(), DEFAULT_MAX_ATTEMPTS);
        assert_eq!(config.filler(), Filler::BelowCap);
        assert!(config.root.is_none());
    }

    #[test]
    fn every_key_parses() {
        let config: Config = toml::from_str(
            r#"
            root = "/tmp/ark/rote"
            state = "/tmp/state/rote"
            rollover-hour = 6
            max-attempts = 2
            filler = "all"
            "#,
        )
        .unwrap();
        assert_eq!(config.root.as_deref(), Some("/tmp/ark/rote".into()));
        assert_eq!(config.rollover_hour(), 6);
        assert_eq!(config.max_attempts(), 2);
        assert_eq!(config.filler(), Filler::All);
    }

    #[test]
    fn an_unknown_key_is_refused_rather_than_ignored() {
        assert!(toml::from_str::<Config>("rollover = 6").is_err());
    }

    #[test]
    fn nonsense_values_are_clamped_rather_than_obeyed() {
        let config: Config = toml::from_str("rollover-hour = 40\nmax-attempts = 0").unwrap();
        assert_eq!(config.rollover_hour(), 23);
        assert_eq!(config.max_attempts(), 1);
    }
}
