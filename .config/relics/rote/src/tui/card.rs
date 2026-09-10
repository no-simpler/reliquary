//! The card: one modal dialog, drawn the same way wherever it appears.
//!
//! Building a card is a pure function from content to lines, so the whole
//! visual design is under snapshot test and the terminal adapter only has to
//! place what it is handed. Colour is resolved here, at build time, because a
//! piece is the smallest thing that knows whether it is an outcome or an aside.
//!
//! The width is fixed for every card in the binary. A box that resizes as
//! content comes and goes reads as instability, and the eye tracks a border it
//! has to re-find. Height is the axis that varies, and the adapter anchors the
//! top so that variation grows downward.

/// Every card is this wide, and no card is any other width.
///
/// Chosen to hold the widest row a sitting can produce — the longest slug, the
/// fullest ladder chip and a short outcome — and to leave a margin inside an
/// eighty-column terminal, which is the floor worth designing for.
pub const WIDTH: usize = 76;

/// The border and the gutters either side of the content.
const CHROME: usize = 8;

/// How much of [`WIDTH`] a line of content may occupy.
pub const CONTENT: usize = WIDTH - CHROME;

/// What is drawn where a secret is being typed.
const CURSOR: &str = "▉";

/// The marker that opens an entry line.
const CARET: &str = "▸ ";

/// What a truncated line ends with. Reaching for this means the content and
/// [`CONTENT`] have drifted apart, which is a defect rather than a layout.
const ELLIPSIS: char = '…';

pub const RESET: &str = "\x1b[0m";
pub const BOLD: &str = "\x1b[1m";
pub const DIM: &str = "\x1b[2m";
pub const RED: &str = "\x1b[31m";
pub const GREEN: &str = "\x1b[32m";
pub const YELLOW: &str = "\x1b[33m";

/// A piece of text, and the same text with colour on it.
#[derive(Clone, Debug)]
pub struct Piece {
    plain: String,
    styled: String,
}

impl Piece {
    /// Text that carries no colour of its own.
    pub fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            styled: text.clone(),
            plain: text,
        }
    }

    /// Text under one style, resolved now.
    pub fn painted(text: impl Into<String>, code: &str, color: bool) -> Self {
        let text = text.into();
        let styled = if color {
            format!("{code}{text}{RESET}")
        } else {
            text.clone()
        };
        Self {
            plain: text,
            styled,
        }
    }

    /// How many columns this occupies.
    pub fn width(&self) -> usize {
        self.plain.chars().count()
    }

    /// Whether there is nothing to draw.
    pub fn is_blank(&self) -> bool {
        self.plain.trim().is_empty()
    }
}

/// Several pieces run together as one line.
pub fn join(pieces: &[Piece]) -> Piece {
    Piece {
        plain: pieces.iter().map(|p| p.plain.as_str()).collect(),
        styled: pieces.iter().map(|p| p.styled.as_str()).collect(),
    }
}

/// How much of a secret being typed is drawn.
///
/// Blind is the safe starting point rather than the final answer. Per-character
/// masking is defensible — the login screen on this machine exposes a count —
/// and would be a variant here and a change nowhere else. Word-boundary masking
/// never is: seven word lengths is most of a diceware phrase's search space.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reveal {
    /// Nothing about the buffer reaches the screen.
    Blind,
}

/// The one place a secret being typed is drawn.
///
/// Every prompt in the binary comes through here, so what an entry looks like
/// is one function rather than a habit repeated at each call site.
pub fn entry_line(typed: usize, reveal: Reveal, hint: &str, color: bool) -> Piece {
    let shown = match reveal {
        // The count is taken and not used, which is what blind means. It is
        // carried so that revealing something is a change to this match and to
        // nothing else.
        Reveal::Blind => {
            let _ = typed;
            String::new()
        }
    };
    join(&[
        Piece::painted(CARET, DIM, color),
        Piece::painted(shown, BOLD, color),
        Piece::painted(CURSOR, BOLD, color),
        Piece::painted(hint.to_owned(), DIM, color),
    ])
}

/// The date, as every card stamps it.
pub fn stamp(today: jiff::civil::Date) -> String {
    today.strftime("%a %-d %b").to_string()
}

/// A modal dialog, as a value.
#[derive(Clone, Debug)]
pub struct Card {
    title: &'static str,
    stamp: String,
    color: bool,
    body: Vec<Piece>,
}

impl Card {
    /// An empty card under a title and a date.
    pub fn new(title: &'static str, stamp: impl Into<String>, color: bool) -> Self {
        Self {
            title,
            stamp: stamp.into(),
            color,
            body: Vec::new(),
        }
    }

    /// Add a line.
    pub fn line(&mut self, piece: Piece) -> &mut Self {
        self.body.push(piece);
        self
    }

    /// Add an empty line.
    pub fn gap(&mut self) -> &mut Self {
        self.body.push(Piece::plain(String::new()));
        self
    }

    /// Add a line under one style.
    pub fn say(&mut self, text: impl Into<String>, code: &str) -> &mut Self {
        let piece = Piece::painted(text, code, self.color);
        self.body.push(piece);
        self
    }

    /// How many lines the card occupies, border and padding included.
    pub fn height(&self) -> usize {
        self.body.len().saturating_add(4)
    }

    /// Draw it.
    pub fn render(&self) -> Vec<String> {
        let mut lines = Vec::with_capacity(self.height());
        lines.push(self.top());
        lines.push(self.edge(&Piece::plain(String::new())));
        for piece in &self.body {
            lines.push(self.edge(piece));
        }
        lines.push(self.edge(&Piece::plain(String::new())));
        lines.push(self.bottom());
        lines
    }

    fn top(&self) -> String {
        let left = format!("┌─ {} ", self.title);
        let right = format!(" {} ─┐", self.stamp);
        let fill = WIDTH
            .saturating_sub(left.chars().count())
            .saturating_sub(right.chars().count());
        self.paint(&format!("{left}{}{right}", "─".repeat(fill)), DIM)
    }

    fn bottom(&self) -> String {
        self.paint(&format!("└{}┘", "─".repeat(WIDTH.saturating_sub(2))), DIM)
    }

    /// One content line between two borders.
    ///
    /// The truncation is a safety net rather than a layout: a line that reaches
    /// it has outgrown [`CONTENT`] and loses its colour along with its tail,
    /// which is what makes the defect visible instead of silently breaking the
    /// box the way an over-long line otherwise would.
    fn edge(&self, piece: &Piece) -> String {
        let (content, visible) = if piece.width() > CONTENT {
            (clip(&piece.plain), CONTENT)
        } else {
            (piece.styled.clone(), piece.width())
        };
        let pad = CONTENT.saturating_sub(visible);
        format!(
            "{}   {content}{}   {}",
            self.paint("│", DIM),
            " ".repeat(pad),
            self.paint("│", DIM)
        )
    }

    fn paint(&self, text: &str, code: &str) -> String {
        if self.color {
            format!("{code}{text}{RESET}")
        } else {
            text.to_owned()
        }
    }
}

/// Cut a line down to [`CONTENT`] columns, ellipsis included.
fn clip(text: &str) -> String {
    let mut out: String = text.chars().take(CONTENT.saturating_sub(1)).collect();
    out.push(ELLIPSIS);
    out
}

#[cfg(test)]
mod tests {
    use super::{CONTENT, Card, Piece, Reveal, WIDTH, entry_line};

    fn card() -> Card {
        Card::new("rote", "Thu 10 Sep", false)
    }

    #[test]
    fn every_card_is_the_same_width_whatever_is_in_it() {
        let mut small = card();
        small.say("x", super::DIM);
        let mut large = card();
        large.say("x".repeat(CONTENT), super::DIM);
        for lines in [small.render(), large.render()] {
            for line in &lines {
                assert_eq!(line.chars().count(), WIDTH, "{line}");
            }
        }
    }

    #[test]
    fn no_line_of_a_card_carries_a_newline() {
        // Raw mode takes the line discipline away and every line is placed with
        // its own `MoveTo`, so a newline reaching the terminal inside a line
        // would put the rest of the card wherever the cursor happened to be.
        let mut drawn = card();
        drawn.say("one", super::DIM).gap().say("two", super::BOLD);
        for line in drawn.render() {
            assert!(!line.contains('\n'), "{line:?}");
            assert!(!line.contains('\r'), "{line:?}");
        }
    }

    #[test]
    fn a_line_that_outgrew_the_card_is_cut_rather_than_breaking_the_box() {
        let mut drawn = card();
        drawn.say("x".repeat(CONTENT + 20), super::DIM);
        let lines = drawn.render();
        for line in &lines {
            assert_eq!(line.chars().count(), WIDTH, "{line}");
        }
        assert!(lines.iter().any(|line| line.contains('…')));
    }

    #[test]
    fn the_entry_line_says_nothing_about_what_was_typed() {
        let empty = entry_line(0, Reveal::Blind, "", false);
        for typed in [1_usize, 7, 64, 4096] {
            assert_eq!(
                entry_line(typed, Reveal::Blind, "", false).width(),
                empty.width(),
                "{typed} characters changed the line"
            );
        }
    }

    #[test]
    fn colour_changes_the_bytes_and_not_the_shape() {
        let plain = Piece::painted("hello", super::RED, false);
        let painted = Piece::painted("hello", super::RED, true);
        assert_eq!(plain.width(), painted.width());
        assert!(painted.styled.len() > plain.styled.len());
    }

    #[test]
    fn height_counts_the_border_and_the_padding() {
        let mut drawn = card();
        drawn.say("one", super::DIM).say("two", super::DIM);
        assert_eq!(drawn.height(), drawn.render().len());
    }
}
