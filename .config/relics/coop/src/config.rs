//! Rendering preferences, and nothing else.
//!
//! Cadence is not here. The card is edge-triggered because a repeated one stops
//! being read and because fish's prompt event makes no promise about repaints —
//! neither of which is a preference.

use anyhow::{Context, Result};
use camino::Utf8Path;
use serde::Deserialize;

/// How the box is drawn.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Deserialize, clap::ValueEnum)]
#[serde(rename_all = "kebab-case")]
pub enum Style {
    /// Box-drawing characters.
    #[default]
    Unicode,
    /// The same card in ASCII, for a terminal or a pipe that cannot hold the
    /// other one.
    Ascii,
}

/// The widest the card is allowed to be, whatever the terminal says.
pub const MAX_WIDTH: usize = 80;
/// The narrowest it is worth drawing.
pub const MIN_WIDTH: usize = 28;

/// What `~/.config/coop/config.toml` may say.
#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// How the box is drawn.
    #[serde(default)]
    pub style: Style,
    /// A fixed width, overriding the terminal. Zero means "ask the terminal".
    #[serde(default)]
    pub width: usize,
}

impl Config {
    /// Read it, or take the defaults.
    ///
    /// An absent file is the default configuration. A malformed one is an
    /// error: silently falling back would make a typo invisible forever.
    ///
    /// # Errors
    ///
    /// When the file exists and does not parse.
    pub fn read(path: &Utf8Path) -> Result<Self> {
        let Ok(text) = fs_err::read_to_string(path) else {
            return Ok(Self::default());
        };
        toml::from_str(&text).with_context(|| format!("parsing {path}"))
    }

    /// The width to draw at, given what the terminal admits to.
    pub fn width(self, terminal: Option<usize>) -> usize {
        let asked = if self.width > 0 {
            self.width
        } else {
            terminal.unwrap_or(MAX_WIDTH)
        };
        asked.clamp(MIN_WIDTH, MAX_WIDTH)
    }
}

/// The terminal's width, when there is a terminal.
pub fn terminal_width() -> Option<usize> {
    crossterm::terminal::size()
        .ok()
        .map(|(columns, _)| usize::from(columns))
}

#[cfg(test)]
mod tests {
    use super::{Config, MAX_WIDTH, MIN_WIDTH, Style};

    #[test]
    fn a_configured_width_outranks_the_terminal() {
        let config = Config {
            style: Style::Unicode,
            width: 50,
        };
        assert_eq!(config.width(Some(200)), 50);
    }

    #[test]
    fn a_wide_terminal_is_clamped_so_the_card_stays_readable() {
        assert_eq!(Config::default().width(Some(400)), MAX_WIDTH);
    }

    #[test]
    fn a_narrow_terminal_is_clamped_so_the_card_stays_drawable() {
        assert_eq!(Config::default().width(Some(4)), MIN_WIDTH);
    }

    #[test]
    fn no_terminal_means_the_default_width() {
        assert_eq!(Config::default().width(None), MAX_WIDTH);
    }
}
