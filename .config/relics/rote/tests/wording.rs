//! The wording, under snapshot.
//!
//! **Snapshots for wording, assertions for invariants.** What a card is made of
//! — one rectangle, a field that never moves, nothing typed ever reaching a
//! rendered line — is asserted in the crate, because a snapshot cannot express
//! it and accepting one would bless a regression in it. What a card and a topic
//! *say* is here, so a rewrite reviews as a diff.

use jiff::civil::date;
use relic_core::style::Style;
use rote::corpus::record::{Drilled, EngramId, Outcome};
use rote::ladder::Occasion;
use rote::sitting::Outturn;
use rote::sitting::screen::{Frame, Kind, Note, Row, RowState, card};
use rote::slug::Slug;
use rote::tui::card::{Field, Reveal, Tone};
use rote::tui::{Ask, Refusal, ask_card};

fn drill(slug: Slug, occasion: Occasion, aided: bool, state: RowState) -> Row {
    Row {
        slug,
        kind: Kind::Drill {
            occasion,
            aided,
            interval_days: 30,
            at_cap: true,
        },
        state,
    }
}

fn dormant(slug: Slug) -> Row {
    Row {
        slug,
        kind: Kind::Dormant,
        state: RowState::Dormant,
    }
}

fn name(text: &str) -> Slug {
    text.parse()
        .unwrap_or_else(|_| unreachable!("a literal slug"))
}

/// One frame as a person actually meets it, which means under the field that
/// prompt actually draws: hidden where a memory is being measured, masked
/// everywhere else.
fn render(rows: &[Row], active: usize, status: Option<&str>, lookup: bool, cold: bool) -> String {
    field_render(rows, active, status, lookup, &Field::resting(cold))
}

fn field_render(
    rows: &[Row],
    active: usize,
    status: Option<&str>,
    lookup: bool,
    field: &Field<'_>,
) -> String {
    let mut frame = Frame::running(date(2026, 9, 13), rows, Some(active));
    frame.status = status.map(str::to_owned);
    frame.lookup = lookup;
    card(&frame, field, Style::PLAIN).render().join("\n")
}

#[test]
fn a_drill_card_reads_the_way_it_is_meant_to() {
    let rows = vec![
        drill(name("escrow-p"), Occasion::Review, false, RowState::Cold),
        drill(
            name("op-master"),
            Occasion::Practice,
            false,
            RowState::Pending,
        ),
    ];
    insta::assert_snapshot!("drill", render(&rows, 0, None, false, true));
}

#[test]
fn a_follow_up_says_which_one_it_is_and_offers_the_lookup() {
    let rows = vec![drill(
        name("escrow-p"),
        Occasion::Review,
        false,
        RowState::FollowUp { count: 2 },
    )];
    insta::assert_snapshot!(
        "drill-follow-up",
        render(&rows, 0, Some("not it · follow-up 2"), true, false)
    );
}

#[test]
fn an_aided_follow_up_says_the_answer_was_looked_up_and_withdraws_the_lookup() {
    let rows = vec![drill(
        name("escrow-p"),
        Occasion::Review,
        true,
        RowState::FollowUp { count: 2 },
    )];
    insta::assert_snapshot!(
        "drill-aided",
        render(&rows, 0, Some("not it · follow-up 2"), false, false)
    );
}

/// The one card every intake draws, in the state asked for.
fn intake(intention: &str, status: Option<&str>, field: &Field<'_>) -> String {
    let prompt = Ask {
        title: "enroll",
        resting: Tone::Calm,
        heading: "escrow-p",
        intention,
        status,
    };
    ask_card(
        &prompt,
        date(2026, 9, 13),
        Style::PLAIN,
        Refusal::None,
        Tone::Calm,
        field,
    )
    .render()
    .join("\n")
}

#[test]
fn an_intake_says_what_it_is_for_and_offers_the_reveal() {
    insta::assert_snapshot!(
        "intake-first",
        intake(
            rote::intake::ENROLL_INTENTION,
            None,
            &Field::empty(Reveal::Masked)
        )
    );
}

#[test]
fn the_confirmation_half_of_an_intake_asks_again_and_nothing_else() {
    insta::assert_snapshot!(
        "intake-again",
        intake(rote::intake::AGAIN, None, &Field::empty(Reveal::Masked))
    );
}

#[test]
fn a_revealed_field_shows_the_characters_and_says_how_to_put_them_back() {
    // The one moment this tool puts a secret on a screen. Typed, not pasted, so
    // the snapshot says what a person sees at the moment they ask to see it.
    let shown = "correct horse battery staple";
    insta::assert_snapshot!(
        "intake-revealed",
        intake(
            rote::intake::ENROLL_INTENTION,
            None,
            &Field {
                reveal: Reveal::Shown,
                drawn: shown,
                clipped: (false, false),
                column: shown.chars().count(),
            },
        )
    );
}

/// One drill, as the closing card saw it.
fn drilled(outcome: Outcome, recovered: bool, aided: bool) -> Drilled {
    Drilled {
        engram: EngramId::mint().unwrap_or_else(|_| unreachable!("randomness")),
        outcome,
        ttfk_ms: Some(900),
        follow_ups: u16::from(recovered),
        recovered,
        aided,
    }
}

fn note(label: &str, said: &str) -> Note {
    Note {
        label: label.to_owned(),
        said: said.to_owned(),
    }
}

fn closing(
    rows: &[Row],
    drills: Vec<(usize, Drilled)>,
    dormant: Vec<usize>,
    notes: &[Note],
) -> String {
    let outturn = Outturn {
        drills,
        dormant,
        aborted: false,
        rows: Vec::new(),
    };
    let frame = Frame::done(date(2026, 9, 13), rows, &outturn, notes);
    card(&frame, &Field::hidden(), Style::PLAIN)
        .render()
        .join("\n")
}

#[test]
fn a_drill_recovered_in_a_follow_up_says_what_was_recorded() {
    // The row says the sitting got there; the tally says what went on the
    // record. Both are true and they are about different things, so the card
    // has to name which is which.
    let rows = vec![drill(
        name("warmup-p"),
        Occasion::Review,
        true,
        RowState::Passed {
            ttfk_ms: Some(900),
            recovered: true,
        },
    )];
    let drills = vec![(0, drilled(Outcome::Fail, true, true))];
    insta::assert_snapshot!("closing-recovered", closing(&rows, drills, Vec::new(), &[]));
}

#[test]
fn a_dormant_row_is_named_on_the_way_out_with_the_verb_that_puts_it_back() {
    let rows = vec![
        drill(
            name("escrow-p"),
            Occasion::Review,
            false,
            RowState::Passed {
                ttfk_ms: Some(900),
                recovered: false,
            },
        ),
        dormant(name("warmup-p")),
    ];
    let drills = vec![(0, drilled(Outcome::Pass, false, false))];
    let notes = vec![note("warmup-p@1", "dormant here — rote attach warmup-p")];
    insta::assert_snapshot!("closing-dormant", closing(&rows, drills, vec![1], &notes));
}

#[test]
fn the_widest_sitting_the_binary_can_produce_still_fits_the_box() {
    // Every bucket at once, under the longest slug a lineage may have, with
    // a closing note. Nothing here is a plausible sitting; it is the envelope,
    // and the render choke point asserts on every line of it.
    let long = "a".repeat(rote::slug::MAX);
    let states = [
        RowState::Passed {
            ttfk_ms: Some(3_000),
            recovered: false,
        },
        RowState::Passed {
            ttfk_ms: Some(3_000),
            recovered: true,
        },
        RowState::Failed { follow_ups: 300 },
        RowState::Skipped,
        RowState::Aborted,
    ];
    let mut rows: Vec<Row> = states
        .into_iter()
        .map(|state| drill(name(&long), Occasion::Review, true, state))
        .collect();
    rows.push(dormant(name(&long)));
    let drills = vec![
        (0, drilled(Outcome::Pass, false, false)),
        (1, drilled(Outcome::Fail, true, true)),
        (2, drilled(Outcome::Fail, false, true)),
    ];
    let label = format!("{long}@12");
    let notes = vec![note(&label, &format!("dormant here — rote attach {long}"))];
    for line in closing(&rows, drills, vec![5], &notes).lines() {
        assert_eq!(line.chars().count(), rote::tui::card::WIDTH, "{line}");
        assert!(!line.contains('…'), "{line}");
    }
}

#[test]
fn every_reference_topic_reads_the_way_it_is_meant_to() {
    for (name, body) in rote::help::TOPICS {
        insta::assert_snapshot!(format!("help-{name}"), body);
    }
}

#[test]
fn every_doctrine_topic_reads_the_way_it_is_meant_to() {
    for (name, body) in rote::guide::TOPICS {
        insta::assert_snapshot!(format!("guide-{name}"), body);
    }
}

#[test]
fn the_command_line_reads_the_way_it_is_meant_to() {
    use clap::CommandFactory as _;
    let help = rote::cli::Cli::command().render_long_help().to_string();
    insta::assert_snapshot!("help-root", help);
}

#[test]
fn an_em_dash_is_spaced_on_both_sides_wherever_one_is_written() {
    // The convention is already uniform across the crate. This is what keeps it
    // that way: it is the kind of thing an edit breaks in one place and nobody
    // sees again, and a dash that closes up against the word after it reads as
    // a hyphen joining two things that are not joined.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut checked = 0usize;
    let mut offences = Vec::new();
    let mut stack = vec![root];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("the source tree").flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            if path.extension().is_none_or(|kind| kind != "rs") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("a source file");
            for (number, line) in text.lines().enumerate() {
                let glyphs: Vec<char> = line.chars().collect();
                for (index, glyph) in glyphs.iter().enumerate() {
                    if *glyph != '—' {
                        continue;
                    }
                    checked += 1;
                    let before = index.checked_sub(1).and_then(|at| glyphs.get(at));
                    let after = glyphs.get(index + 1);
                    // A dash that is the whole of a string literal is the
                    // other idiom entirely: the placeholder for a quantity
                    // there is no value for. It is bounded by its quotes.
                    if before == Some(&'"') && after == Some(&'"') {
                        continue;
                    }
                    let spaced = |side: Option<&char>| side.is_none_or(|c| *c == ' ');
                    if !spaced(before) || !spaced(after) {
                        offences.push(format!("{}:{}: {line}", path.display(), number + 1));
                    }
                }
            }
        }
    }
    assert!(
        checked > 40,
        "the scan found almost nothing, so it is broken"
    );
    assert!(offences.is_empty(), "{}", offences.join("\n"));
}
