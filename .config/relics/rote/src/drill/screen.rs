//! What the drill puts on a card.
//!
//! Building one is a pure function from a frame to a card, so the whole visual
//! design is under snapshot test and the terminal adapter only has to place what
//! it is handed. The box, the palette and the entry line are the shared dialog's
//! and live in `tui::card`; what is here is the drill's own layout.

use jiff::civil::Date;

use super::{Item, Sitting};
use crate::ladder::{Class, Step};
use crate::slug::Slug;
use crate::tui::card::{
    BOLD, CONTENT, Card, DIM, GREEN, Piece, RED, Reveal, YELLOW, entry_line, join,
};

/// Where one slug has got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowState {
    /// Not reached yet.
    Pending,
    /// At the prompt.
    Active {
        /// Which try.
        attempt: u8,
        /// Tries after this one.
        left: u8,
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
        /// Tries after this one.
        left: u8,
        /// Whether this one moved the ladder.
        scored: bool,
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

/// Something to say once, under the card.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Notice {
    /// A paste arrived and was refused.
    PasteRefused,
    /// The entry was refused, and the next one is being chosen.
    Choose {
        /// Whether there are tries left to offer.
        retry: bool,
    },
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
    /// A one-off message.
    pub notice: Option<Notice>,
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
            notice: None,
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
            notice: None,
            sitting: Some(sitting),
            notes,
        }
    }
}

/// Draw the card.
/// Lay the frame out as a card.
pub fn card(frame: &Frame<'_>, color: bool) -> Card {
    let title = if frame.sitting.is_some() {
        "done"
    } else {
        "rote"
    };
    let stamp = frame.today.strftime("%a %-d %b").to_string();
    let mut card = Card::new(title, stamp, color);
    let names = frame
        .rows
        .iter()
        .map(|row| row.slug.as_str().chars().count())
        .max()
        .unwrap_or(0);

    if frame.rows.is_empty() {
        card.say("nothing to drill", DIM);
    }

    for (index, row) in frame.rows.iter().enumerate() {
        for piece in row_lines(row, names, color) {
            card.line(piece);
        }
        if frame.active == Some(index) {
            if let Some(text) = intention(row) {
                card.say(text, DIM);
            }
            card.line(prompt_line(row, color));
        }
    }

    match frame.notice {
        Some(Notice::PasteRefused) => {
            card.gap()
                .say("a paste was refused — the drill has to be typed", YELLOW);
        }
        Some(Notice::Choose { retry }) => {
            card.gap();
            if retry {
                card.say("[enter]  try again from memory", YELLOW);
            }
            card.say("[l]      look it up, then type it", YELLOW);
            card.say(
                if retry {
                    "[s]      move on"
                } else {
                    "[enter]  move on"
                },
                YELLOW,
            );
        }
        None => {}
    }

    if let Some(sitting) = frame.sitting {
        card.gap().say(tally(sitting), BOLD);
        for note in frame.notes {
            card.say(note.clone(), DIM);
        }
        card.gap().say("press any key", DIM);
    }
    card
}

/// The row, and the outcome beside it or under it.
///
/// A sentence-shaped outcome does not fit beside a name and a chip, so it takes
/// the line below rather than pushing the box wider than every other card in the
/// binary.
fn row_lines(row: &Row, names: usize, color: bool) -> Vec<Piece> {
    let indent = names.saturating_add(2);
    let head = join(&[
        Piece::painted(format!("{:<indent$}", row.slug.as_str()), BOLD, color),
        Piece::painted(chip(row), DIM, color),
    ]);
    let state = state_piece(row, color);
    if state.is_blank() {
        return vec![head];
    }
    if head.width().saturating_add(2).saturating_add(state.width()) <= CONTENT {
        return vec![join(&[head, Piece::plain("  "), state])];
    }
    vec![head, join(&[Piece::plain(" ".repeat(indent)), state])]
}

fn chip(row: &Row) -> String {
    let class = match row.class {
        Class::Review => "review",
        Class::Practice => "practice",
        Class::Probe => "probe",
        // An aided entry has no position of its own and moves nothing, so the
        // day and the cap would be reporting somebody else's business.
        Class::Aided => return format!("{:<28}", "aided · not a reading"),
    };
    let mut text = format!("{class} · day {}", row.step.interval());
    if row.step.at_cap() {
        text.push_str(" · at cap");
    }
    if row.stretch {
        text.push_str(" · stretch");
    }
    format!("{text:<28}")
}

/// What this prompt is asking for, said where and when it applies.
///
/// The discipline is one line — the drill runs before the lookup — and this is
/// the only place it reaches a person at the moment it is due.
fn intention(row: &Row) -> Option<String> {
    match row.state {
        RowState::Active { .. } if !row.class.unaided() => {
            Some("look it up now, then type it — this one measures nothing".to_owned())
        }
        RowState::Active { attempt: 1, .. } => {
            Some("from memory only — empty entry if there is nothing".to_owned())
        }
        RowState::Active { .. }
        | RowState::Pending
        | RowState::Checking
        | RowState::Passed { .. }
        | RowState::Failed { .. }
        | RowState::Skipped
        | RowState::Aborted
        | RowState::Unverifiable => None,
    }
}

fn state_piece(row: &Row, color: bool) -> Piece {
    match row.state {
        RowState::Pending | RowState::Active { .. } => Piece::plain(String::new()),
        RowState::Checking => Piece::painted("checking", DIM, color),
        RowState::Passed { total_ms, retries } => {
            let mut text = format!("✓  {}", seconds(total_ms));
            if retries > 0 {
                let _ = std::fmt::Write::write_fmt(
                    &mut text,
                    format_args!("  on try {}", retries.saturating_add(1)),
                );
                if row.class.scores() {
                    text.push_str(" · lapse recorded");
                }
            }
            Piece::painted(text, GREEN, color)
        }
        RowState::Failed {
            left,
            scored,
            attempt: _,
        } if !row.class.unaided() => {
            let _ = (left, scored);
            Piece::painted("✗  the vault and the verifier disagree", YELLOW, color)
        }
        RowState::Failed {
            left,
            scored,
            attempt: _,
        } => {
            let text = if left == 0 {
                "✗  wrong".to_owned()
            } else {
                format!("✗  wrong — {left} left")
            };
            let text = if scored {
                format!("{text}  lapse recorded")
            } else {
                text
            };
            Piece::painted(text, RED, color)
        }
        RowState::Skipped => Piece::painted("skipped", DIM, color),
        RowState::Aborted => Piece::painted("abandoned", YELLOW, color),
        RowState::Unverifiable => Piece::painted("no verifier here — re-enrol", YELLOW, color),
    }
}

fn prompt_line(row: &Row, color: bool) -> Piece {
    let hint = match row.state {
        RowState::Active { attempt, left } if attempt > 1 => {
            format!("   try {attempt}, {left} after this")
        }
        RowState::Active { .. }
        | RowState::Pending
        | RowState::Checking
        | RowState::Passed { .. }
        | RowState::Failed { .. }
        | RowState::Skipped
        | RowState::Aborted
        | RowState::Unverifiable => String::new(),
    };
    // The drill is blind, so there is nothing typed for the entry line to know
    // about. It is asked for anyway, through the one function every prompt in
    // the binary draws itself with.
    entry_line(0, Reveal::Blind, &hint, color)
}

fn seconds(total_ms: Option<u64>) -> String {
    match total_ms {
        Some(ms) => format!(
            "{:.1}s",
            f64::from(u32::try_from(ms).unwrap_or(u32::MAX)) / 1000.0
        ),
        None => "—".to_owned(),
    }
}

fn tally(sitting: &Sitting) -> String {
    let mut reviews = 0u32;
    let mut practice = 0u32;
    let mut probes = 0u32;
    for taken in sitting.taken.iter().filter(|t| t.attempt == 1) {
        match taken.class {
            Class::Review => reviews = reviews.saturating_add(1),
            Class::Practice => practice = practice.saturating_add(1),
            Class::Probe => probes = probes.saturating_add(1),
            Class::Aided => {}
        }
    }
    let mut parts = Vec::new();
    if reviews > 0 {
        parts.push(relic_core::fmt::plural(
            usize::try_from(reviews).unwrap_or(0),
            "review",
            "reviews",
        ));
    }
    if probes > 0 {
        parts.push(relic_core::fmt::plural(
            usize::try_from(probes).unwrap_or(0),
            "probe",
            "probes",
        ));
    }
    if practice > 0 {
        parts.push(format!("{practice} practice"));
    }
    let lapses = sitting.lapses();
    parts.push(relic_core::fmt::plural(lapses, "lapse", "lapses"));
    if sitting.aborted {
        parts.push("abandoned".to_owned());
    }
    if parts.is_empty() {
        return "nothing recorded".to_owned();
    }
    parts.join(" · ")
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::{Frame, Notice, Row, RowState, card};
    use crate::drill::{Sitting, Taken};
    use crate::ladder::{Class, Step};
    use crate::log::Outcome;
    use crate::tui::card::WIDTH;

    fn row(name: &str, class: Class, state: RowState) -> Row {
        Row {
            slug: name.parse().unwrap(),
            class,
            step: Step::cap(),
            stretch: false,
            state,
        }
    }

    /// Every state a row can reach, so the layout is checked against the
    /// content it actually has to hold rather than against a happy one.
    fn every_state() -> Vec<RowState> {
        vec![
            RowState::Pending,
            RowState::Active {
                attempt: 3,
                left: 0,
            },
            RowState::Checking,
            RowState::Passed {
                total_ms: Some(12_345),
                retries: 2,
            },
            RowState::Failed {
                attempt: 2,
                left: 1,
                scored: true,
            },
            RowState::Skipped,
            RowState::Aborted,
            RowState::Unverifiable,
        ]
    }

    #[test]
    fn every_line_of_every_card_is_exactly_the_card_width() {
        for state in every_state() {
            for class in [Class::Review, Class::Practice, Class::Probe, Class::Aided] {
                let rows = vec![
                    row("escrow-p", class, state),
                    row("flagship-login", class, state),
                ];
                let mut frame = Frame::running(date(2026, 9, 10), &rows, Some(0));
                for notice in [
                    None,
                    Some(Notice::PasteRefused),
                    Some(Notice::Choose { retry: true }),
                    Some(Notice::Choose { retry: false }),
                ] {
                    frame.notice = notice;
                    for line in card(&frame, false).render() {
                        assert_eq!(line.chars().count(), WIDTH, "{state:?} {class:?} {line}");
                    }
                }
            }
        }
    }

    #[test]
    fn no_real_content_ever_reaches_the_truncation_net() {
        for state in every_state() {
            let rows = vec![row("flagship-login", Class::Aided, state)];
            let mut frame = Frame::running(date(2026, 9, 10), &rows, Some(0));
            frame.notice = Some(Notice::Choose { retry: true });
            let text = card(&frame, false).render().join("\n");
            assert!(!text.contains('…'), "{state:?} was cut: {text}");
        }
    }

    #[test]
    fn the_first_prompt_says_what_it_is_asking_for() {
        let rows = vec![row(
            "a",
            Class::Review,
            RowState::Active {
                attempt: 1,
                left: 2,
            },
        )];
        let lines = card(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false).render();
        let text = lines.join("\n");
        assert!(text.contains("from memory only"), "{text}");
        assert!(text.contains("empty entry if there is nothing"), "{text}");
    }

    #[test]
    fn a_retry_is_not_told_again_what_the_first_try_was_told() {
        let rows = vec![row(
            "a",
            Class::Review,
            RowState::Active {
                attempt: 2,
                left: 1,
            },
        )];
        let lines = card(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false).render();
        assert!(!lines.join("\n").contains("from memory only"));
    }

    #[test]
    fn an_aided_prompt_says_it_measures_nothing() {
        let rows = vec![row(
            "a",
            Class::Aided,
            RowState::Active {
                attempt: 2,
                left: 0,
            },
        )];
        let lines = card(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false).render();
        let text = lines.join("\n");
        assert!(text.contains("look it up now"), "{text}");
        assert!(text.contains("aided · not a reading"), "{text}");
    }

    #[test]
    fn the_offer_after_a_miss_drops_the_retry_when_there_are_none_left() {
        let rows = vec![row(
            "a",
            Class::Review,
            RowState::Failed {
                attempt: 3,
                left: 0,
                scored: false,
            },
        )];
        let mut frame = Frame::running(date(2026, 9, 10), &rows, Some(0));
        frame.notice = Some(Notice::Choose { retry: false });
        let text = card(&frame, false).render().join("\n");
        assert!(text.contains("look it up, then type it"), "{text}");
        assert!(text.contains("[enter]  move on"), "{text}");
        assert!(!text.contains("try again from memory"), "{text}");

        frame.notice = Some(Notice::Choose { retry: true });
        assert!(
            card(&frame, false)
                .render()
                .join("\n")
                .contains("try again from memory")
        );
    }

    #[test]
    fn a_refused_aided_entry_names_the_disagreement_rather_than_a_lapse() {
        let rows = vec![row(
            "a",
            Class::Aided,
            RowState::Failed {
                attempt: 2,
                left: 0,
                scored: false,
            },
        )];
        let text = card(&Frame::running(date(2026, 9, 10), &rows, None), false)
            .render()
            .join("\n");
        assert!(
            text.contains("the vault and the verifier disagree"),
            "{text}"
        );
    }

    #[test]
    fn colour_changes_the_bytes_and_not_the_shape() {
        let rows = vec![row(
            "a",
            Class::Review,
            RowState::Passed {
                total_ms: Some(4_100),
                retries: 0,
            },
        )];
        let frame = Frame::running(date(2026, 9, 10), &rows, None);
        let plain = card(&frame, false).render();
        let painted = card(&frame, true).render();
        assert_eq!(plain.len(), painted.len());
        assert!(painted.iter().any(|line| line.contains("\x1b[")));
        assert!(!plain.iter().any(|line| line.contains("\x1b[")));
    }

    #[test]
    fn the_prompt_shows_a_cursor_and_never_an_echo() {
        let rows = vec![row(
            "a",
            Class::Review,
            RowState::Active {
                attempt: 1,
                left: 2,
            },
        )];
        let lines = card(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false).render();
        assert!(lines.iter().any(|line| line.contains("▸ ▉")), "{lines:?}");
    }

    #[test]
    fn a_retry_says_how_many_tries_are_left() {
        let rows = vec![row(
            "a",
            Class::Review,
            RowState::Active {
                attempt: 2,
                left: 1,
            },
        )];
        let lines = card(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false).render();
        assert!(lines.iter().any(|line| line.contains("try 2")));
    }

    #[test]
    fn a_refused_paste_says_so_on_the_card() {
        let rows = vec![row("a", Class::Review, RowState::Pending)];
        let mut frame = Frame::running(date(2026, 9, 10), &rows, Some(0));
        frame.notice = Some(Notice::PasteRefused);
        let lines = card(&frame, false).render();
        assert!(lines.iter().any(|line| line.contains("paste was refused")));
    }

    #[test]
    fn a_slug_with_no_verifier_says_what_to_do_about_it() {
        let rows = vec![row("a", Class::Review, RowState::Unverifiable)];
        let lines = card(&Frame::running(date(2026, 9, 10), &rows, None), false).render();
        assert!(lines.iter().any(|line| line.contains("re-enrol")));
    }

    #[test]
    fn the_closing_card_tallies_the_sitting_and_waits() {
        let rows = vec![
            row(
                "escrow-p",
                Class::Review,
                RowState::Passed {
                    total_ms: Some(4_100),
                    retries: 0,
                },
            ),
            row("op-master", Class::Practice, RowState::Skipped),
        ];
        let sitting = Sitting {
            taken: vec![
                Taken {
                    item: 0,
                    class: Class::Review,
                    attempt: 1,
                    outcome: Outcome::Pass,
                    ttfk_ms: Some(1_200),
                    total_ms: Some(4_100),
                    corrections: 0,
                    paste_refused: 0,
                    step_after: Step::cap(),
                },
                Taken {
                    item: 1,
                    class: Class::Practice,
                    attempt: 1,
                    outcome: Outcome::Skip,
                    ttfk_ms: None,
                    total_ms: None,
                    corrections: 0,
                    paste_refused: 0,
                    step_after: Step::cap(),
                },
            ],
            aborted: false,
            rows: Vec::new(),
        };
        let notes = vec!["escrow-p  cutover ready — 4 passes at 7d".to_owned()];
        let lines = card(
            &Frame::done(date(2026, 9, 10), &rows, &sitting, &notes),
            false,
        )
        .render();
        let text = lines.join("\n");
        assert!(text.contains("done"));
        assert!(text.contains("1 review"));
        assert!(text.contains("1 practice"));
        assert!(text.contains("0 lapses"));
        assert!(text.contains("cutover ready"));
        assert!(text.contains("press any key"));
    }

    #[test]
    fn an_empty_sitting_says_so_rather_than_drawing_an_empty_box() {
        let lines = card(&Frame::running(date(2026, 9, 10), &[], None), false).render();
        assert!(lines.iter().any(|line| line.contains("nothing to drill")));
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
        let lines = card(&Frame::running(date(2026, 9, 10), &rows, None), false).render();
        assert!(lines.iter().any(|line| line.contains('—')));
    }
}
