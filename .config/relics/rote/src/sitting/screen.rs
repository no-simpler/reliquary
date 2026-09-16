//! What a sitting puts on a card.
//!
//! Building one is a pure function from a frame to a card, so the whole visual
//! design is under test and the terminal adapter only has to place what it is
//! handed. The box, the palette and the field are the shared dialog's and live
//! in `tui::card`; what is here is the sitting's own layout.
//!
//! Every state of a sitting is the same rectangle. The list, the intention and
//! the field sit at fixed lines, so what changes between states changes inside
//! a reserved column or on the line under the field, and the field itself
//! never moves.

use jiff::civil::Date;
use relic_core::style::{Style, Tint};

use super::{Outturn, Task, Turn};
use crate::corpus::drill::Landing;
use crate::ladder::Occasion;
use crate::slug::Slug;
use crate::tui::card::{CONTENT, Card, Field, Piece, Tone, fit_to, join};

/// What points at the engram being asked about.
const MARKER: &str = "▸ ";

/// The chip's column, wide enough for the fullest one.
///
/// A ceiling rather than a constant. A slug at [`crate::slug::MAX`] and a full
/// chip together outgrow the box, and the chip is the only column here made of
/// droppable facts — so it is the one that gives. [`chip_width`] spends what is
/// left after the columns that cannot.
const CHIP: usize = 28;

/// The column holding one glyph of standing.
const ICON: usize = 3;

/// Lines a card holds beyond one per turn: air, air, the intention, the three
/// of the field, air, and the line under it.
pub const SLOTS: usize = 8;

/// Where one turn has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    /// Not reached yet.
    Pending,
    /// At the prompt.
    Active {
        /// Which try.
        attempt: u8,
    },
    /// Submitted, and the verifier is working.
    Checking,
    /// Accepted.
    Passed {
        /// Milliseconds from first keystroke to submission.
        total_ms: Option<u64>,
        /// Tries before this one.
        retries: u8,
    },
    /// Refused.
    Failed {
        /// Which try.
        attempt: u8,
    },
    /// Passed over.
    Skipped,
    /// The sitting was abandoned here.
    Aborted,
    /// No verifier here, so nothing was asked.
    Dormant,
}

/// What a row is for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The secret is asked for and the verifier judges it.
    Drill {
        /// Whether the schedule asked.
        occasion: Occasion,
        /// Whether the answer has been consulted.
        aided: bool,
        /// What the ladder asked for.
        interval_days: u32,
        /// Whether the rung is the top of the ladder.
        at_cap: bool,
    },
    /// This machine holds no verifier, so there is nothing to ask.
    Dormant,
}

/// One line of the card.
#[derive(Clone, Debug)]
pub struct Row {
    /// The lineage. Bare, without the engram's ordinal: during a sitting there
    /// is only ever the current engram, and the ordinal would be noise in the
    /// one place a person looks every day.
    pub slug: Slug,
    /// What this row is for.
    pub kind: Kind,
    /// Where it has got to.
    pub state: RowState,
}

impl Row {
    /// A row for a turn nobody has reached yet.
    pub fn pending(turn: &Turn) -> Self {
        Self {
            slug: turn.slug.clone(),
            kind: match &turn.task {
                Task::Drill(drilling) => Kind::Drill {
                    occasion: drilling.occasion,
                    aided: drilling.aided,
                    interval_days: drilling.scheduled_interval_days,
                    at_cap: drilling.at_cap,
                },
                Task::Dormant => Kind::Dormant,
            },
            state: RowState::Pending,
        }
    }

    /// Note that the answer has been consulted.
    pub fn set_aided(&mut self, value: bool) {
        if let Kind::Drill { aided, .. } = &mut self.kind {
            *aided = value;
        }
    }
}

/// What to draw.
#[derive(Clone, Debug)]
pub struct Frame<'a> {
    /// The drill day.
    pub today: Date,
    /// Every turn in the sitting.
    pub rows: &'a [Row],
    /// Which row is at the prompt.
    pub active: Option<usize>,
    /// What the field looks like.
    pub tone: Tone,
    /// What the line under the field says.
    pub status: Option<String>,
    /// Whether the lookup is on offer, which it is once a miss is on record.
    pub lookup: bool,
    /// The sitting, once it is over.
    pub outturn: Option<&'a Outturn>,
    /// Closing lines, once it is over.
    pub notes: &'a [Note],
}

/// One closing line: what it is about, and what it says.
///
/// Kept apart rather than joined by the caller, because the card wraps the
/// saying under the label and only the card knows how wide it may be.
#[derive(Clone, Debug)]
pub struct Note {
    /// The engram this is about.
    pub label: String,
    /// What is being said about it.
    pub said: String,
}

impl<'a> Frame<'a> {
    /// A frame mid-sitting.
    pub fn running(today: Date, rows: &'a [Row], active: Option<usize>) -> Self {
        Self {
            today,
            rows,
            active,
            tone: Tone::Calm,
            status: None,
            lookup: false,
            outturn: None,
            notes: &[],
        }
    }

    /// The closing frame.
    pub fn done(today: Date, rows: &'a [Row], outturn: &'a Outturn, notes: &'a [Note]) -> Self {
        Self {
            today,
            rows,
            active: None,
            tone: Tone::Calm,
            status: None,
            lookup: false,
            outturn: Some(outturn),
            notes,
        }
    }
}

/// What a row's prompt rests as when nothing has just been refused.
///
/// Every prompt the sitting draws verifies what was typed, so every one rests
/// calm. The one prompt that checks nothing is the attachment's, and it is not
/// drawn here.
#[must_use]
pub fn resting(row: Option<&Row>) -> Tone {
    match row.map(|row| &row.kind) {
        Some(Kind::Drill { .. } | Kind::Dormant) | None => Tone::Calm,
    }
}

/// What the card is titled, which says which instrument this is.
fn title(frame: &Frame<'_>) -> &'static str {
    if frame.outturn.is_some() {
        "done"
    } else {
        "rote"
    }
}

/// Render one frame.
pub fn card(frame: &Frame<'_>, field: &Field<'_>, style: Style) -> Card {
    let mut card = Card::new(title(frame), crate::tui::card::stamp(frame.today), style);
    card.reserve(frame.rows.len().saturating_add(SLOTS));
    let names = frame
        .rows
        .iter()
        .map(|row| row.slug.as_str().chars().count())
        .max()
        .unwrap_or(0);
    // One width for every row, taken from the widest of each column, so the
    // glyphs stay in one place however the chips are trimmed.
    let chip = chip_width(frame, names, style);

    if frame.rows.is_empty() {
        card.say("nothing to drill", Tint::Dim);
    }
    for (index, row) in frame.rows.iter().enumerate() {
        card.line(row_line(
            row,
            names,
            chip,
            frame.active == Some(index),
            style,
        ));
    }

    if let Some(row) = frame.active.and_then(|index| frame.rows.get(index)) {
        card.gap().gap();
        card.say(intention(row), intention_tint(row));
        if row.state == RowState::Checking {
            // Submitted, and the verifier is running. The box goes now rather
            // than after: what is on the screen has to say whether typing does
            // anything, and here it does not.
            card.waiting(crate::tui::WORKING);
        } else {
            // What the field shows is the entry loop's to decide and this
            // card's to draw: blind where a memory is being measured, and one
            // glyph per character everywhere else.
            card.entry(field, frame.tone);
        }
        card.gap();
        let (left, right) = under(frame, field);
        card.split(&left, &right, Tint::Dim);
    }

    if let Some(outturn) = frame.outturn {
        // The tally names its own subject. It buckets each turn by its *first*
        // capture, so a drill recovered with the vault still reads as the miss
        // it opened with — true, and unreadable beside a row that says the
        // sitting got there in the end, unless the two say what they are about.
        card.gap().prose(
            RECORDED,
            &tally(outturn, frame.rows.len()),
            Tint::Dim,
            Tint::Bold,
        );
        let labels = frame
            .notes
            .iter()
            .map(|note| note.label.chars().count())
            .max()
            .unwrap_or(0);
        for note in frame.notes {
            card.prose(
                &format!("{:<width$}", note.label, width = labels.saturating_add(2)),
                &note.said,
                Tint::Dim,
                Tint::Dim,
            );
        }
        let last = frame.rows.len().saturating_add(SLOTS).saturating_sub(1);
        while card.lines() < last {
            card.gap();
        }
        card.say("press any key", Tint::Dim);
    }
    card
}

/// The line under the field: how this prompt is going on the left, and on the
/// right the keys a person could not guess at. `Card::split` draws it, and
/// drops the chips where the status crowds them.
fn under(frame: &Frame<'_>, field: &Field<'_>) -> (String, String) {
    let left = frame.status.clone().unwrap_or_default();
    let lookup = if frame.lookup { "^L  look it up" } else { "" };
    let reveal = crate::tui::reveal_chip(field.reveal);
    let right = match (lookup.is_empty(), reveal.is_empty()) {
        (false, false) => format!("{lookup}   {reveal}"),
        (false, true) => lookup.to_owned(),
        (true, false) => reveal.to_owned(),
        (true, true) => String::new(),
    };
    (left, right)
}

fn row_line(row: &Row, names: usize, chips: usize, active: bool, style: Style) -> Piece {
    let marker = if active { MARKER } else { "  " };
    let (glyph, tint) = icon(row);
    join(&[
        Piece::painted(marker, Tint::Dim, style),
        Piece::painted(
            format!(
                "{:<width$}",
                row.slug.as_str(),
                width = names.saturating_add(2)
            ),
            Tint::Bold,
            style,
        ),
        Piece::painted(
            format!("{:<chips$}", fit_to(&chip(row), chips)),
            Tint::Dim,
            style,
        ),
        Piece::painted(format!("{glyph:<ICON$}"), tint, style),
        detail(row, style),
    ])
}

/// What the chip column may spend, once the columns that cannot give have.
///
/// The marker, the slug, the glyph and the detail are each as wide as their
/// widest row and none of them is droppable — a trimmed slug names the wrong
/// lineage and a trimmed latency is a wrong number. The chip is a list of
/// separated facts, so it is the column that shortens.
fn chip_width(frame: &Frame<'_>, names: usize, style: Style) -> usize {
    let details = frame
        .rows
        .iter()
        .map(|row| detail(row, style).width())
        .max()
        .unwrap_or(0);
    let room = CONTENT
        .saturating_sub(MARKER.chars().count())
        .saturating_sub(names.saturating_add(2))
        .saturating_sub(ICON)
        .saturating_sub(details);
    CHIP.min(room)
}

/// The one character that says where a row stands.
///
/// One rule over every state, so no screen decides this for itself: **green is
/// only ever a clean pass — unaided, first try. Yellow is everything that got
/// there another way. Red is a failure. Dim is nothing measured.**
///
/// A green tick on a drill that took three goes, or on one answered with the
/// vault open, claims a verdict nobody rendered. The tally below says what
/// went on the record; the glyph says how the sitting went, and the two agree
/// about which of them is which.
fn icon(row: &Row) -> (&'static str, Tint) {
    match row.state {
        // The marker already says which row is being asked about, and a row
        // nobody has reached says nothing at all.
        RowState::Pending | RowState::Active { .. } => (" ", Tint::Dim),
        RowState::Checking => ("·", Tint::Dim),
        RowState::Passed { retries, .. } if retries == 0 && !aided(row) => ("✓", Tint::Green),
        RowState::Passed { .. } => ("✓", Tint::Yellow),
        RowState::Failed { .. } => ("✗", Tint::Red),
        RowState::Skipped | RowState::Aborted | RowState::Dormant => ("–", Tint::Dim),
    }
}

/// Whether the vault was open for this row.
fn aided(row: &Row) -> bool {
    match row.kind {
        Kind::Drill { aided, .. } => aided,
        Kind::Dormant => false,
    }
}

/// What the glyph cannot say on its own.
fn detail(row: &Row, style: Style) -> Piece {
    match row.state {
        RowState::Passed { total_ms, retries } => {
            // A latency that was not measured is left out rather than dashed:
            // a dash is a table's empty cell, and on a card beside a try count
            // it reads as one more fact.
            let mut parts = Vec::new();
            if total_ms.is_some() {
                parts.push(crate::render::seconds(total_ms));
            }
            if retries > 0 {
                parts.push(format!("x{}", retries.saturating_add(1)));
            }
            Piece::painted(parts.join("   "), Tint::Dim, style)
        }
        RowState::Failed { attempt } if attempt > 1 => {
            Piece::painted(format!("x{attempt}"), Tint::Dim, style)
        }
        RowState::Pending
        | RowState::Active { .. }
        | RowState::Checking
        | RowState::Failed { .. }
        | RowState::Skipped
        | RowState::Aborted
        | RowState::Dormant => Piece::plain(String::new()),
    }
}

/// What the row is for, spelled the way `status` and `log` spell it.
fn chip(row: &Row) -> String {
    match &row.kind {
        // An aided sample has no position of its own and moves nothing, so the
        // interval and the cap would be reporting somebody else's business.
        Kind::Drill { aided: true, .. } => "aided".to_owned(),
        Kind::Drill {
            occasion,
            interval_days,
            at_cap,
            aided: false,
        } => {
            let mut text = format!("{} · {interval_days}d", occasion.word());
            if *at_cap {
                text.push_str(" · at cap");
            }
            text
        }
        Kind::Dormant => "dormant".to_owned(),
    }
}

/// How loudly the intention is said.
///
/// Only where nothing can check what is typed, which is where the sentence
/// matters most and is least likely to be read.
fn intention_tint(row: &Row) -> Tint {
    match resting(Some(row)) {
        Tone::Unchecked => Tint::Yellow,
        Tone::Calm | Tone::Alarm => Tint::Dim,
    }
}

/// What this prompt is asking for, said where and when it applies.
///
/// The discipline is one line, and this is the only place it reaches a person at
/// the moment it is due.
fn intention(row: &Row) -> &'static str {
    match &row.kind {
        Kind::Drill { aided: true, .. } => "looked up, so this one measures nothing",
        Kind::Drill { aided: false, .. } => "from memory — submit nothing to concede",
        // Never at the prompt, so never said; the row's chip says it instead.
        Kind::Dormant => "dormant here — nothing to ask",
    }
}

/// What the tally is a tally of.
const RECORDED: &str = "recorded  ";

/// How the sitting went, in one line.
fn tally(outturn: &Outturn, turns: usize) -> String {
    let landings = outturn.landings(turns);
    let mut parts = Vec::new();
    match landings.as_slice() {
        [] => {}
        [only] => parts.push(only.alone().to_owned()),
        many => {
            for (landing, one, more) in ORDER {
                let count = many.iter().filter(|found| **found == *landing).count();
                if count > 0 {
                    parts.push(relic_core::fmt::plural(count, one, more));
                }
            }
        }
    }
    if outturn.aborted {
        parts.push("abandoned".to_owned());
    }
    if parts.is_empty() {
        return "nothing recorded".to_owned();
    }
    parts.join(" · ")
}

/// The buckets, in the order they are read out, with what to call one and many.
const ORDER: &[(Landing, &str, &str)] = &[
    (Landing::Pass, "pass", "passes"),
    (Landing::Lapse, "lapse", "lapses"),
    (Landing::Miss, "miss", "misses"),
    (Landing::Skipped, "skipped", "skipped"),
    (Landing::Aided, "aided", "aided"),
    (Landing::Dormant, "dormant", "dormant"),
];

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use relic_core::style::{Style, Tint};

    use super::{Frame, Kind, Note, Row, RowState, SLOTS, card};
    use crate::corpus::record::Outcome;
    use crate::ladder::{Occasion, Rung};
    use crate::sitting::{Capture, Outturn};
    use crate::tui::card::{CONTENT, Card, Field, Reveal, Tone, WIDTH};

    /// A card over a field nothing has been typed into, which is what every
    /// assertion about the layout wants.
    fn card_with(frame: &Frame<'_>, style: Style) -> Card {
        card(frame, &Field::blind(), style)
    }

    fn drill_row(name: &str, occasion: Occasion, aided: bool, state: RowState) -> Row {
        Row {
            slug: name.parse().unwrap(),
            kind: Kind::Drill {
                occasion,
                aided,
                interval_days: 30,
                at_cap: true,
            },
            state,
        }
    }

    fn dormant_row(name: &str, state: RowState) -> Row {
        Row {
            slug: name.parse().unwrap(),
            kind: Kind::Dormant,
            state,
        }
    }

    /// Every state a row can reach, so the layout is checked against the content
    /// it actually has to hold rather than against a happy one.
    fn every_state() -> Vec<RowState> {
        vec![
            RowState::Pending,
            RowState::Active { attempt: 3 },
            RowState::Checking,
            RowState::Passed {
                total_ms: Some(12_345),
                retries: 2,
            },
            RowState::Failed { attempt: 2 },
            RowState::Skipped,
            RowState::Aborted,
            RowState::Dormant,
        ]
    }

    fn rows() -> Vec<Row> {
        let mut out = Vec::new();
        for state in every_state() {
            out.push(drill_row("escrow-p", Occasion::Review, false, state));
            out.push(drill_row("op-master", Occasion::Practice, true, state));
            out.push(dormant_row("flagship-login", state));
        }
        out
    }

    fn outturn() -> Outturn {
        Outturn {
            captures: vec![Capture {
                turn: 0,
                occasion: Occasion::Review,
                aided: false,
                ordinal: 1,
                outcome: Outcome::Pass,
                ttfk_ms: Some(900),
                total_ms: Some(3_000),
                corrections: 0,
                paste_accepted: 0,
                paste_refused: 0,
                rung_after: Rung::FIRST,
            }],
            dormant: vec![1],
            aborted: false,
            rows: Vec::new(),
        }
    }

    #[test]
    fn every_line_of_every_card_is_exactly_the_card_width() {
        // The widest line under the field the binary can produce: the longest
        // refusal beside both chips. `Card::edge` asserts on it, and nothing
        // else in the suite reaches this line at all.
        let widest = crate::tui::Refusal::Full
            .status()
            .expect("a refusal says something");
        let full = "x".repeat(crate::tui::card::FIELD);
        for row in rows() {
            let list = vec![row];
            for tone in [Tone::Calm, Tone::Alarm] {
                for lookup in [false, true] {
                    for status in ["not it · try 2 of 3", widest] {
                        for field in [
                            Field::blind(),
                            Field::empty(Reveal::Masked),
                            Field {
                                reveal: Reveal::Shown,
                                drawn: &full,
                                clipped: (true, true),
                                column: 0,
                            },
                        ] {
                            let mut frame = Frame::running(date(2026, 9, 13), &list, Some(0));
                            frame.tone = tone;
                            frame.lookup = lookup;
                            frame.status = Some(status.to_owned());
                            for line in card(&frame, &field, Style::PLAIN).render() {
                                assert_eq!(line.chars().count(), WIDTH, "{line}");
                                assert!(!line.contains('\n'), "no line carries a newline");
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_card_names_the_reveal_exactly_where_it_is_on_offer() {
        let list = rows();
        let drawn = |field: &Field<'_>| {
            let frame = Frame::running(date(2026, 9, 13), &list, Some(0));
            card(&frame, field, Style::PLAIN).render().join("\n")
        };
        assert!(
            !drawn(&Field::blind()).contains("^R"),
            "a cold prompt names no reveal"
        );
        assert!(drawn(&Field::empty(Reveal::Masked)).contains("^R  show"));
        assert!(drawn(&Field::empty(Reveal::Shown)).contains("^R  hide"));
    }

    #[test]
    fn a_refusal_that_crowds_the_line_drops_the_chip_and_not_itself() {
        let list = rows();
        // Nothing the binary says is this long. The mechanism is what is under
        // test: a line that cannot hold both drops the chip, never the refusal,
        // and never overruns the box.
        let mut frame = Frame::running(date(2026, 9, 13), &list, Some(0));
        frame.lookup = true;
        frame.status = Some(format!(
            "{:<width$}",
            "longer than a secret",
            width = CONTENT
        ));
        let rendered = card(&frame, &Field::empty(Reveal::Masked), Style::PLAIN)
            .render()
            .join("\n");
        assert!(rendered.contains("longer than a secret"), "{rendered}");
        assert!(!rendered.contains("^R"), "the chip is what gives");
    }

    #[test]
    fn every_state_of_a_sitting_is_the_same_rectangle() {
        let list = rows();
        let expected = list.len().saturating_add(SLOTS);
        let mut heights = Vec::new();
        for active in 0..list.len() {
            let frame = Frame::running(date(2026, 9, 13), &list, Some(active));
            heights.push(card(&frame, &Field::blind(), Style::PLAIN).height());
        }
        let done = outturn();
        let notes = vec![Note {
            label: "escrow-p@2".to_owned(),
            said: "a note".to_owned(),
        }];
        heights.push(
            card_with(
                &Frame::done(date(2026, 9, 13), &list, &done, &notes),
                Style::PLAIN,
            )
            .height(),
        );
        assert!(
            heights.iter().all(|height| *height == heights[0]),
            "{heights:?}"
        );
        assert!(heights[0] >= expected);
    }

    #[test]
    fn the_field_sits_on_the_same_line_in_every_state() {
        let list = rows();
        let mut lines = Vec::new();
        for active in 0..list.len() {
            let frame = Frame::running(date(2026, 9, 13), &list, Some(active));
            let rendered = card(&frame, &Field::blind(), Style::PLAIN).render();
            lines.push(field_line(&rendered));
        }
        assert!(lines.iter().all(|line| *line == lines[0]), "{lines:?}");
    }

    /// Where the field's own block begins, whichever of its two shapes it wears.
    ///
    /// A prompt being typed into draws a box; one waiting on its verifier draws
    /// the same three lines with the box taken away. Both have to begin on the
    /// same row or the window moves under the reader, which is the invariant.
    fn field_line(rendered: &[String]) -> Option<usize> {
        // The chrome's own top corner is line zero, and the title beside it may
        // say anything, so it is skipped by position rather than by wording.
        if let Some(found) = rendered
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, line)| line.contains('╭'))
        {
            return Some(found.0);
        }
        rendered
            .iter()
            .position(|line| line.contains(crate::tui::WORKING))
            .map(|line| line.saturating_sub(1))
    }

    #[test]
    fn a_prompt_waiting_on_its_verifier_keeps_the_rectangle_and_drops_the_box() {
        let list = vec![drill_row(
            "escrow-p",
            Occasion::Review,
            false,
            RowState::Checking,
        )];
        let frame = Frame::running(date(2026, 9, 13), &list, Some(0));
        let drawn = card(&frame, &Field::blind(), Style::PLAIN);
        assert!(drawn.caret().is_none(), "no cursor where nothing is typed");
        let rendered = drawn.render();
        assert!(
            rendered
                .iter()
                .any(|line| line.contains(crate::tui::WORKING))
        );

        let typing = vec![drill_row(
            "escrow-p",
            Occasion::Review,
            false,
            RowState::Active { attempt: 1 },
        )];
        let other = card_with(
            &Frame::running(date(2026, 9, 13), &typing, Some(0)),
            Style::PLAIN,
        );
        assert_eq!(drawn.height(), other.height());
        assert_eq!(field_line(&rendered), field_line(&other.render()));
    }

    #[test]
    fn no_real_content_ever_reaches_the_truncation_net() {
        for row in rows() {
            let list = vec![row];
            let frame = Frame::running(date(2026, 9, 13), &list, Some(0));
            for line in card(&frame, &Field::blind(), Style::PLAIN).render() {
                assert!(!line.contains('…'), "{line}");
            }
        }
    }

    #[test]
    fn a_dormant_row_says_so_and_asks_for_nothing() {
        let list = vec![
            dormant_row("escrow-p", RowState::Dormant),
            drill_row(
                "a",
                Occasion::Review,
                false,
                RowState::Active { attempt: 1 },
            ),
        ];
        let frame = Frame::running(date(2026, 9, 13), &list, Some(1));
        let rendered = card(&frame, &Field::blind(), Style::PLAIN)
            .render()
            .join("\n");
        assert!(rendered.contains("escrow-p"));
        assert!(rendered.contains("dormant"));
        assert!(
            !rendered.contains("nothing here can check it"),
            "the sitting takes no secret for a dormant row"
        );
        assert_eq!(super::icon(&list[0]), ("–", Tint::Dim));
    }

    #[test]
    fn a_drill_states_the_discipline_where_it_applies() {
        let list = vec![drill_row(
            "a",
            Occasion::Review,
            false,
            RowState::Active { attempt: 1 },
        )];
        let frame = Frame::running(date(2026, 9, 13), &list, Some(0));
        let rendered = card(&frame, &Field::blind(), Style::PLAIN)
            .render()
            .join("\n");
        assert!(rendered.contains("from memory"));
        assert!(rendered.contains("review · 30d · at cap"));

        let aided = vec![drill_row(
            "a",
            Occasion::Review,
            true,
            RowState::Active { attempt: 1 },
        )];
        let frame = Frame::running(date(2026, 9, 13), &aided, Some(0));
        let rendered = card(&frame, &Field::blind(), Style::PLAIN)
            .render()
            .join("\n");
        assert!(rendered.contains("measures nothing"));
        assert!(
            rendered.contains("aided") && !rendered.contains("30d"),
            "an aided sample has no interval of its own to report"
        );
    }

    #[test]
    fn an_empty_roster_says_so_rather_than_drawing_a_blank_box() {
        let frame = Frame::running(date(2026, 9, 13), &[], None);
        let rendered = card(&frame, &Field::blind(), Style::PLAIN)
            .render()
            .join("\n");
        assert!(rendered.contains("nothing to drill"));
    }

    #[test]
    fn the_closing_tally_counts_every_turn_exactly_once() {
        let list = vec![
            drill_row(
                "a",
                Occasion::Review,
                false,
                RowState::Passed {
                    total_ms: Some(1_000),
                    retries: 0,
                },
            ),
            dormant_row("b", RowState::Dormant),
        ];
        let done = outturn();
        let rendered = card_with(
            &Frame::done(date(2026, 9, 13), &list, &done, &[]),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(rendered.contains("1 pass"));
        assert!(rendered.contains("1 dormant"));
        assert!(rendered.contains("press any key"));
    }

    #[test]
    fn no_frame_ever_carries_what_was_typed() {
        // The field is blind, so nothing a person types can reach a rendered
        // line. The sitting passes a count of zero and the card never sees the
        // buffer at all; this pins that the wiring stays that way.
        let list = rows();
        for active in 0..list.len() {
            let frame = Frame::running(date(2026, 9, 13), &list, Some(active));
            for line in card(&frame, &Field::blind(), Style::PLAIN).render() {
                assert!(!line.contains("hunter2"), "{line}");
            }
        }
    }

    #[test]
    fn a_green_tick_is_only_ever_an_unaided_first_try_pass() {
        let clean = RowState::Passed {
            total_ms: Some(1_000),
            retries: 0,
        };
        let retried = RowState::Passed {
            total_ms: Some(1_000),
            retries: 1,
        };
        assert_eq!(
            super::icon(&drill_row("a", Occasion::Review, false, clean)),
            ("✓", Tint::Green)
        );
        for row in [
            drill_row("a", Occasion::Review, true, clean),
            drill_row("a", Occasion::Review, false, retried),
            drill_row("a", Occasion::Review, true, retried),
        ] {
            assert_eq!(super::icon(&row), ("✓", Tint::Yellow));
        }
    }

    #[test]
    fn nothing_that_was_not_measured_is_ever_green() {
        // The whole vocabulary, so a new state cannot quietly claim a verdict:
        // green is a clean pass and nothing else wears it.
        let states = [
            RowState::Pending,
            RowState::Active { attempt: 1 },
            RowState::Checking,
            RowState::Failed { attempt: 1 },
            RowState::Skipped,
            RowState::Aborted,
            RowState::Dormant,
        ];
        for state in states {
            let (_, tint) = super::icon(&drill_row("a", Occasion::Review, false, state));
            assert_ne!(tint, Tint::Green, "{state:?}");
        }
    }
}
