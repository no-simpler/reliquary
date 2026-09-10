//! The card, as a value.
//!
//! Rendering is a pure function from a frame to lines, so the whole visual
//! design is under snapshot test and the terminal adapter only has to paint what
//! it is handed. Colour is resolved once, by the caller, and passed in.

use jiff::civil::Date;

use super::{Item, Sitting};
use crate::ladder::{Class, Step};
use crate::slug::Slug;

/// Narrowest card. Wider than this only when the content asks for it.
const MIN_WIDTH: usize = 52;

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

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";

/// A piece of text, and the same text with colour on it.
struct Piece {
    plain: String,
    styled: String,
}

impl Piece {
    fn plain(text: impl Into<String>) -> Self {
        let text = text.into();
        Self {
            styled: text.clone(),
            plain: text,
        }
    }

    fn painted(text: impl Into<String>, code: &str, color: bool) -> Self {
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

    fn width(&self) -> usize {
        self.plain.chars().count()
    }
}

fn join(pieces: &[Piece]) -> Piece {
    Piece {
        plain: pieces.iter().map(|p| p.plain.as_str()).collect(),
        styled: pieces.iter().map(|p| p.styled.as_str()).collect(),
    }
}

/// Draw the card.
pub fn render(frame: &Frame<'_>, color: bool) -> Vec<String> {
    let mut body: Vec<Piece> = Vec::new();
    let names = frame
        .rows
        .iter()
        .map(|row| row.slug.as_str().chars().count())
        .max()
        .unwrap_or(0);

    if frame.rows.is_empty() {
        body.push(Piece::painted("nothing to drill", DIM, color));
    }

    for (index, row) in frame.rows.iter().enumerate() {
        body.push(row_line(row, names, color));
        if frame.active == Some(index) {
            if let Some(text) = intention(row) {
                body.push(Piece::painted(text, DIM, color));
            }
            body.push(prompt_line(row, color));
        }
    }

    match frame.notice {
        Some(Notice::PasteRefused) => {
            body.push(Piece::plain(""));
            body.push(Piece::painted(
                "a paste was refused — the drill has to be typed",
                YELLOW,
                color,
            ));
        }
        Some(Notice::Choose { retry }) => {
            body.push(Piece::plain(""));
            let text = if retry {
                "[enter] try again from memory   [l] look it up, then type it   [s] move on"
            } else {
                "[l] look it up, then type it   [enter] move on"
            };
            body.push(Piece::painted(text, YELLOW, color));
        }
        None => {}
    }

    if let Some(sitting) = frame.sitting {
        body.push(Piece::plain(""));
        body.push(Piece::painted(tally(sitting), BOLD, color));
        for note in frame.notes {
            body.push(Piece::painted(note.clone(), DIM, color));
        }
        body.push(Piece::plain(""));
        body.push(Piece::painted("press any key", DIM, color));
    }

    let title = if frame.sitting.is_some() {
        "done"
    } else {
        "rote"
    };
    let stamp = frame.today.strftime("%a %-d %b").to_string();
    let width = body
        .iter()
        .map(Piece::width)
        .max()
        .unwrap_or(0)
        .saturating_add(8)
        .max(MIN_WIDTH)
        .max(title.chars().count() + stamp.chars().count() + 8);

    let mut lines = Vec::with_capacity(body.len().saturating_add(4));
    lines.push(top(title, &stamp, width, color));
    lines.push(edge("", 0, width, color));
    for piece in &body {
        lines.push(edge(&piece.styled, piece.width(), width, color));
    }
    lines.push(edge("", 0, width, color));
    lines.push(bottom(width, color));
    lines
}

fn row_line(row: &Row, names: usize, color: bool) -> Piece {
    let name = format!(
        "{:<width$}",
        row.slug.as_str(),
        width = names.saturating_add(2)
    );
    let mut pieces = vec![Piece::painted(name, BOLD, color)];
    pieces.push(Piece::painted(chip(row), DIM, color));
    pieces.push(Piece::plain("  "));
    pieces.push(state_piece(row, color));
    join(&pieces)
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
    let attempt = match row.state {
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
    join(&[
        Piece::painted("▸ ", DIM, color),
        Piece::painted("▉", BOLD, color),
        Piece::painted(attempt, DIM, color),
    ])
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

fn top(title: &str, stamp: &str, width: usize, color: bool) -> String {
    let left = format!("┌─ {title} ");
    let right = format!(" {stamp} ─┐");
    let fill = width
        .saturating_sub(left.chars().count())
        .saturating_sub(right.chars().count());
    let line = format!("{left}{}{right}", "─".repeat(fill));
    paint(&line, DIM, color)
}

fn bottom(width: usize, color: bool) -> String {
    let line = format!("└{}┘", "─".repeat(width.saturating_sub(2)));
    paint(&line, DIM, color)
}

fn edge(content: &str, visible: usize, width: usize, color: bool) -> String {
    let pad = width.saturating_sub(visible).saturating_sub(8);
    format!(
        "{}   {content}{}   {}",
        paint("│", DIM, color),
        " ".repeat(pad),
        paint("│", DIM, color)
    )
}

fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("{code}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;

    use super::{Frame, Notice, Row, RowState, render};
    use crate::drill::{Sitting, Taken};
    use crate::ladder::{Class, Step};
    use crate::log::Outcome;

    fn row(name: &str, class: Class, state: RowState) -> Row {
        Row {
            slug: name.parse().unwrap(),
            class,
            step: Step::cap(),
            stretch: false,
            state,
        }
    }

    fn width_of(lines: &[String]) -> usize {
        lines
            .iter()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0)
    }

    #[test]
    fn every_line_of_a_card_is_the_same_width() {
        let rows = vec![
            row("escrow-p", Class::Review, RowState::Pending),
            row("flagship-login", Class::Practice, RowState::Pending),
        ];
        let lines = render(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false);
        let width = width_of(&lines);
        for line in &lines {
            assert_eq!(line.chars().count(), width, "{line}");
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
        let lines = render(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false);
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
        let lines = render(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false);
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
        let lines = render(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false);
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
        let text = render(&frame, false).join("\n");
        assert!(text.contains("[l] look it up"), "{text}");
        assert!(!text.contains("try again from memory"), "{text}");

        frame.notice = Some(Notice::Choose { retry: true });
        assert!(
            render(&frame, false)
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
        let text = render(&Frame::running(date(2026, 9, 10), &rows, None), false).join("\n");
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
        let plain = render(&frame, false);
        let painted = render(&frame, true);
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
        let lines = render(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false);
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
        let lines = render(&Frame::running(date(2026, 9, 10), &rows, Some(0)), false);
        assert!(lines.iter().any(|line| line.contains("try 2")));
    }

    #[test]
    fn a_refused_paste_says_so_on_the_card() {
        let rows = vec![row("a", Class::Review, RowState::Pending)];
        let mut frame = Frame::running(date(2026, 9, 10), &rows, Some(0));
        frame.notice = Some(Notice::PasteRefused);
        let lines = render(&frame, false);
        assert!(lines.iter().any(|line| line.contains("paste was refused")));
    }

    #[test]
    fn a_slug_with_no_verifier_says_what_to_do_about_it() {
        let rows = vec![row("a", Class::Review, RowState::Unverifiable)];
        let lines = render(&Frame::running(date(2026, 9, 10), &rows, None), false);
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
        let lines = render(
            &Frame::done(date(2026, 9, 10), &rows, &sitting, &notes),
            false,
        );
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
        let lines = render(&Frame::running(date(2026, 9, 10), &[], None), false);
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
        let lines = render(&Frame::running(date(2026, 9, 10), &rows, None), false);
        assert!(lines.iter().any(|line| line.contains('—')));
    }
}
