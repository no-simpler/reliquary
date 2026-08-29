//! How the tables read.
//!
//! Colour is resolved once, by `relic_core::ui`, which strips ANSI when the
//! output is piped — the ladder every relic used to hand-write.

use std::io::Write;

use anstream::adapter::strip_str;

/// The escape sequences the tables use.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// Whether to emit any at all.
    pub colour: bool,
}

impl Style {
    /// Bold.
    #[must_use]
    pub fn bold(self, text: &str) -> String {
        self.wrap("\u{1b}[1m", text)
    }

    /// Dim.
    #[must_use]
    pub fn dim(self, text: &str) -> String {
        self.wrap("\u{1b}[2m", text)
    }

    /// Green.
    #[must_use]
    pub fn green(self, text: &str) -> String {
        self.wrap("\u{1b}[32m", text)
    }

    /// Yellow.
    #[must_use]
    pub fn yellow(self, text: &str) -> String {
        self.wrap("\u{1b}[33m", text)
    }

    /// Red.
    #[must_use]
    pub fn red(self, text: &str) -> String {
        self.wrap("\u{1b}[31m", text)
    }

    fn wrap(self, code: &str, text: &str) -> String {
        if self.colour {
            format!("{code}{text}\u{1b}[0m")
        } else {
            text.to_owned()
        }
    }
}

/// Print a heading with its dim subtitle.
///
/// # Errors
///
/// When the stream refuses.
pub fn heading(out: &mut impl Write, style: Style, title: &str, note: &str) -> std::io::Result<()> {
    writeln!(out, "{} {}", style.bold(title), style.dim(note))
}

/// A left-padded column whose width counts **visible** characters.
///
/// Padding a string that carries escape sequences by its byte length puts the
/// columns wherever the colours happen to fall — which is the whole reason this
/// is one function rather than a format specifier at each site.
#[must_use]
pub fn pad(text: &str, width: usize) -> String {
    let visible = strip_str(text).to_string().chars().count();
    let mut out = text.to_owned();
    for _ in visible..width {
        out.push(' ');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{Style, pad};

    #[test]
    fn colour_is_a_choice_and_the_text_survives_it() {
        let plain = Style { colour: false };
        assert_eq!(plain.bold("x"), "x");
        let colour = Style { colour: true };
        assert!(colour.bold("x").contains('x'));
        assert!(colour.bold("x").len() > 1);
    }

    #[test]
    fn padding_counts_what_is_visible_rather_than_what_is_stored() {
        let colour = Style { colour: true };
        let painted = colour.green("ok");
        assert!(painted.len() > 2, "the fixture is not actually painted");
        assert_eq!(
            anstream::adapter::strip_str(&pad(&painted, 6)).to_string(),
            "ok    "
        );
    }

    #[test]
    fn a_column_narrower_than_its_content_is_not_truncated() {
        assert_eq!(pad("abcdef", 3), "abcdef");
    }
}
