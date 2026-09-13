//! The wording, under snapshot.
//!
//! **Snapshots for wording, assertions for invariants.** What a card is made of
//! — one rectangle, a field that never moves, nothing typed ever reaching a
//! rendered line — is asserted in the crate, because a snapshot cannot express
//! it and accepting one would bless a regression in it. What a card and a topic
//! *say* is here, so a rewrite reviews as a diff.

use jiff::civil::date;
use relic_core::style::Style;
use rote::ladder::Occasion;
use rote::sitting::screen::{Frame, Kind, Row, RowState, card};
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
