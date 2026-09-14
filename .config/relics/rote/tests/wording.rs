//! The wording, under snapshot.
//!
//! **Snapshots for wording, assertions for invariants.** What a card is made of
//! — one rectangle, a field that never moves, nothing typed ever reaching a
//! rendered line — is asserted in the crate, because a snapshot cannot express
//! it and accepting one would bless a regression in it. What a card and a topic
//! *say* is here, so a rewrite reviews as a diff.

use jiff::civil::date;
use relic_core::style::Style;
use rote::corpus::record::Outcome;
use rote::ladder::{Occasion, Rung};
use rote::sitting::screen::{Frame, Kind, Note, Row, RowState, card};
use rote::sitting::{Capture, Outturn};
use rote::slug::Slug;

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

fn attach(slug: Slug, state: RowState, replacing: bool) -> Row {
    Row {
        kind: Kind::Attach {
            dossier: format!("{slug}@2 · enrolled 2026-09-14 · rung 5 · reviewed 3d ago"),
            replacing,
        },
        slug,
        state,
    }
}

fn name(text: &str) -> Slug {
    text.parse()
        .unwrap_or_else(|_| unreachable!("a literal slug"))
}

fn render(rows: &[Row], active: usize, status: Option<&str>, lookup: bool) -> String {
    let mut frame = Frame::running(date(2026, 9, 13), rows, Some(active));
    frame.status = status.map(str::to_owned);
    frame.lookup = lookup;
    card(&frame, Style::PLAIN).render().join("\n")
}

#[test]
fn a_drill_card_reads_the_way_it_is_meant_to() {
    let rows = vec![
        drill(
            name("escrow-p"),
            Occasion::Review,
            false,
            RowState::Active { attempt: 1 },
        ),
        drill(
            name("op-master"),
            Occasion::Practice,
            false,
            RowState::Pending,
        ),
    ];
    insta::assert_snapshot!("drill", render(&rows, 0, None, false));
}

#[test]
fn a_refused_drill_says_which_try_this_is_and_offers_the_lookup() {
    let rows = vec![drill(
        name("escrow-p"),
        Occasion::Review,
        false,
        RowState::Failed { attempt: 2 },
    )];
    insta::assert_snapshot!(
        "drill-refused",
        render(&rows, 0, Some("not it · try 2 of 3"), true)
    );
}

#[test]
fn an_aided_drill_says_it_measures_nothing() {
    let rows = vec![drill(
        name("escrow-p"),
        Occasion::Review,
        true,
        RowState::Active { attempt: 2 },
    )];
    insta::assert_snapshot!("drill-aided", render(&rows, 0, None, false));
}

#[test]
fn an_attachment_names_what_it_is_continuing() {
    let rows = vec![attach(name("escrow-p"), RowState::Claiming, false)];
    insta::assert_snapshot!("attach-claiming", render(&rows, 0, None, false));
}

#[test]
fn the_second_half_of_an_attachment_asks_again() {
    let rows = vec![attach(name("escrow-p"), RowState::Confirming, false)];
    insta::assert_snapshot!("attach-confirming", render(&rows, 0, None, false));
}

#[test]
fn an_attachment_that_differed_says_so() {
    let rows = vec![attach(name("escrow-p"), RowState::Differed, false)];
    insta::assert_snapshot!(
        "attach-differed",
        render(&rows, 0, Some("the two entries differ"), false)
    );
}

#[test]
fn an_attachment_that_replaces_one_says_that_instead() {
    let rows = vec![attach(name("escrow-p"), RowState::Claiming, true)];
    insta::assert_snapshot!("attach-replacing", render(&rows, 0, None, false));
}

/// One sample, as the closing card saw it.
fn capture(turn: usize, aided: bool, ordinal: u8, outcome: Outcome) -> Capture {
    Capture {
        turn,
        occasion: Occasion::Review,
        aided,
        ordinal,
        outcome,
        ttfk_ms: Some(900),
        total_ms: Some(3_000),
        corrections: 0,
        paste_accepted: 0,
        paste_refused: 0,
        rung_after: Rung::FIRST,
    }
}

fn note(label: &str, said: &str) -> Note {
    Note {
        label: label.to_owned(),
        said: said.to_owned(),
    }
}

fn closing(rows: &[Row], captures: Vec<Capture>, notes: &[Note]) -> String {
    let outturn = Outturn {
        captures,
        attached: Vec::new(),
        aborted: false,
        rows: Vec::new(),
    };
    let frame = Frame::done(date(2026, 9, 13), rows, &outturn, notes);
    card(&frame, Style::PLAIN).render().join("\n")
}

#[test]
fn a_drill_recovered_with_the_vault_says_what_was_recorded() {
    // The row says the sitting got there; the tally says what went on the
    // record. Both are true and they are about different things, so the card
    // has to name which is which.
    let rows = vec![drill(
        name("warmup-p"),
        Occasion::Review,
        true,
        RowState::Passed {
            total_ms: None,
            retries: 1,
        },
    )];
    let captures = vec![
        capture(0, false, 1, Outcome::Fail),
        capture(0, true, 2, Outcome::Pass),
    ];
    insta::assert_snapshot!("closing-aided", closing(&rows, captures, &[]));
}

#[test]
fn a_closing_note_wraps_under_its_label_rather_than_losing_its_tail() {
    let rows = vec![drill(
        name("warmup-p"),
        Occasion::Review,
        true,
        RowState::Passed {
            total_ms: None,
            retries: 1,
        },
    )];
    let captures = vec![capture(0, true, 1, Outcome::Fail)];
    let notes = vec![note(
        "warmup-p@1",
        "an aided capture was refused — the vault and the verifier hold \
         different secrets. Confirm which one is current, then rote rotate",
    )];
    insta::assert_snapshot!("closing-drifted", closing(&rows, captures, &notes));
}

#[test]
fn the_widest_sitting_the_binary_can_produce_still_fits_the_box() {
    // Every bucket at once, under the longest slug a lineage may have, with
    // both closing notes. Nothing here is a plausible sitting; it is the
    // envelope, and the render choke point asserts on every line of it.
    let long = "a".repeat(rote::slug::MAX);
    let states = [
        RowState::Passed {
            total_ms: Some(3_000),
            retries: 4,
        },
        RowState::Failed { attempt: 3 },
        RowState::Attached,
        RowState::Differed,
        RowState::Skipped,
        RowState::Aborted,
    ];
    let rows: Vec<Row> = states
        .into_iter()
        .map(|state| drill(name(&long), Occasion::Review, false, state))
        .collect();
    let captures: Vec<Capture> = [
        Outcome::Pass,
        Outcome::Fail,
        Outcome::Blank,
        Outcome::Skip,
        Outcome::Abort,
    ]
    .into_iter()
    .enumerate()
    .map(|(turn, outcome)| capture(turn, false, 1, outcome))
    .collect();
    let label = format!("{long}@12");
    let notes = vec![
        note(
            &label,
            "attached — rote took your word for it, and checked nothing",
        ),
        note(
            &label,
            "an aided capture was refused — the vault and the verifier hold \
             different secrets. Confirm which one is current, then rote rotate",
        ),
    ];
    for line in closing(&rows, captures, &notes).lines() {
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
