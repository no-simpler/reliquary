//! The sitting: the daily call, and drilling by name.
//!
//! **Bare `rote` asks for what is due and nothing else.** One lineage due out of
//! ten means one turn. If nothing is due it says so and offers to drill
//! everything anyway, on one keystroke, rather than deciding on your behalf
//! what to add: deciding what to add beyond what is due is the schedule
//! deciding again.

use anyhow::Result;
use jiff::civil::Date;

use super::{Context, open_store, save_verifiers};
use crate::cli::DrillArgs;
use crate::corpus::Corpus;
use crate::corpus::record::{Drilled, Event};
use crate::exit::{CLEAN, FAIL, INCOMPLETE};
use crate::intake::refreshed;
use crate::ladder::{Ladder, Standing};
use crate::sitting::{self, Selection, Turn, screen};
use crate::store::Store;
use crate::tui::{Screen as _, term};
use crate::verifier::file::Verifiers;

/// What bare `rote` and `rote drill` say when there is no lineage to ask
/// about.
const NOTHING_TO_DRILL: &str = "nothing to drill · rote enroll <name> to start one";

/// Bare `rote`.
///
/// # Errors
///
/// When the corpus cannot be read or written, or the terminal refuses.
pub fn daily(ctx: &Context) -> Result<u8> {
    let mut store = open_store(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(store.chains().records(), &ladder);
    let verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    let today = ctx.today();

    let due = sitting::roster(&corpus, &verifiers, today, &ladder, Selection::Due);
    if !due.is_empty() {
        return work(
            ctx,
            &mut store,
            &Opening {
                corpus: &corpus,
                verifiers: &verifiers,
                today,
            },
            &due,
            None,
        );
    }

    if corpus.active().is_empty() {
        if !ctx.quiet {
            println!("{NOTHING_TO_DRILL}");
        }
        return Ok(CLEAN);
    }

    let waiting = nothing_due(&corpus, today, &ladder);
    // Not a refusal: with nothing due there is nothing to insist on, so a run
    // that cannot open a dialog — no terminal, or one too small to hold a
    // card — says what is waiting and leaves clean.
    if !term::is_interactive() || !term::roomy() {
        if !ctx.quiet {
            println!("{waiting}");
        }
        return Ok(CLEAN);
    }

    // Nothing is due, so the only thing left to offer is everything. Asked
    // rather than assumed: a sitting nobody asked for is the schedule deciding
    // again.
    let mut console = term::Terminal::enter(ctx.style)?;
    let wanted = crate::tui::offer(&mut console, today, &waiting, "drill everything anyway?")?;
    if !wanted {
        return Ok(CLEAN);
    }
    let roster = sitting::roster(&corpus, &verifiers, today, &ladder, Selection::Named(&[]));
    work(
        ctx,
        &mut store,
        &Opening {
            corpus: &corpus,
            verifiers: &verifiers,
            today,
        },
        &roster,
        Some(console),
    )
}

/// `rote drill`.
///
/// # Errors
///
/// When a named lineage is unknown, or the corpus cannot be read or written.
pub fn drill(ctx: &Context, args: &DrillArgs) -> Result<u8> {
    let mut store = open_store(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(store.chains().records(), &ladder);
    let verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    let today = ctx.today();

    for slug in &args.lineages {
        if corpus.lineage(slug).is_none() {
            anyhow::bail!("there is no lineage called {slug}");
        }
    }

    let roster = sitting::roster(
        &corpus,
        &verifiers,
        today,
        &ladder,
        Selection::Named(&args.lineages),
    );
    if roster.is_empty() {
        if !ctx.quiet {
            println!("{NOTHING_TO_DRILL}");
        }
        return Ok(CLEAN);
    }
    work(
        ctx,
        &mut store,
        &Opening {
            corpus: &corpus,
            verifiers: &verifiers,
            today,
        },
        &roster,
        None,
    )
}

/// What a sitting starts from: the record and the day, as they stood before it.
struct Opening<'a> {
    /// The corpus as it was, which is what the closing card names rows by.
    corpus: &'a Corpus,
    /// What this machine holds.
    verifiers: &'a Verifiers,
    /// The drill day.
    today: Date,
}

/// Run one roster to the end.
fn work(
    ctx: &Context,
    store: &mut Store,
    opening: &Opening<'_>,
    turns: &[Turn],
    open: Option<term::Terminal>,
) -> Result<u8> {
    let Opening {
        verifiers, today, ..
    } = *opening;
    let mut console = match open {
        Some(console) => console,
        None => term::Terminal::enter(ctx.style)?,
    };

    // Two closures reach the same chain and the same verifier file, so each
    // handle is shared rather than borrowed twice. The cells are local to the
    // sitting and never escape it.
    let store = std::cell::RefCell::new(store);
    let held = std::cell::RefCell::new(verifiers.clone());

    // Every record carries the instant it was written, not the instant the
    // sitting opened: the clock is read once, at the process boundary, and
    // each write adds what the monotonic clock has counted since.
    let opened = std::time::Instant::now();
    let now = ctx.clock.now();
    let at = move || {
        jiff::SignedDuration::try_from(opened.elapsed())
            .ok()
            .and_then(|elapsed| now.checked_add(elapsed).ok())
            .unwrap_or(now)
    };

    // A secret proved against a verifier below the cost floor is re-minted at
    // the floor while it is in hand, which is the one moment that costs
    // nothing to ask for.
    let verify =
        |index: usize, verifier: &crate::verifier::Verifier, secret: &crate::secret::Secret| {
            let accepted = verifier.accepts(secret)?;
            if accepted
                && let Some(turn) = turns.get(index)
                && let Some(fresh) = refreshed(verifier, secret)?
            {
                let mut held = held.borrow_mut();
                held.set(turn.engram, &turn.slug, today, &fresh);
                save_verifiers(ctx, &held)?;
            }
            Ok(accepted)
        };

    let outturn = {
        // Each drill goes to the corpus the moment it ends, so a sitting cut
        // short keeps every drill that had already ended.
        let mut record = |_: usize, drilled: &Drilled| -> Result<()> {
            store
                .borrow_mut()
                .append(at(), today, Event::Drill(drilled.clone()))
        };

        sitting::run(
            turns,
            today,
            sitting::Wiring {
                console: &mut console,
                verify: &verify,
                record: &mut record,
            },
        )?
    };

    close(ctx, opening, turns, &outturn, console)
}

/// The closing card, held until it has been read; then the exit status.
fn close(
    ctx: &Context,
    opening: &Opening<'_>,
    turns: &[Turn],
    outturn: &sitting::Outturn,
    mut console: term::Terminal,
) -> Result<u8> {
    let notes = closing_notes(turns, opening.corpus);
    let closing = screen::card(
        &screen::Frame::done(opening.today, &outturn.rows, outturn, &notes),
        &crate::tui::card::Field::blind(),
        ctx.style,
    );
    console.paint(&closing)?;
    console.hold()?;
    drop(console);

    Ok(if outturn.aborted {
        INCOMPLETE
    } else if outturn.failed() > 0 {
        FAIL
    } else {
        CLEAN
    })
}

/// What belongs under the closing card: the verb for each row the sitting
/// could not ask about. A dormant row is named on the day it is skipped,
/// because that is the day the person is looking.
fn closing_notes(turns: &[Turn], corpus: &Corpus) -> Vec<screen::Note> {
    turns
        .iter()
        .filter(|turn| turn.dormant())
        .map(|turn| screen::Note {
            label: corpus.label(&turn.engram),
            said: format!("dormant here — rote attach {}", turn.slug),
        })
        .collect()
}

fn nothing_due(corpus: &Corpus, today: Date, ladder: &Ladder) -> String {
    let next = corpus
        .active()
        .into_iter()
        .filter_map(|lineage| lineage.current())
        .filter_map(|dossier| match dossier.standing(today, ladder) {
            Standing::Waiting { until } => Some(until),
            Standing::Due => None,
        })
        .min();
    match next {
        Some(day) => format!(
            "nothing due · next in {}",
            relic_core::fmt::plural(
                usize::try_from(crate::ladder::days_between(today, day)).unwrap_or(0),
                "day",
                "days"
            )
        ),
        None => "nothing due".to_owned(),
    }
}
