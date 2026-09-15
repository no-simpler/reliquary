//! Optional configuration. Absent means defaults, which is the ordinary case.

use camino::Utf8PathBuf;
use serde::Deserialize;

use crate::ladder::Ladder;

/// The hour a drill day begins, in local time. Drilling at one in the morning
/// belongs to the day before.
pub const DEFAULT_ROLLOVER_HOUR: i8 = 4;

/// Samples allowed per engram per sitting. Only the first is measured.
pub const DEFAULT_MAX_ATTEMPTS: u8 = 3;

/// The file, as read.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Where the corpus lives. Defaults to `~/Trove/ark/rote`.
    pub root: Option<Utf8PathBuf>,
    /// Where the verifiers and the reminder cache live. Defaults to
    /// `~/.local/state/rote`.
    pub state: Option<Utf8PathBuf>,
    /// The hour a drill day begins, 0 to 23.
    pub rollover_hour: Option<i8>,
    /// Samples per engram per sitting.
    pub max_attempts: Option<u8>,
    /// Days between reviews, by rung, the last held forever.
    pub ladder: Option<Vec<u32>>,
}

impl Config {
    /// Read a config file, treating an absent one as empty.
    ///
    /// # Errors
    ///
    /// When the file exists and will not parse, or declares a ladder that is not
    /// one. A malformed config is a loud refusal rather than a silent fall back
    /// to defaults: the defaults might point at a different tree than the one
    /// the record is in, and a different schedule than the one the record was
    /// built under.
    pub fn read(path: &camino::Utf8Path) -> anyhow::Result<Self> {
        let config: Self = match fs_err::read_to_string(path) {
            Ok(text) => toml::from_str(&text)?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(error) => return Err(error.into()),
        };
        config.ladder()?;
        Ok(config)
    }

    /// The rollover hour, clamped into a day.
    pub fn rollover_hour(&self) -> i8 {
        self.rollover_hour
            .unwrap_or(DEFAULT_ROLLOVER_HOUR)
            .clamp(0, 23)
    }

    /// Samples per engram per sitting, at least one.
    pub fn max_attempts(&self) -> u8 {
        self.max_attempts.unwrap_or(DEFAULT_MAX_ATTEMPTS).max(1)
    }

    /// The schedule.
    ///
    /// # Errors
    ///
    /// When the declared ladder is empty, shortens, or holds an interval this
    /// tool will not schedule. Refused rather than clamped: a schedule quietly
    /// different from the one that was asked for is worse than none.
    pub fn ladder(&self) -> anyhow::Result<Ladder> {
        match &self.ladder {
            Some(days) => Ok(Ladder::new(days.clone())?),
            None => Ok(Ladder::default()),
        }
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::{Config, DEFAULT_MAX_ATTEMPTS, DEFAULT_ROLLOVER_HOUR};
    use crate::ladder::{DEFAULT_LADDER, Ladder};

    #[test]
    fn an_absent_file_reads_as_defaults() {
        let config = Config::read(&Utf8PathBuf::from("/nowhere/at/all/rote.toml")).unwrap();
        assert_eq!(config.rollover_hour(), DEFAULT_ROLLOVER_HOUR);
        assert_eq!(config.max_attempts(), DEFAULT_MAX_ATTEMPTS);
        assert_eq!(config.ladder().unwrap(), Ladder::default());
        assert_eq!(Ladder::default().days(), DEFAULT_LADDER);
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
            ladder = [1, 3, 9]
            "#,
        )
        .unwrap();
        assert_eq!(config.root.as_deref(), Some("/tmp/ark/rote".into()));
        assert_eq!(config.rollover_hour(), 6);
        assert_eq!(config.max_attempts(), 2);
        assert_eq!(config.ladder().unwrap().days(), [1, 3, 9]);
    }

    #[test]
    fn an_unknown_key_is_refused_rather_than_ignored() {
        assert!(toml::from_str::<Config>("rollover = 6").is_err());
        assert!(
            toml::from_str::<Config>("policy = \"all\"").is_err(),
            "a key this binary does not read is a setting that silently does nothing"
        );
    }

    #[test]
    fn nonsense_values_are_clamped_rather_than_obeyed() {
        let config: Config = toml::from_str("rollover-hour = 40\nmax-attempts = 0").unwrap();
        assert_eq!(config.rollover_hour(), 23);
        assert_eq!(config.max_attempts(), 1);
    }

    #[test]
    fn a_ladder_that_is_not_one_is_refused_rather_than_clamped() {
        let config: Config = toml::from_str("ladder = [7, 2]").unwrap();
        assert!(config.ladder().is_err());
    }

    #[test]
    fn reading_a_file_refuses_a_bad_ladder_before_anything_uses_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("config.toml")).unwrap();
        std::fs::write(&path, "ladder = []\n").unwrap();
        assert!(Config::read(&path).is_err());
    }
}
