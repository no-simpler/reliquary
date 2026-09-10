//! What the drill puts on a card.
//!
//! Building one is a pure function from a frame to a card, so the whole visual
//! design is under snapshot test and the terminal adapter only has to place what
//! it is handed. The box, the palette and the field are the shared dialog's and
//! live in `tui::card`; what is here is the drill's own layout.
//!
//! Every state of a sitting is the same rectangle. The list, the intention and
//! the field sit at fixed lines, so what changes between states changes inside a
//! reserved column or on the line under the field, and the field itself never
//! moves. Where a word would only repeat what a glyph already says, the glyph
//! carries it alone.

use jiff::civil::Date;
use relic_core::style::{Style, Tint};

use super::{Item, Landed, Sitting};
use crate::ladder::{Class, Step};
use crate::slug::Slug;
use crate::tui::card::{CONTENT, Card, Piece, Reveal, Tone, join};

/// What points at the slug being asked about.
const MARKER: &str = "▸ ";

/// The ladder chip's column, wide enough for the fullest one.
const CHIP: usize = 28;

/// The column holding one glyph of standing.
const ICON: usize = 3;

/// Lines a card holds beyond one per slug: air, the intention, the three of the
/// field, air, and the line under it.
pub const SLOTS: usize = 7;

/// Where one slug has got to.
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
    /// This machine has no verifier for it.
    Unverifiable,
}

/// One line of the card.
#[derive(Clone, Debug)]
pub struct Row {
    /// The slug.
    pub slug: Slug,
    /// What an entry counts as.
    pub class: Class,
    /// Where it sits on the ladder.
    pub step: Step,
    /// Whether the interval reaches long-horizon evidence.
    pub stretch: bool,
    /// Where it has got to.
    pub state: RowState,
}

impl Row {
    /// A row for an item nobody has reached yet.
    pub fn pending(item: &Item) -> Self {
        Self {
            slug: item.slug.clone(),
            class: item.class,
            step: item.step,
            stretch: item.stretch(),
            state: RowState::Pending,
        }
    }
}

/// What to draw.
#[derive(Clone, Debug)]
pub struct Frame<'a> {
    /// The drill day.
    pub today: Date,
    /// Every slug in the sitting.
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
    pub sitting: Option<&'a Sitting>,
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
            sitting: None,
            notes: &[],
        }
    }

    /// The closing frame.
    pub fn done(today: Date, rows: &'a [Row], sitting: &'a Sitting, notes: &'a [String]) -> Self {
        Self {
            today,
            rows,
            active: None,
            tone: Tone::Calm,
            status: None,
            lookup: false,
            sitting: Some(sitting),
            notes,
        }
    }
}

pub fn card(frame: &Frame<'_>, style: Style) -> Card {
    let title = if frame.sitting.is_some() {
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
        card.say(intention(row), Tint::Dim);
        // The drill is blind, so there is nothing typed for the field to show.
        // It is asked for anyway, through the one place in the binary where a
        // typed secret becomes something on a screen.
        card.entry(0, Reveal::Blind, frame.tone);
        card.gap();
        card.line(under(frame, style));
    }

    if let Some(sitting) = frame.sitting {
        card.gap().say(tally(sitting), Tint::Bold);
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

/// The line under the field: how this attempt is going, and the one branch a
/// person could not guess at.
///
/// Enter submits and escape leaves, which nobody has to be told. The lookup is
/// neither, and it is only ever on offer once a cold attempt is on record.
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

/// One slug's line: a marker, its name, where it stands on the ladder, and one
/// glyph of standing with whatever that glyph needs qualifying by.
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
        RowState::Pending | RowState::Active { .. } => (" ", Tint::Dim),
        RowState::Checking => ("·", Tint::Dim),
        RowState::Passed { .. } => ("✓", Tint::Green),
        RowState::Failed { .. } => ("✗", Tint::Red),
        RowState::Skipped | RowState::Aborted => ("–", Tint::Dim),
        RowState::Unverifiable => ("!", Tint::Yellow),
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
        RowState::Unverifiable => Piece::painted("no verifier", Tint::Dim, style),
        RowState::Pending
        | RowState::Active { .. }
        | RowState::Checking
        | RowState::Failed { .. }
        | RowState::Skipped
        | RowState::Aborted => Piece::plain(String::new()),
    }
}

/// The class and the interval, spelled the way `status` and `log` spell them.
fn chip(row: &Row) -> String {
    let class = match row.class {
        Class::Review => "review",
        Class::Practice => "practice",
        Class::Probe => "probe",
        // An aided entry has no position of its own and moves nothing, so the
        // interval and the cap would be reporting somebody else's business.
        // What it measures is on the line above the field; the chip need not
        // say it twice.
        Class::Aided => return "aided".to_owned(),
    };
    let mut text = format!("{class} · {}d", row.step.interval());
    if row.step.at_cap() {
        text.push_str(" · at cap");
    }
    if row.stretch {
        text.push_str(" · stretch");
    }
    text
}

/// What this prompt is asking for, said where and when it applies.
///
/// The discipline is one line — the drill runs before the lookup — and this is
/// the only place it reaches a person at the moment it is due. It names the way
/// out too, because conceding by submitting nothing is the one thing here that
/// is not already a convention.
fn intention(row: &Row) -> &'static str {
    if row.class.unaided() {
        "from memory — submit nothing to concede"
    } else {
        "looked up, so this one measures nothing"
    }
}

/// How the sitting went, in one line.
///
/// Counts of one thing rather than a count of one thing beside a count of
/// another: every slug lands in exactly one bucket, so the numbers add up to
/// what was drilled. One slug needs no arithmetic at all, so it is described
/// rather than counted.
fn tally(sitting: &Sitting) -> String {
    let landings = sitting.landings();
    let mut parts = Vec::new();
    match landings.as_slice() {
        [] => {}
        [only] => parts.push(alone(*only).to_owned()),
        many => {
            for (landing, one, more) in ORDER {
                let count = many.iter().filter(|found| **found == *landing).count();
                if count > 0 {
                    parts.push(relic_core::fmt::plural(count, one, more));
                }
            }
        }
    }
    if sitting.aborted {
        parts.push("abandoned".to_owned());
    }
    if parts.is_empty() {
        return "nothing recorded".to_owned();
    }
    parts.join(" · ")
}

/// The buckets, in the order they are read out, with what to call one and many.
const ORDER: &[(Landed, &str, &str)] = &[
    (Landed::Pass, "pass", "passes"),
    (Landed::Lapse, "lapse", "lapses"),
    (Landed::Miss, "miss", "misses"),
    (Landed::Skipped, "skipped", "skipped"),
    (Landed::Aided, "aided", "aided"),
];

/// What one slug on its own did, said rather than counted.
fn alone(landing: Landed) -> &'static str {
    match landing {
        Landed::Pass => "passed",
        Landed::Lapse => "lapsed",
        Landed::Miss => "missed",
        Landed::Skipped => "skipped",
        Landed::Aided => "aided",
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use relic_core::style::Style;

    use super::{Frame, Row, RowState, SLOTS, card};
    use crate::drill::{Sitting, Taken};
    use crate::ladder::{Class, Step};
    use crate::log::Outcome;
    use crate::tui::card::{Tone, WIDTH};

    fn row(name: &str, class: Class, state: RowState) -> Row {
        Row {
            slug: name.parse().unwrap(),
            class,
            step: Step::cap(),
            stretch: false,
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
            RowState::Unverifiable,
        ]
    }

    /// Every frame one sitting can put on the screen.
    fn every_frame(rows: &[Row]) -> Vec<Frame<'_>> {
        let mut out = Vec::new();
        for tone in [Tone::Calm, Tone::Alarm] {
            for lookup in [false, true] {
                for status in [
                    None,
                    Some("try 3 of 3".to_owned()),
                    Some("out of tries".to_owned()),
                    Some("that is longer than a secret this tool will take".to_owned()),
                ] {
                    let mut frame = Frame::running(date(2026, 9, 10), rows, Some(0));
                    frame.tone = tone;
                    frame.lookup = lookup;
                    frame.status = status;
                    out.push(frame);
                }
            }
        }
        out
    }

    #[test]
    fn every_line_of_every_card_is_exactly_the_card_width() {
        for state in every_state() {
            for class in [Class::Review, Class::Practice, Class::Probe, Class::Aided] {
                let rows = vec![
                    row("escrow-p", class, state),
                    row("flagship-login", class, state),
                ];
                for frame in every_frame(&rows) {
                    for line in card(&frame, Style::PLAIN).render() {
                        assert_eq!(line.chars().count(), WIDTH, "{state:?} {class:?} {line}");
                    }
                }
            }
        }
    }

    #[test]
    fn every_state_of_a_sitting_is_the_same_rectangle() {
        let names = ["escrow-p", "flagship-login"];
        let wanted = names.len() + SLOTS + 4;
        for state in every_state() {
            let rows: Vec<Row> = names
                .iter()
                .map(|name| row(name, Class::Review, state))
                .collect();
            for frame in every_frame(&rows) {
                assert_eq!(
                    card(&frame, Style::PLAIN).render().len(),
                    wanted,
                    "{state:?} changed the height"
                );
            }
        }
    }

    #[test]
    fn the_field_sits_on_the_same_line_in_every_state() {
        let mut seen = std::collections::BTreeSet::new();
        for state in every_state() {
            let rows = vec![row("a", Class::Review, state)];
            for frame in every_frame(&rows) {
                if let Some((line, column)) = card(&frame, Style::PLAIN).caret() {
                    seen.insert((line, column));
                }
            }
        }
        assert_eq!(seen.len(), 1, "the field moved between states: {seen:?}");
    }

    #[test]
    fn the_closing_card_is_the_same_rectangle_as_the_sitting() {
        let rows = vec![
            row(
                "a",
                Class::Review,
                RowState::Passed {
                    total_ms: Some(400),
                    retries: 0,
                },
            ),
            row(
                "b",
                Class::Practice,
                RowState::Passed {
                    total_ms: Some(900),
                    retries: 1,
                },
            ),
        ];
        let sitting = Sitting::default();
        let running = card(
            &Frame::running(date(2026, 9, 10), &rows, Some(0)),
            Style::PLAIN,
        );
        let done = card(
            &Frame::done(date(2026, 9, 10), &rows, &sitting, &[]),
            Style::PLAIN,
        );
        assert_eq!(running.render().len(), done.render().len());
    }

    #[test]
    fn no_real_content_ever_reaches_the_truncation_net() {
        for state in every_state() {
            for class in [Class::Review, Class::Aided] {
                let mut wide = row("flagship-login", class, state);
                wide.stretch = true;
                for frame in every_frame(&[wide]) {
                    let text = card(&frame, Style::PLAIN).render().join("\n");
                    assert!(!text.contains('…'), "{state:?} was cut: {text}");
                }
            }
        }
    }

    #[test]
    fn the_prompt_says_what_it_wants_and_how_to_concede() {
        let rows = vec![row("a", Class::Review, RowState::Active { attempt: 1 })];
        let text = card(
            &Frame::running(date(2026, 9, 10), &rows, Some(0)),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(text.contains("from memory"), "{text}");
        assert!(text.contains("submit nothing to concede"), "{text}");
    }

    #[test]
    fn an_aided_prompt_says_it_measures_nothing() {
        let rows = vec![row("a", Class::Aided, RowState::Active { attempt: 1 })];
        let text = card(
            &Frame::running(date(2026, 9, 10), &rows, Some(0)),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(text.contains("measures nothing"), "{text}");
    }

    #[test]
    fn the_lookup_is_named_only_where_it_is_on_offer() {
        let rows = vec![row("a", Class::Review, RowState::Active { attempt: 1 })];
        let mut frame = Frame::running(date(2026, 9, 10), &rows, Some(0));
        assert!(
            !card(&frame, Style::PLAIN)
                .render()
                .join("\n")
                .contains("^L")
        );
        frame.lookup = true;
        assert!(
            card(&frame, Style::PLAIN)
                .render()
                .join("\n")
                .contains("^L")
        );
    }

    #[test]
    fn nothing_on_the_card_spells_out_what_a_key_already_says() {
        let rows = vec![row("a", Class::Review, RowState::Active { attempt: 2 })];
        let mut frame = Frame::running(date(2026, 9, 10), &rows, Some(0));
        frame.lookup = true;
        frame.status = Some("try 2 of 3".to_owned());
        let text = card(&frame, Style::PLAIN).render().join("\n");
        for spelled_out in ["[enter]", "[s]", "try again", "move on", "lapse recorded"] {
            assert!(!text.contains(spelled_out), "{spelled_out:?} in {text}");
        }
    }

    #[test]
    fn a_miss_shows_a_count_rather_than_a_verdict() {
        let rows = vec![row("a", Class::Review, RowState::Active { attempt: 2 })];
        let mut frame = Frame::running(date(2026, 9, 10), &rows, Some(0));
        frame.status = Some("try 2 of 3".to_owned());
        let text = card(&frame, Style::PLAIN).render().join("\n");
        assert!(text.contains("try 2 of 3"), "{text}");
        assert!(!text.contains("wrong"), "{text}");
    }

    #[test]
    fn the_chip_spells_the_interval_the_way_the_tables_do() {
        let mut climbing = row("a", Class::Review, RowState::Pending);
        climbing.step = Step::from_recorded(2);
        let text = card(
            &Frame::running(date(2026, 9, 10), &[climbing], None),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(text.contains("review · 2d"), "{text}");
        assert!(!text.contains("day 2"), "{text}");
        assert!(!text.contains("at cap"), "{text}");

        let mut capped = row("a", Class::Review, RowState::Pending);
        capped.stretch = true;
        let text = card(
            &Frame::running(date(2026, 9, 10), &[capped], None),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(text.contains("review · 7d · at cap · stretch"), "{text}");
    }

    #[test]
    fn a_pass_is_a_glyph_and_a_number() {
        let rows = vec![row(
            "a",
            Class::Review,
            RowState::Passed {
                total_ms: Some(1_400),
                retries: 1,
            },
        )];
        let text = card(
            &Frame::running(date(2026, 9, 10), &rows, None),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(text.contains('✓'), "{text}");
        assert!(text.contains("1.4s"), "{text}");
        assert!(text.contains("x2"), "{text}");
    }

    #[test]
    fn a_slug_with_no_verifier_says_what_to_do_about_it() {
        let rows = vec![row("a", Class::Review, RowState::Unverifiable)];
        let text = card(
            &Frame::running(date(2026, 9, 10), &rows, None),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(text.contains("no verifier"), "{text}");
    }

    #[test]
    fn the_prompt_opens_a_field_and_puts_the_cursor_in_it() {
        let rows = vec![row("a", Class::Review, RowState::Active { attempt: 1 })];
        let drawn = card(
            &Frame::running(date(2026, 9, 10), &rows, Some(0)),
            Style::PLAIN,
        );
        assert!(drawn.caret().is_some(), "the field takes the cursor");
        let lines = drawn.render();
        assert!(lines.iter().any(|line| line.contains('╭')), "{lines:?}");
        assert!(lines.iter().any(|line| line.contains('╰')), "{lines:?}");
    }

    #[test]
    fn a_row_that_is_not_at_the_prompt_opens_no_field() {
        let rows = vec![row("a", Class::Review, RowState::Pending)];
        let drawn = card(
            &Frame::running(date(2026, 9, 10), &rows, None),
            Style::PLAIN,
        );
        assert_eq!(drawn.caret(), None);
    }

    #[test]
    fn colour_changes_the_bytes_and_not_the_shape() {
        let rows = vec![row("a", Class::Review, RowState::Active { attempt: 1 })];
        let frame = Frame::running(date(2026, 9, 10), &rows, Some(0));
        let plain = card(&frame, Style::PLAIN).render();
        let painted = card(&frame, Style::COLOUR).render();
        assert_eq!(plain.len(), painted.len());
        for (a, b) in plain.iter().zip(painted.iter()) {
            assert!(b.len() >= a.len());
        }
    }

    #[test]
    fn the_closing_card_tallies_the_sitting_and_waits() {
        let rows = vec![row(
            "a",
            Class::Review,
            RowState::Passed {
                total_ms: Some(400),
                retries: 0,
            },
        )];
        let sitting = Sitting {
            taken: vec![Taken {
                item: 0,
                class: Class::Review,
                attempt: 1,
                outcome: Outcome::Pass,
                ttfk_ms: Some(10),
                total_ms: Some(400),
                corrections: 0,
                paste_refused: 0,
                step_after: Step::cap(),
            }],
            aborted: false,
            rows: Vec::new(),
        };
        let notes = vec!["escrow-p  cutover ready — 4 passes at 7d".to_owned()];
        let text = card(
            &Frame::done(date(2026, 9, 10), &rows, &sitting, &notes),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(text.contains("done"));
        // One slug is described, not counted.
        assert!(text.contains("passed"), "{text}");
        assert!(
            !text.contains("1 pass"),
            "a lone slug needs no arithmetic: {text}"
        );
        assert!(text.contains("press any key"));
        assert!(text.contains("cutover ready"));
    }

    /// A sitting with `n` slugs, each landing as its outcome and class say.
    fn sitting_of(entries: &[(Class, Outcome)]) -> Sitting {
        Sitting {
            taken: entries
                .iter()
                .enumerate()
                .map(|(item, (class, outcome))| Taken {
                    item,
                    class: *class,
                    attempt: 1,
                    outcome: *outcome,
                    ttfk_ms: Some(10),
                    total_ms: Some(400),
                    corrections: 0,
                    paste_refused: 0,
                    step_after: Step::FIRST,
                })
                .collect(),
            aborted: false,
            rows: Vec::new(),
        }
    }

    fn tallied(entries: &[(Class, Outcome)]) -> String {
        super::tally(&sitting_of(entries))
    }

    #[test]
    fn one_slug_is_described_and_several_are_counted() {
        assert_eq!(tallied(&[(Class::Review, Outcome::Pass)]), "passed");
        assert_eq!(tallied(&[(Class::Review, Outcome::Fail)]), "lapsed");
        assert_eq!(tallied(&[(Class::Practice, Outcome::Fail)]), "missed");
        assert_eq!(tallied(&[(Class::Review, Outcome::Skip)]), "skipped");
        assert_eq!(
            tallied(&[
                (Class::Review, Outcome::Pass),
                (Class::Review, Outcome::Fail)
            ]),
            "1 pass · 1 lapse"
        );
    }

    #[test]
    fn the_counts_add_up_to_what_was_drilled() {
        let entries = [
            (Class::Review, Outcome::Pass),
            (Class::Review, Outcome::Pass),
            (Class::Review, Outcome::Fail),
            (Class::Practice, Outcome::Blank),
            (Class::Probe, Outcome::Fail),
            (Class::Review, Outcome::Skip),
        ];
        assert_eq!(
            tallied(&entries),
            "2 passes · 1 lapse · 2 misses · 1 skipped"
        );
    }

    #[test]
    fn a_miss_that_moved_nothing_is_not_called_a_lapse() {
        // rote guide probes: failing a chosen horizon is a reading, not a lapse
        // in the schedule, and rote stats counts it that way too.
        assert_eq!(tallied(&[(Class::Probe, Outcome::Fail)]), "missed");
        assert_eq!(tallied(&[(Class::Practice, Outcome::Fail)]), "missed");
        assert_eq!(tallied(&[(Class::Review, Outcome::Fail)]), "lapsed");
    }

    #[test]
    fn an_abandoned_sitting_says_so_and_counts_no_reading_for_where_it_stopped() {
        let mut sitting = sitting_of(&[(Class::Review, Outcome::Pass)]);
        sitting.taken.push(Taken {
            item: 1,
            class: Class::Review,
            attempt: 1,
            outcome: Outcome::Abort,
            ttfk_ms: None,
            total_ms: None,
            corrections: 0,
            paste_refused: 0,
            step_after: Step::FIRST,
        });
        sitting.aborted = true;
        assert_eq!(super::tally(&sitting), "passed · abandoned");
    }

    #[test]
    fn an_empty_sitting_says_so_rather_than_drawing_an_empty_box() {
        let text = card(&Frame::running(date(2026, 9, 10), &[], None), Style::PLAIN)
            .render()
            .join("\n");
        assert!(text.contains("nothing to drill"), "{text}");
    }

    #[test]
    fn a_missing_duration_renders_as_a_dash_rather_than_zero() {
        let rows = vec![row(
            "a",
            Class::Review,
            RowState::Passed {
                total_ms: None,
                retries: 0,
            },
        )];
        let text = card(
            &Frame::running(date(2026, 9, 10), &rows, None),
            Style::PLAIN,
        )
        .render()
        .join("\n");
        assert!(text.contains('—'), "{text}");
    }
}
