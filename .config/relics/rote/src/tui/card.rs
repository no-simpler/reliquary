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

/// Where the content starts, past the border and the gutter.
const CONTENT_COL: usize = 4;

/// Where text starts inside the entry field: its own border, then a space.
const FIELD_TEXT_COL: usize = CONTENT_COL + 2;

/// The border and the blank line above the first line of body.
const BODY_OFFSET: usize = 2;

/// The text area inside the entry field, past its border and padding.
pub const FIELD: usize = CONTENT - 4;

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

impl Reveal {
    /// What stands in the field for a buffer this long.
    ///
    /// The one place a typed secret becomes something on a screen. The count is
    /// taken and not used, which is what blind means; it is carried so that
    /// revealing something is a change to this match and to nothing else.
    fn shown(self, typed: usize) -> String {
        match self {
            Self::Blind => {
                let _ = typed;
                String::new()
            }
        }
    }
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
    caret: Option<(usize, usize)>,
}

impl Card {
    /// An empty card under a title and a date.
    pub fn new(title: &'static str, stamp: impl Into<String>, color: bool) -> Self {
        Self {
            title,
            stamp: stamp.into(),
            color,
            body: Vec::new(),
            caret: None,
        }
    }

    /// Where the terminal's own cursor belongs, in rendered coordinates.
    ///
    /// The card draws no caret of its own. A real cursor blinks, follows the
    /// reader's own terminal settings, and cannot be overlapped by text the way
    /// a glyph on a shared line can.
    pub fn caret(&self) -> Option<(usize, usize)> {
        self.caret
    }

    /// Add the entry field: its own delimited area, and nothing else on the
    /// line.
    ///
    /// It draws no air of its own, so a caller composes one blank line before
    /// the label-and-field block and the label sits directly above the field it
    /// labels. Instructions never share the line: a hint beside somewhere a
    /// secret is typed is a hint that will one day be overlapped by what is
    /// typed into it.
    pub fn entry(&mut self, typed: usize, reveal: Reveal) -> &mut Self {
        let shown = reveal.shown(typed);
        let filled = shown.chars().count().min(FIELD);
        self.say(format!("╭{}╮", "─".repeat(FIELD + 2)), DIM);
        let interior = join(&[
            Piece::painted("│ ", DIM, self.color),
            Piece::painted(format!("{shown:<FIELD$}"), BOLD, self.color),
            Piece::painted(" │", DIM, self.color),
        ]);
        self.body.push(interior);
        self.caret = Some((
            self.body
                .len()
                .saturating_sub(1)
                .saturating_add(BODY_OFFSET),
            FIELD_TEXT_COL.saturating_add(filled),
        ));
        self.say(format!("╰{}╯", "─".repeat(FIELD + 2)), DIM);
        self
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
        let left = format!("╭─ {} ", self.title);
        let right = format!(" {} ─╮", self.stamp);
        let fill = WIDTH
            .saturating_sub(left.chars().count())
            .saturating_sub(right.chars().count());
        self.paint(&format!("{left}{}{right}", "─".repeat(fill)), DIM)
    }

    fn bottom(&self) -> String {
        self.paint(&format!("╰{}╯", "─".repeat(WIDTH.saturating_sub(2))), DIM)
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
    use super::{CONTENT, Card, Piece, Reveal, WIDTH};

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
    fn the_field_says_nothing_about_what_was_typed() {
        let mut empty = card();
        empty.entry(0, Reveal::Blind);
        for typed in [1_usize, 7, 64, 4096] {
            let mut drawn = card();
            drawn.entry(typed, Reveal::Blind);
            assert_eq!(
                drawn.render(),
                empty.render(),
                "{typed} characters changed the field"
            );
            assert_eq!(drawn.caret(), empty.caret(), "{typed} moved the cursor");
        }
    }

    #[test]
    fn the_cursor_lands_inside_the_field_and_nowhere_else() {
        let mut drawn = card();
        drawn.say("a label", super::DIM).entry(0, Reveal::Blind);
        let (row, column) = drawn.caret().expect("a field takes the cursor");
        let lines = drawn.render();
        let line: Vec<char> = lines[row].chars().collect();
        assert_eq!(line[column], ' ', "the cursor must sit on the text area");
        assert_eq!(
            line[column - 2],
            '│',
            "two columns left is the field border"
        );
    }

    #[test]
    fn a_card_with_no_field_takes_no_cursor() {
        let mut drawn = card();
        drawn.say("nothing to type here", super::DIM);
        assert_eq!(drawn.caret(), None);
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
