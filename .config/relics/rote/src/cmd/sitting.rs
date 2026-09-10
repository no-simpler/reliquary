//! The drill: the daily call and voluntary practice.

use anyhow::{Context as _, Result};
use jiff::civil::Date;

use super::{Context, stamped, write_cache};
use crate::cli::PracticeArgs;
use crate::drill::{self, Mode, screen};
use crate::exit::{CLEAN, INCOMPLETE, LAPSE};
use crate::ladder::{self, Class, Gate, Standing};
use crate::log::{Attempted, Event, SessionId};
use crate::model::State;
use crate::slug::Slug;
use crate::store::{Store, Verifiers};
use crate::tui::{Screen as _, term};

/// Bare `rote`.
pub fn daily(ctx: &Context, aided: bool) -> Result<u8> {
    sitting(ctx, Mode::Daily(ctx.config.filler()), &[], aided)
}

/// `rote practice`.
pub fn practice(ctx: &Context, args: &PracticeArgs, aided: bool) -> Result<u8> {
    sitting(ctx, Mode::Practice, &args.slugs, aided)
}

fn sitting(ctx: &Context, mode: Mode, only: &[Slug], aided: bool) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    let verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    let today = ctx.today();

    for slug in only {
        if state.get(slug).is_none() {
            anyhow::bail!("there is no slug called {slug}");
        }
    }

    let mut plan = drill::plan(&state, &verifiers, today, mode, only);
    if aided {
        // Declared up front, because the lookup already happened. Every entry in
        // the sitting is a transcription and none of it is a reading.
        for item in &mut plan.items {
            item.class = Class::Aided;
        }
    }
    if plan.is_empty() {
        write_cache(ctx, &state, today)?;
        if !ctx.quiet {
            println!("{}", nothing_due(&state, today));
        }
        return Ok(CLEAN);
    }

    // Nothing here can be asked, so say so inline rather than opening a screen
    // that would only report it and vanish.
    if plan.missing().count() == plan.items.len() {
        let names: Vec<String> = plan.missing().map(|item| item.slug.to_string()).collect();
        println!(
            "no verifier on this machine for {} · rote rekey --force to re-enroll",
            names.join(", ")
        );
        return Ok(INCOMPLETE);
    }

    let session = new_session()?;
    let mut console = term::Terminal::enter(ctx.style)?;
    let verify = |item: &drill::Item, secret: &crate::secret::Secret| match &item.verifier {
        Some(verifier) => Ok(verifier.accepts(secret)?),
        None => Ok(false),
    };
    // Each entry goes to the log the moment it is taken, so a sitting cut short
    // keeps every reading it had already produced.
    let mut write = |taken: &drill::Taken| -> Result<()> {
        let Some(item) = plan.items.get(taken.item) else {
            return Ok(());
        };
        store.append(&stamped(
            ctx,
            today,
            Event::Attempt(attempted(item, &session, taken)),
        ))
    };
    let taken = drill::run(
        &plan,
        ctx.config.max_attempts(),
        &mut console,
        &verify,
        &mut write,
        today,
    )?;

    let after = State::replay(store.journal().lines());
    let notes = closing_notes(&plan, &after);
    let closing = screen::card(
        &screen::Frame::done(today, &taken.rows, &taken, &notes),
        ctx.style,
    );
    console.paint(&closing)?;
    console.hold()?;
    drop(console);

    write_cache(ctx, &after, today)?;
    Ok(if taken.aborted {
        INCOMPLETE
    } else if taken.missed() > 0 {
        LAPSE
    } else {
        CLEAN
    })
}

/// The wire form of one entry: what the sitting saw, joined to what the plan
/// knew about the slug.
fn attempted(item: &drill::Item, session: &SessionId, taken: &drill::Taken) -> Attempted {
    Attempted {
        slug: item.slug.clone(),
        session: session.clone(),
        version: item.version,
        class: taken.class,
        attempt: taken.attempt,
        outcome: taken.outcome,
        ttfk_ms: taken.ttfk_ms,
        total_ms: taken.total_ms,
        corrections: taken.corrections,
        paste_refused: taken.paste_refused,
        scheduled_interval_days: item.scheduled_interval_days,
        actual_interval_days: item.actual_interval_days,
        effective_interval_days: item.effective_interval_days,
        stretch: item.stretch(),
        step_before: item.step.get(),
        step_after: taken.step_after.get(),
    }
}

/// What belongs under the closing card: a reading of the log *after* the
/// sitting, which the sitting itself cannot know.
fn closing_notes(plan: &drill::Plan, after: &State) -> Vec<String> {
    let mut notes = Vec::new();
    for item in &plan.items {
        let Some(slug) = after.get(&item.slug) else {
            continue;
        };
        if slug.aided_mismatch {
            notes.push(format!(
                "{}  the vault and the verifier disagree — confirm the item, then rote rekey",
                slug.slug
            ));
        }
        if let Gate::Ready { passes } = slug.gate() {
            notes.push(format!(
                "{}  cutover ready — {passes} cold passes at {}d",
                slug.slug,
                ladder::cap_days()
            ));
        }
    }
    notes
}

fn nothing_due(state: &State, today: Date) -> String {
    let next = state
        .scheduled()
        .into_iter()
        .filter_map(|slug| match slug.standing(today) {
            Standing::Waiting { until } | Standing::Held { until } => Some(until),
            Standing::Due | Standing::Probe => None,
        })
        .min();
    match next {
        Some(day) => format!(
            "nothing due · next in {}",
            relic_core::fmt::plural(
                usize::try_from(ladder::days_between(today, day)).unwrap_or(0),
                "day",
                "days"
            )
        ),
        None => "nothing to drill · rote add <slug> to start one".to_owned(),
    }
}

fn new_session() -> Result<SessionId> {
    let mut bytes = [0u8; 8];
    getrandom::fill(&mut bytes).context("drawing a session identifier")?;
    Ok(SessionId::from_bytes(bytes))
}
