//! What a sitting puts on a card.
//!
//! Building one is a pure function from a frame to a card, so the whole visual
//! design is under test and the terminal adapter only has to place what it is
//! handed. The box, the palette and the field are the shared dialog's and live
//! in `tui::card`; what is here is the sitting's own layout.
//!
//! Every state of a sitting is the same rectangle. The list, the context line,
//! the intention and the field sit at fixed lines, so what changes between
//! states changes inside a reserved column or on the line under the field, and
//! the field itself never moves.

use jiff::civil::Date;
use relic_core::style::{Style, Tint};

use super::{Outturn, Task, Turn};
use crate::corpus::drill::Landing;
use crate::ladder::Occasion;
use crate::slug::Slug;
use crate::tui::card::{CONTENT, Card, Piece, Reveal, Tone, join};

/// What points at the engram being asked about.
const MARKER: &str = "▸ ";

/// The chip's column, wide enough for the fullest one.
const CHIP: usize = 28;

/// The column holding one glyph of standing.
const ICON: usize = 3;

/// Lines a card holds beyond one per turn: air, the context line, the
/// intention, the three of the field, air, and the line under it.
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
    /// An attachment, waiting for the first of the pair.
    Claiming,
    /// An attachment, waiting for the second.
    Confirming,
    /// A verifier was minted here.
    Attached,
    /// The two entries never agreed, so nothing was minted.
    Differed,
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
    /// The secret is taken so a verifier can be made here.
    Attach {
        /// What is being continued.
        dossier: String,
        /// Whether a verifier is already held here.
        replacing: bool,
    },
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
                Task::Attach(attaching) => Kind::Attach {
                    dossier: attaching.dossier.clone(),
                    replacing: attaching.replacing,
                },
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
    pub notes: &'a [String],
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
    pub fn done(today: Date, rows: &'a [Row], outturn: &'a Outturn, notes: &'a [String]) -> Self {
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

/// Render one frame.
pub fn card(frame: &Frame<'_>, style: Style) -> Card {
    let title = if frame.outturn.is_some() {
        "done"
    } else {
        "rote"
    };
    let mut card = Card::new(title, crate::tui::card::stamp(frame.today), style);
    card.reserve(frame.rows.len().saturating_add(SLOTS));
    let names = frame
        .rows
        .iter()
        .map(|row| row.slug.as_str().chars().count())
        .max()
        .unwrap_or(0);

    if frame.rows.is_empty() {
        card.say("nothing to drill", Tint::Dim);
    }
    for (index, row) in frame.rows.iter().enumerate() {
        card.line(row_line(row, names, frame.active == Some(index), style));
    }

    if let Some(row) = frame.active.and_then(|index| frame.rows.get(index)) {
        card.gap();
        // Unconditional, so a drill's blank context line pads rather than
        // moving the field up into it.
        card.say(context(row), Tint::Dim);
        card.say(intention(row), Tint::Dim);
        // The sitting is blind, so there is nothing typed for the field to
        // show. It is asked for anyway, through the one place in the binary
        // where a typed secret becomes something on a screen.
        card.entry(0, Reveal::Blind, frame.tone);
        card.gap();
        card.line(under(frame, style));
    }

    if let Some(outturn) = frame.outturn {
        card.gap().say(tally(outturn, frame.rows.len()), Tint::Bold);
        for note in frame.notes {
            card.say(note.clone(), Tint::Dim);
        }
        let last = frame.rows.len().saturating_add(SLOTS).saturating_sub(1);
        while card.lines() < last {
            card.gap();
        }
        card.say("press any key", Tint::Dim);
    }
    card
}

/// The line under the field: how this prompt is going, and the one branch a
/// person could not guess at.
fn under(frame: &Frame<'_>, style: Style) -> Piece {
    let left = frame.status.clone().unwrap_or_default();
    let right = if frame.lookup { "^L  look it up" } else { "" };
    let gap = CONTENT
        .saturating_sub(left.chars().count())
        .saturating_sub(right.chars().count());
    join(&[
        Piece::painted(left, Tint::Dim, style),
        Piece::plain(" ".repeat(gap)),
        Piece::painted(right, Tint::Dim, style),
    ])
}

fn row_line(row: &Row, names: usize, active: bool, style: Style) -> Piece {
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
        Piece::painted(format!("{:<CHIP$}", chip(row)), Tint::Dim, style),
        Piece::painted(format!("{glyph:<ICON$}"), tint, style),
        detail(row, style),
    ])
}

/// The one character that says where a row stands.
fn icon(row: &Row) -> (&'static str, Tint) {
    match row.state {
        // The marker already says which row is being asked about, and a row
        // nobody has reached says nothing at all.
        RowState::Pending | RowState::Active { .. } | RowState::Claiming | RowState::Confirming => {
            (" ", Tint::Dim)
        }
        RowState::Checking => ("·", Tint::Dim),
        RowState::Passed { .. } => ("✓", Tint::Green),
        // Something was added, and nothing was judged. A green tick would claim
        // a verdict nobody rendered.
        RowState::Attached => ("+", Tint::Yellow),
        RowState::Failed { .. } => ("✗", Tint::Red),
        RowState::Differed => ("–", Tint::Red),
        RowState::Skipped | RowState::Aborted => ("–", Tint::Dim),
    }
}

/// What the glyph cannot say on its own.
fn detail(row: &Row, style: Style) -> Piece {
    match row.state {
        RowState::Passed { total_ms, retries } => {
            let mut text = crate::render::seconds(total_ms);
            if retries > 0 {
                use std::fmt::Write as _;
                let _ = write!(text, "   x{}", retries.saturating_add(1));
            }
            Piece::painted(text, Tint::Dim, style)
        }
        RowState::Failed { attempt } if attempt > 1 => {
            Piece::painted(format!("x{attempt}"), Tint::Dim, style)
        }
        RowState::Attached => Piece::painted("attached", Tint::Dim, style),
        RowState::Differed => Piece::painted("differed", Tint::Dim, style),
        RowState::Pending
        | RowState::Active { .. }
        | RowState::Checking
        | RowState::Claiming
        | RowState::Confirming
        | RowState::Failed { .. }
        | RowState::Skipped
        | RowState::Aborted => Piece::plain(String::new()),
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
        Kind::Attach { replacing, .. } => {
            if *replacing {
                "attach · replacing".to_owned()
            } else {
                "attach · dormant here".to_owned()
            }
        }
    }
}

/// What is being continued, for an attachment, and nothing for a drill.
fn context(row: &Row) -> String {
    match &row.kind {
        Kind::Drill { .. } => String::new(),
        Kind::Attach { dossier, .. } => fit(dossier),
    }
}

/// Trim a context line to what the card will hold, at a separator rather than
/// mid-word, so nothing real ever reaches the render choke point's net.
fn fit(text: &str) -> String {
    if text.chars().count() <= CONTENT {
        return text.to_owned();
    }
    let mut kept = String::new();
    for part in text.split(" · ") {
        let next = if kept.is_empty() {
            part.to_owned()
        } else {
            format!("{kept} · {part}")
        };
        if next.chars().count() > CONTENT {
            break;
        }
        kept = next;
    }
    if kept.is_empty() {
        kept.extend(text.chars().take(CONTENT));
    }
    kept
}

/// What this prompt is asking for, said where and when it applies.
///
/// The discipline is one line, and this is the only place it reaches a person at
/// the moment it is due.
fn intention(row: &Row) -> &'static str {
    match &row.kind {
        Kind::Drill { aided: true, .. } => "looked up, so this one measures nothing",
        Kind::Drill { aided: false, .. } => "from memory — submit nothing to concede",
        Kind::Attach { .. } => match row.state {
            RowState::Confirming => "again",
            RowState::Pending
            | RowState::Active { .. }
            | RowState::Checking
            | RowState::Passed { .. }
            | RowState::Failed { .. }
            | RowState::Skipped
            | RowState::Aborted
            | RowState::Claiming
            | RowState::Attached
            | RowState::Differed => "type it as you know it — nothing here can check it",
        },
    }
}

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
    (Landing::Attached, "attached", "attached"),
];

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use relic_core::style::Style;

    use super::{Frame, Kind, Row, RowState, SLOTS, card, fit};
    use crate::corpus::record::Outcome;
    use crate::ladder::{Occasion, Rung};
    use crate::sitting::{Capture, Outturn};
    use crate::tui::card::{CONTENT, Tone, WIDTH};

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

    fn attach_row(name: &str, state: RowState) -> Row {
        Row {
            slug: name.parse().unwrap(),
            kind: Kind::Attach {
                dossier: format!("{name}@2 · enrolled 2026-09-14 · rung 5 · reviewed 3d ago"),
                replacing: false,
            },
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
            RowState::Claiming,
            RowState::Confirming,
            RowState::Attached,
            RowState::Differed,
        ]
    }

    fn rows() -> Vec<Row> {
        let mut out = Vec::new();
        for state in every_state() {
            out.push(drill_row("escrow-p", Occasion::Review, false, state));
            out.push(drill_row("op-master", Occasion::Practice, true, state));
            out.push(attach_row("flagship-login", state));
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
                paste_refused: 0,
                rung_after: Rung::FIRST,
            }],
            attached: vec![1],
            aborted: false,
            rows: Vec::new(),
        }
    }

    #[test]
    fn every_line_of_every_card_is_exactly_the_card_width() {
        for row in rows() {
            let list = vec![row];
            for tone in [Tone::Calm, Tone::Alarm] {
                for lookup in [false, true] {
                    let mut frame = Frame::running(date(2026, 9, 13), &list, Some(0));
                    frame.tone = tone;
                    frame.lookup = lookup;
                    frame.status = Some("not it · try 2 of 3".to_owned());
                    for line in card(&frame, Style::PLAIN).render() {
                        assert_eq!(line.chars().count(), WIDTH, "{line}");
                        assert!(!line.contains('\n'), "no line carries a newline");
                    }
                }
            }
        }
    }

    #[test]
    fn every_state_of_a_sitting_is_the_same_rectangle() {
        let list = rows();
        let expected = list.len().saturating_add(SLOTS);
        let mut heights = Vec::new();
        for active in 0..list.len() {
            let frame = Frame::running(date(2026, 9, 13), &list, Some(active));
            heights.push(card(&frame, Style::PLAIN).height());
        }
        let done = outturn();
        let notes = vec!["a note".to_owned()];
        heights.push(
            card(
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
            let rendered = card(&frame, Style::PLAIN).render();
            let found = rendered
                .iter()
                .position(|line| line.contains('╭') && !line.contains("rote"));
            lines.push(found);
        }
        assert!(lines.iter().all(|line| *line == lines[0]), "{lines:?}");
    }

    #[test]
    fn no_real_content_ever_reaches_the_truncation_net() {
        for row in rows() {
            let list = vec![row];
            let frame = Frame::running(date(2026, 9, 13), &list, Some(0));
            for line in card(&frame, Style::PLAIN).render() {
                assert!(!line.contains('…'), "{line}");
            }
        }
    }

    #[test]
    fn a_long_context_line_is_trimmed_at_a_separator_rather_than_mid_word() {
        let long = format!(
            "{}@2 · enrolled 2026-09-14 · rung 5 · reviewed 3d ago",
            "a".repeat(32)
        );
        let trimmed = fit(&long);
        assert!(trimmed.chars().count() <= CONTENT);
        assert!(!trimmed.ends_with(' '));
        assert_eq!(fit("short"), "short");
    }

    #[test]
    fn an_attachment_says_what_it_is_continuing_and_offers_no_lookup() {
        let list = vec![attach_row("escrow-p", RowState::Claiming)];
        let frame = Frame::running(date(2026, 9, 13), &list, Some(0));
        let rendered = card(&frame, Style::PLAIN).render().join("\n");
        assert!(rendered.contains("attach · dormant here"));
        assert!(rendered.contains("enrolled 2026-09-14"));
        assert!(rendered.contains("nothing here can check it"));
        assert!(
            !rendered.contains("^L"),
            "there is nothing to look up against"
        );
    }

    #[test]
    fn the_second_half_of_a_pair_asks_again_and_nothing_else() {
        let list = vec![attach_row("escrow-p", RowState::Confirming)];
        let frame = Frame::running(date(2026, 9, 13), &list, Some(0));
        let rendered = card(&frame, Style::PLAIN).render().join("\n");
        assert!(rendered.contains("again"));
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
        let rendered = card(&frame, Style::PLAIN).render().join("\n");
        assert!(rendered.contains("from memory"));
        assert!(rendered.contains("review · 30d · at cap"));

        let aided = vec![drill_row(
            "a",
            Occasion::Review,
            true,
            RowState::Active { attempt: 1 },
        )];
        let frame = Frame::running(date(2026, 9, 13), &aided, Some(0));
        let rendered = card(&frame, Style::PLAIN).render().join("\n");
        assert!(rendered.contains("measures nothing"));
        assert!(
            rendered.contains("aided") && !rendered.contains("30d"),
            "an aided sample has no interval of its own to report"
        );
    }

    #[test]
    fn an_empty_roster_says_so_rather_than_drawing_a_blank_box() {
        let frame = Frame::running(date(2026, 9, 13), &[], None);
        let rendered = card(&frame, Style::PLAIN).render().join("\n");
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
            attach_row("b", RowState::Attached),
        ];
        let done = outturn();
        let rendered = card(
            &Frame::done(date(2026, 9, 13), &list, &done, &[]),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(rendered.contains("1 pass"));
        assert!(rendered.contains("1 attached"));
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
            for line in card(&frame, Style::PLAIN).render() {
                assert!(!line.contains("hunter2"), "{line}");
            }
        }
    }
}
