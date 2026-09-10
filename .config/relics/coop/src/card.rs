//! The card, as a pure function from notices to lines.
//!
//! Nothing here reads the clock, the filesystem or the terminal. Width and
//! colour arrive as arguments, which is what lets every shape the card can take
//! be a test rather than a screenshot.

use relic_core::finding::Severity;

use crate::config::Style;
use crate::notice::Notice;

const RESET: &str = "\x1b[0m";
const DIM: &str = "\x1b[2m";
const YELLOW: &str = "\x1b[33m";
const RED: &str = "\x1b[1;31m";

/// The title, which is also the narrowest a header can be.
const TITLE: &str = "coop";
/// One space of padding inside each vertical border.
const PADDING: usize = 1;
/// Between the source column and what it has to say.
const GAP: &str = "  ";

struct Glyphs {
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    horizontal: char,
    vertical: char,
}

impl Glyphs {
    fn of(style: Style) -> Self {
        match style {
            Style::Unicode => Self {
                top_left: '╭',
                top_right: '╮',
                bottom_left: '╰',
                bottom_right: '╯',
                horizontal: '─',
                vertical: '│',
            },
            Style::Ascii => Self {
                top_left: '+',
                top_right: '+',
                bottom_left: '+',
                bottom_right: '+',
                horizontal: '-',
                vertical: '|',
            },
        }
    }
}

/// Draw the card, or nothing at all when there is nothing to say.
///
/// An empty coop draws no box. The intended state of this inbox is empty, and a
/// box announcing that would be the first piece of furniture in it.
pub fn draw(notices: &[Notice], width: usize, style: Style, color: bool) -> Option<String> {
    if notices.is_empty() {
        return None;
    }
    let glyphs = Glyphs::of(style);
    let inner = width.saturating_sub(2 + PADDING * 2);
    let column = notices
        .iter()
        .map(|notice| notice.source.chars().count())
        .max()
        .unwrap_or(0);

    let mut lines = vec![header(&glyphs, width)];
    for notice in notices {
        lines.push(row(notice, &glyphs, inner, column, color));
    }
    lines.push(footer(&glyphs, width));
    Some(lines.join("\n"))
}

/// The prompt segment: how many, or nothing at all.
pub fn badge(notices: &[Notice]) -> Option<String> {
    (!notices.is_empty()).then(|| notices.len().to_string())
}

fn header(glyphs: &Glyphs, width: usize) -> String {
    // left corner, one rule, a space, the title, a space
    let used = 1 + 1 + 1 + TITLE.chars().count() + 1;
    let fill = width.saturating_sub(used + 1);
    format!(
        "{}{} {TITLE} {}{}",
        glyphs.top_left,
        glyphs.horizontal,
        repeat(glyphs.horizontal, fill),
        glyphs.top_right
    )
}

fn footer(glyphs: &Glyphs, width: usize) -> String {
    format!(
        "{}{}{}",
        glyphs.bottom_left,
        repeat(glyphs.horizontal, width.saturating_sub(2)),
        glyphs.bottom_right
    )
}

fn row(notice: &Notice, glyphs: &Glyphs, inner: usize, column: usize, color: bool) -> String {
    let id = pad(&notice.source, column);
    let summary = notice.finding.summary.as_str();
    let tail = notice.distinct_fix().map(|fix| format!(" — {fix}"));

    let fixed = column + GAP.chars().count();
    let room = inner.saturating_sub(fixed);
    // The fix hint is the first thing to go: it is a shorthand for something the
    // summary has already named, and losing the summary loses the notice.
    let tail = tail.filter(|tail| summary.chars().count() + tail.chars().count() <= room);
    let tail_width = tail.as_ref().map_or(0, |tail| tail.chars().count());
    let summary = clip(summary, room.saturating_sub(tail_width));

    let visible = fixed + summary.chars().count() + tail_width;
    let slack = inner.saturating_sub(visible);

    let body = if color {
        format!(
            "{DIM}{id}{RESET}{GAP}{}{summary}{RESET}{}",
            severity_color(notice.finding.severity),
            tail.map(|tail| format!("{DIM}{tail}{RESET}"))
                .unwrap_or_default()
        )
    } else {
        format!("{id}{GAP}{summary}{}", tail.unwrap_or_default())
    };

    format!(
        "{} {body}{} {}",
        glyphs.vertical,
        " ".repeat(slack),
        glyphs.vertical
    )
}

fn severity_color(severity: Severity) -> &'static str {
    match severity {
        Severity::Broken => RED,
        // A note never reaches the card, but a colour table with a hole in it is
        // how a later severity arrives invisible.
        Severity::Soft | Severity::Note => YELLOW,
    }
}

fn pad(text: &str, width: usize) -> String {
    let count = text.chars().count();
    format!("{text}{}", " ".repeat(width.saturating_sub(count)))
}

fn clip(text: &str, room: usize) -> String {
    if text.chars().count() <= room {
        return text.to_owned();
    }
    if room <= 1 {
        return "…".chars().take(room).collect();
    }
    let kept: String = text.chars().take(room.saturating_sub(1)).collect();
    format!("{}…", kept.trim_end())
}

fn repeat(glyph: char, count: usize) -> String {
    std::iter::repeat_n(glyph, count).collect()
}

#[cfg(test)]
mod tests {
    use super::{badge, draw};
    use crate::config::Style;
    use crate::notice::Notice;
    use relic_core::finding::{FixHint, Severity, StationId, Summary};

    fn notice(source: &str, severity: Severity, summary: &str, fix: Option<&str>) -> Notice {
        let station = StationId::from_static("test");
        let mut finding = station.finds(severity, Summary::lossy(summary));
        if let Some(fix) = fix {
            finding = finding.fixed_by(FixHint::lossy(fix));
        }
        Notice {
            source: source.to_owned(),
            finding,
        }
    }

    fn pair() -> Vec<Notice> {
        vec![
            notice(
                "up",
                Severity::Soft,
                "3d since the last system update",
                Some("up"),
            ),
            notice("rote", Severity::Soft, "2 drills due", Some("rote")),
        ]
    }

    fn widths(card: &str) -> Vec<usize> {
        card.lines().map(|line| line.chars().count()).collect()
    }

    #[test]
    fn an_empty_coop_draws_nothing_at_all() {
        assert_eq!(draw(&[], 60, Style::Unicode, false), None);
        assert_eq!(badge(&[]), None);
    }

    #[test]
    fn every_line_is_exactly_the_asked_width() {
        for width in [28, 40, 60, 80] {
            let card = draw(&pair(), width, Style::Unicode, false).unwrap();
            for measured in widths(&card) {
                assert_eq!(measured, width, "at width {width}\n{card}");
            }
        }
    }

    #[test]
    fn colour_does_not_change_the_shape() {
        let plain = draw(&pair(), 60, Style::Unicode, false).unwrap();
        let painted = draw(&pair(), 60, Style::Unicode, true).unwrap();
        assert_eq!(plain.lines().count(), painted.lines().count());
        assert!(painted.contains("\x1b[33m"));
        assert!(!plain.contains('\x1b'));
    }

    #[test]
    fn ascii_is_the_same_card_in_characters_that_always_arrive() {
        let card = draw(&pair(), 50, Style::Ascii, false).unwrap();
        assert!(card.starts_with("+- coop "));
        assert!(!card.contains('│'));
        for measured in widths(&card) {
            assert_eq!(measured, 50);
        }
    }

    #[test]
    fn a_fix_that_only_repeats_the_source_is_not_drawn() {
        let card = draw(&pair(), 60, Style::Unicode, false).unwrap();
        assert!(!card.contains("— up"), "{card}");
    }

    #[test]
    fn a_fix_worth_saying_is_drawn() {
        let one = vec![notice(
            "rote",
            Severity::Soft,
            "2 drills due",
            Some("rote drill"),
        )];
        let card = draw(&one, 60, Style::Unicode, false).unwrap();
        assert!(card.contains("— rote drill"), "{card}");
    }

    #[test]
    fn a_narrow_card_drops_the_hint_before_it_drops_the_notice() {
        let one = vec![notice(
            "rote",
            Severity::Soft,
            "2 drills due",
            Some("rote drill"),
        )];
        let card = draw(&one, 28, Style::Unicode, false).unwrap();
        assert!(card.contains("2 drills due"), "{card}");
        assert!(!card.contains("rote drill"), "{card}");
    }

    #[test]
    fn an_overlong_summary_is_clipped_rather_than_wrapped() {
        let one = vec![notice(
            "x",
            Severity::Soft,
            "a summary far longer than any card could ever hope to hold in one row",
            None,
        )];
        let card = draw(&one, 40, Style::Unicode, false).unwrap();
        assert_eq!(card.lines().count(), 3, "one row, not several\n{card}");
        assert!(card.contains('…'), "{card}");
        for measured in widths(&card) {
            assert_eq!(measured, 40);
        }
    }

    #[test]
    fn the_source_column_lines_up_across_rows() {
        let card = draw(&pair(), 60, Style::Unicode, false).unwrap();
        let rows: Vec<&str> = card.lines().skip(1).take(2).collect();
        let positions: Vec<Option<usize>> = rows
            .iter()
            .map(|row| row.find(char::is_alphabetic))
            .collect();
        assert_eq!(positions.first(), positions.last());
    }

    #[test]
    fn broken_is_painted_differently_from_soft() {
        let one = vec![notice("x", Severity::Broken, "it is broken", None)];
        let card = draw(&one, 40, Style::Unicode, true).unwrap();
        assert!(card.contains("\x1b[1;31m"), "{card}");
    }

    #[test]
    fn the_badge_is_a_count() {
        assert_eq!(badge(&pair()).as_deref(), Some("2"));
    }
}
