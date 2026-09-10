//! Colour, decided once at the edge and spent the same way by every relic.
//!
//! Whether to emit escape sequences is [`crate::ui::ColorChoice`]'s decision,
//! made from the flag, the environment and the output shape. What is here is
//! only the spending: a [`Style`] carries the decision, a [`Tint`] names what a
//! piece of text is painted with, and the plain form survives either way.

/// Whether output spends on escape sequences at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Style {
    /// True when a sequence is written; false leaves every text as it was.
    pub colour: bool,
}

/// What a piece of text is painted with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tint {
    /// Weight, for a heading or a name.
    Bold,
    /// Secondary: a border, a note, a hint.
    Dim,
    /// Broken, refused, alarmed.
    Red,
    /// Clean, accepted.
    Green,
    /// Soft: worth a look, not a failure.
    Yellow,
    /// Red with weight, for the one line that must not be missed.
    BoldRed,
}

impl Tint {
    /// The SGR sequence that opens this tint.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Bold => "\x1b[1m",
            Self::Dim => "\x1b[2m",
            Self::Red => "\x1b[31m",
            Self::Green => "\x1b[32m",
            Self::Yellow => "\x1b[33m",
            Self::BoldRed => "\x1b[1;31m",
        }
    }
}

/// The sequence that closes any tint.
pub const RESET: &str = "\x1b[0m";

impl Style {
    /// No sequences at all. What a test and a piped stream get.
    pub const PLAIN: Self = Self { colour: false };

    /// Sequences on.
    pub const COLOUR: Self = Self { colour: true };

    /// The text under one tint, or the text itself when colour is off.
    ///
    /// ```
    /// use relic_core::style::{Style, Tint};
    ///
    /// assert_eq!(Style::PLAIN.paint(Tint::Red, "x"), "x");
    /// assert_eq!(Style::COLOUR.paint(Tint::Red, "x"), "\x1b[31mx\x1b[0m");
    /// ```
    #[must_use]
    pub fn paint(self, tint: Tint, text: &str) -> String {
        if self.colour {
            format!("{}{text}{RESET}", tint.code())
        } else {
            text.to_owned()
        }
    }

    /// Bold.
    #[must_use]
    pub fn bold(self, text: &str) -> String {
        self.paint(Tint::Bold, text)
    }

    /// Dim.
    #[must_use]
    pub fn dim(self, text: &str) -> String {
        self.paint(Tint::Dim, text)
    }

    /// Red.
    #[must_use]
    pub fn red(self, text: &str) -> String {
        self.paint(Tint::Red, text)
    }

    /// Green.
    #[must_use]
    pub fn green(self, text: &str) -> String {
        self.paint(Tint::Green, text)
    }

    /// Yellow.
    #[must_use]
    pub fn yellow(self, text: &str) -> String {
        self.paint(Tint::Yellow, text)
    }
}

#[cfg(test)]
mod tests {
    use super::{RESET, Style, Tint};

    #[test]
    fn colour_is_a_choice_and_the_text_survives_it() {
        assert_eq!(Style::PLAIN.bold("x"), "x");
        let painted = Style::COLOUR.bold("x");
        assert!(painted.contains('x'));
        assert!(painted.starts_with(Tint::Bold.code()));
        assert!(painted.ends_with(RESET));
    }

    #[test]
    fn every_tint_opens_with_an_escape_and_closes_with_the_reset() {
        for tint in [
            Tint::Bold,
            Tint::Dim,
            Tint::Red,
            Tint::Green,
            Tint::Yellow,
            Tint::BoldRed,
        ] {
            let painted = Style::COLOUR.paint(tint, "t");
            assert!(painted.starts_with("\x1b["), "{tint:?}");
            assert!(painted.ends_with(RESET), "{tint:?}");
            assert_eq!(Style::PLAIN.paint(tint, "t"), "t", "{tint:?}");
        }
    }
}
