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

use relic_core::style::{Style, Tint};

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

/// What a field looks like, which is the only thing on a card that changes
/// colour to say something happened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tone {
    /// Nothing to report.
    Calm,
    /// Something was refused. Shown for a moment and then dropped: a mark that
    /// stays makes every later glance re-read it.
    Alarm,
}

impl Tone {
    fn tint(self) -> Tint {
        match self {
            Self::Calm => Tint::Dim,
            Self::Alarm => Tint::Red,
        }
    }
}

/// The border and the blank line above the first line of body.
const BODY_OFFSET: usize = 2;

/// The text area inside the entry field, past its border and padding.
pub const FIELD: usize = CONTENT - 4;

/// What a truncated line ends with. Reaching for this means the content and
/// [`CONTENT`] have drifted apart, which is a defect rather than a layout.
const ELLIPSIS: char = '…';

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

    /// Text under one tint, resolved now.
    pub fn painted(text: impl Into<String>, tint: Tint, style: Style) -> Self {
        let text = text.into();
        Self {
            styled: style.paint(tint, &text),
            plain: text,
        }
    }

    /// How many columns this occupies.
    pub fn width(&self) -> usize {
        self.plain.chars().count()
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
    style: Style,
    body: Vec<Piece>,
    caret: Option<(usize, usize)>,
    reserved: Option<usize>,
}

impl Card {
    /// An empty card under a title and a date.
    pub fn new(title: &'static str, stamp: impl Into<String>, style: Style) -> Self {
        Self {
            title,
            stamp: stamp.into(),
            style,
            body: Vec::new(),
            caret: None,
            reserved: None,
        }
    }

    /// Fix the body at this many lines, whatever a given state puts in it.
    ///
    /// Every state of one window is then the same rectangle in the same place,
    /// so a transition moves nothing and the eye keeps its bearings. Short
    /// states are padded; a state that overruns is cut, which is the signal
    /// that the layout or the wording wants shortening rather than the box.
    pub fn reserve(&mut self, lines: usize) -> &mut Self {
        self.reserved = Some(lines);
        self
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
    pub fn entry(&mut self, typed: usize, reveal: Reveal, tone: Tone) -> &mut Self {
        let shown = reveal.shown(typed);
        let filled = shown.chars().count().min(FIELD);
        let edge = tone.tint();
        self.say(format!("╭{}╮", "─".repeat(FIELD + 2)), edge);
        let interior = join(&[
            Piece::painted("│ ", edge, self.style),
            Piece::painted(format!("{shown:<FIELD$}"), Tint::Bold, self.style),
            Piece::painted(" │", edge, self.style),
        ]);
        self.body.push(interior);
        self.caret = Some((
            self.body
                .len()
                .saturating_sub(1)
                .saturating_add(BODY_OFFSET),
            FIELD_TEXT_COL.saturating_add(filled),
        ));
        self.say(format!("╰{}╯", "─".repeat(FIELD + 2)), edge);
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

    /// Add a line under one tint.
    pub fn say(&mut self, text: impl Into<String>, tint: Tint) -> &mut Self {
        let piece = Piece::painted(text, tint, self.style);
        self.body.push(piece);
        self
    }

    /// How many lines the card occupies, border and padding included.
    pub fn height(&self) -> usize {
        self.reserved.unwrap_or(self.body.len()).saturating_add(4)
    }

    /// Add a line with something at each end.
    ///
    /// Two reserved areas on one line, so what is said on the left can change
    /// without moving what sits on the right.
    pub fn split(&mut self, left: &str, right: &str, tint: Tint) -> &mut Self {
        let gap = CONTENT
            .saturating_sub(left.chars().count())
            .saturating_sub(right.chars().count());
        let piece = join(&[
            Piece::painted(left.to_owned(), tint, self.style),
            Piece::plain(" ".repeat(gap)),
            Piece::painted(right.to_owned(), Tint::Dim, self.style),
        ]);
        self.body.push(piece);
        self
    }

    /// Pad out to a given line, so what comes next lands in its own slot.
    pub fn pad_to(&mut self, line: usize) -> &mut Self {
        while self.body.len() < line {
            self.gap();
        }
        self
    }

    /// How many lines of body have been put in so far.
    pub fn lines(&self) -> usize {
        self.body.len()
    }

    /// Draw it.
    pub fn render(&self) -> Vec<String> {
        let lines = self.reserved.unwrap_or(self.body.len());
        let blank = Piece::plain(String::new());
        let mut out = Vec::with_capacity(self.height());
        out.push(self.top());
        out.push(self.edge(&blank));
        for index in 0..lines {
            out.push(self.edge(self.body.get(index).unwrap_or(&blank)));
        }
        out.push(self.edge(&blank));
        out.push(self.bottom());
        out
    }

    fn top(&self) -> String {
        let left = format!("╭─ {} ", self.title);
        let right = format!(" {} ─╮", self.stamp);
        let fill = WIDTH
            .saturating_sub(left.chars().count())
            .saturating_sub(right.chars().count());
        self.style
            .dim(&format!("{left}{}{right}", "─".repeat(fill)))
    }

    fn bottom(&self) -> String {
        self.style
            .dim(&format!("╰{}╯", "─".repeat(WIDTH.saturating_sub(2))))
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
            self.style.dim("│"),
            " ".repeat(pad),
            self.style.dim("│")
        )
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
    use relic_core::style::{Style, Tint};

    use super::{CONTENT, Card, Piece, Reveal, Tone, WIDTH};

    fn card() -> Card {
        Card::new("rote", "Thu 10 Sep", Style::PLAIN)
    }

    #[test]
    fn every_card_is_the_same_width_whatever_is_in_it() {
        let mut small = card();
        small.say("x", Tint::Dim);
        let mut large = card();
        large.say("x".repeat(CONTENT), Tint::Dim);
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
        drawn.say("one", Tint::Dim).gap().say("two", Tint::Bold);
        for line in drawn.render() {
            assert!(!line.contains('\n'), "{line:?}");
            assert!(!line.contains('\r'), "{line:?}");
        }
    }

    #[test]
    fn a_line_that_outgrew_the_card_is_cut_rather_than_breaking_the_box() {
        let mut drawn = card();
        drawn.say("x".repeat(CONTENT + 20), Tint::Dim);
        let lines = drawn.render();
        for line in &lines {
            assert_eq!(line.chars().count(), WIDTH, "{line}");
        }
        assert!(lines.iter().any(|line| line.contains('…')));
    }

    #[test]
    fn the_field_says_nothing_about_what_was_typed() {
        let mut empty = card();
        empty.entry(0, Reveal::Blind, Tone::Calm);
        for typed in [1_usize, 7, 64, 4096] {
            let mut drawn = card();
            drawn.entry(typed, Reveal::Blind, Tone::Calm);
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
        drawn
            .say("a label", Tint::Dim)
            .entry(0, Reveal::Blind, Tone::Calm);
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
        drawn.say("nothing to type here", Tint::Dim);
        assert_eq!(drawn.caret(), None);
    }

    #[test]
    fn colour_changes_the_bytes_and_not_the_shape() {
        let plain = Piece::painted("hello", Tint::Red, Style::PLAIN);
        let painted = Piece::painted("hello", Tint::Red, Style::COLOUR);
        assert_eq!(plain.width(), painted.width());
        assert!(painted.styled.len() > plain.styled.len());
    }

    #[test]
    fn a_reserved_card_is_the_same_height_however_much_is_put_in_it() {
        let mut empty = card();
        empty.reserve(8);
        let mut full = card();
        full.reserve(8);
        for n in 0..6 {
            full.say(format!("line {n}"), Tint::Dim);
        }
        assert_eq!(empty.render().len(), full.render().len());
        assert_eq!(empty.render().len(), 8 + 4);
    }

    #[test]
    fn a_state_that_overruns_its_reservation_is_cut_rather_than_stretching_the_box() {
        let mut drawn = card();
        drawn.reserve(3);
        for n in 0..9 {
            drawn.say(format!("line {n}"), Tint::Dim);
        }
        let lines = drawn.render();
        assert_eq!(lines.len(), 3 + 4);
        assert!(lines.iter().any(|l| l.contains("line 2")));
        assert!(!lines.iter().any(|l| l.contains("line 3")));
    }

    #[test]
    fn an_alarmed_field_differs_only_in_colour() {
        let mut calm = card();
        calm.entry(0, Reveal::Blind, Tone::Calm);
        let mut alarm = Card::new("rote", "Thu 10 Sep", Style::COLOUR);
        alarm.entry(0, Reveal::Blind, Tone::Alarm);
        let mut painted_calm = Card::new("rote", "Thu 10 Sep", Style::COLOUR);
        painted_calm.entry(0, Reveal::Blind, Tone::Calm);
        assert_eq!(calm.caret(), alarm.caret());
        assert_eq!(
            alarm.render().len(),
            painted_calm.render().len(),
            "an alarm must not change the shape"
        );
        assert_ne!(alarm.render(), painted_calm.render(), "and must be visible");
    }

    #[test]
    fn height_counts_the_border_and_the_padding() {
        let mut drawn = card();
        drawn.say("one", Tint::Dim).say("two", Tint::Dim);
        assert_eq!(drawn.height(), drawn.render().len());
    }
}
