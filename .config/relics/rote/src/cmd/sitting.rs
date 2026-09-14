//! The sitting: the daily call, and voluntary practice.
//!
//! **Bare `rote` asks for what is due and nothing else.** One lineage due out of
//! ten means one turn. If nothing is due it says so and offers practice on one
//! keystroke, rather than deciding on your behalf what to add — which is what
//! the filler policy used to do, and what it was removed for.

use anyhow::Result;
use jiff::civil::Date;

use super::{Context, open_store, write_cache};
use crate::cli::PracticeArgs;
use crate::corpus::Corpus;
use crate::corpus::record::{Captured, Event, SittingId};
use crate::exit::{CLEAN, INCOMPLETE, LAPSE};
use crate::intake::make_verifier;
use crate::ladder::{Ladder, Standing};
use crate::sitting::{self, Mode, Turn, screen};
use crate::store::Store;
use crate::tui::{Screen as _, term};
use crate::verifier::file::Verifiers;

/// Bare `rote`.
///
/// # Errors
///
/// When the corpus cannot be read or written, or the terminal refuses.
pub fn daily(ctx: &Context, aided: bool) -> Result<u8> {
    let mut store = open_store(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(store.chains().records(), &ladder);
    let verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    let today = ctx.today();

    let due = sitting::roster(&corpus, &verifiers, today, &ladder, Mode::Due, aided, &[]);
    if !due.is_empty() {
        return work(ctx, &mut store, &verifiers, &ladder, today, &due, None);
    }

    write_cache(ctx, &corpus, &verifiers, today, &ladder)?;
    if corpus.active().is_empty() {
        if !ctx.quiet {
            println!("nothing to drill · rote enroll <name> to start one");
        }
        return Ok(CLEAN);
    }

    let waiting = nothing_due(&corpus, today, &ladder);
    if !interactive(ctx) {
        if !ctx.quiet {
            println!("{waiting}");
        }
        return Ok(CLEAN);
    }

    // Nothing is due, so the only thing left to offer is practice. Asked rather
    // than assumed: it is voluntary, and a sitting nobody asked for is the
    // schedule deciding again.
    let mut console = term::Terminal::enter(ctx.style)?;
    let wanted = crate::tui::offer(&mut console, today, &waiting, "practice anyway?")?;
    if !wanted {
        return Ok(CLEAN);
    }
    let roster = sitting::roster(
        &corpus,
        &verifiers,
        today,
        &ladder,
        Mode::Practice,
        aided,
        &[],
    );
    work(
        ctx,
        &mut store,
        &verifiers,
        &ladder,
        today,
        &roster,
        Some(console),
    )
}

/// `rote practice`.
///
/// # Errors
///
/// When a named lineage is unknown, or the corpus cannot be read or written.
pub fn practice(ctx: &Context, args: &PracticeArgs, aided: bool) -> Result<u8> {
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
        Mode::Practice,
        aided,
        &args.lineages,
    );
    if roster.is_empty() {
        if !ctx.quiet {
            println!("nothing to drill · rote enroll <name> to start one");
        }
        return Ok(CLEAN);
    }
    work(ctx, &mut store, &verifiers, &ladder, today, &roster, None)
}

/// Run one roster to the end.
fn work(
    ctx: &Context,
    store: &mut Store,
    verifiers: &Verifiers,
    ladder: &Ladder,
    today: Date,
    turns: &[Turn],
    open: Option<term::Terminal>,
) -> Result<u8> {
    let id = SittingId::mint()?;
    let mut console = match open {
        Some(console) => console,
        None => term::Terminal::enter(ctx.style)?,
    };

    let verify = |verifier: &crate::verifier::Verifier, secret: &crate::secret::Secret| {
        Ok(verifier.accepts(secret)?)
    };

    // Two closures write to the same chain, so the handle is shared rather than
    // borrowed twice. The cell is local to the sitting and never escapes it.
    let store = std::cell::RefCell::new(store);
    let held = std::cell::RefCell::new(verifiers.clone());
    let now = ctx.clock.now();
    let paths = ctx.paths.clone();

    let outturn = {
        // Each capture goes to the corpus the moment it is taken, so a sitting
        // cut short keeps every reading it had already produced.
        let mut record = |capture: &sitting::Capture| -> Result<()> {
            let Some(turn) = turns.get(capture.turn) else {
                return Ok(());
            };
            let sitting::Task::Drill(drilling) = &turn.task else {
                return Ok(());
            };
            store.borrow_mut().append(
                now,
                today,
                Event::Capture(Captured {
                    slug: turn.slug.clone(),
                    engram: turn.engram,
                    sitting: id,
                    ordinal: capture.ordinal,
                    occasion: capture.occasion,
                    aided: capture.aided,
                    outcome: capture.outcome,
                    ttfk_ms: capture.ttfk_ms,
                    total_ms: capture.total_ms,
                    corrections: capture.corrections,
                    paste_accepted: capture.paste_accepted,
                    paste_refused: capture.paste_refused,
                    scheduled_interval_days: drilling.scheduled_interval_days,
                    actual_interval_days: drilling.actual_interval_days,
                    effective_interval_days: drilling.effective_interval_days,
                    rung_before: drilling.rung.get(),
                    rung_after: capture.rung_after.get(),
                }),
            )
        };

        // The verifier is written before the record of it, so a failure between
        // the two leaves a fact with no record rather than a record with no
        // fact.
        let mut attach = |index: usize, secret: &crate::secret::Secret| -> Result<()> {
            let Some(turn) = turns.get(index) else {
                return Ok(());
            };
            let verifier = make_verifier(secret)?;
            let mut held = held.borrow_mut();
            held.set(turn.engram, &turn.slug, today, &verifier);
            held.save(&paths.verifiers())?;
            store.borrow_mut().append(
                now,
                today,
                Event::Attach(crate::corpus::record::Attached {
                    slug: turn.slug.clone(),
                    engram: turn.engram,
                }),
            )
        };

        sitting::run(
            turns,
            ctx.config.max_attempts(),
            ladder,
            today,
            sitting::Wiring {
                console: &mut console,
                verify: &verify,
                record: &mut record,
                attach: &mut attach,
            },
        )?
    };

    let store = store.into_inner();
    let held = held.into_inner();
    let after = Corpus::replay(store.chains().records(), ladder);
    let notes = closing_notes(turns, &after, &held);
    let closing = screen::card(
        &screen::Frame::done(today, &outturn.rows, &outturn, &notes),
        ctx.style,
    );
    console.paint(&closing)?;
    console.hold()?;
    drop(console);

    write_cache(ctx, &after, &held, today, ladder)?;
    Ok(if outturn.aborted {
        INCOMPLETE
    } else if outturn.missed() > 0 {
        LAPSE
    } else {
        CLEAN
    })
}

/// What belongs under the closing card: a reading of the corpus *after* the
/// sitting, which the sitting itself cannot know.
fn closing_notes(turns: &[Turn], after: &Corpus, held: &Verifiers) -> Vec<String> {
    let mut notes = Vec::new();
    for turn in turns {
        let Some(lineage) = after.lineage(&turn.slug) else {
            continue;
        };
        let Some(dossier) = lineage.dossier(&turn.engram) else {
            continue;
        };
        let label = after.label(&turn.engram);
        if turn.attaching() && held.holds(&turn.engram) {
            notes.push(format!(
                "{label}  attached — rote took your word for it, and checked nothing"
            ));
        }
        if dossier.aided_mismatch {
            notes.push(format!(
                "{label}  the vault and the verifier disagree — confirm the item, then rote rotate"
            ));
        }
    }
    notes
}

fn interactive(ctx: &Context) -> bool {
    use std::io::IsTerminal as _;
    ctx.format == relic_core::ui::Format::Human && std::io::stdout().is_terminal()
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
