//! The terminal side of the commands that take a secret.
//!
//! One place opens a dialog, because raw mode and the alternate screen are
//! process-wide: a command that opens a second one restores the terminal under
//! the first and turns echo on beneath the next prompt.
//!
//! The policy for taking a secret twice is not here — it is in `intake`, pure,
//! because the sitting needs the same policy and draws a different card.

use anyhow::{Context as _, Result, bail};
use jiff::civil::Date;
use relic_core::style::Tint;

use super::Context;
use crate::exit::{CLEAN, INCOMPLETE};
use crate::intake::{DIFFERED, NOT_CURRENT, Pair, Pairing};
use crate::secret::Secret;
use crate::tui::{self, term};
use crate::verifier::Verifier;

/// The screen a secret is typed on, or nothing when this is a pipe.
///
/// # Errors
///
/// When the terminal cannot be entered.
pub fn open(stdin: bool, ctx: &Context) -> Result<Option<term::Terminal>> {
    if stdin {
        return Ok(None);
    }
    term::Terminal::enter(ctx.style).map(Some)
}

/// Ask for one secret. `None` means the dialog was abandoned rather than
/// answered.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn typed(
    console: &mut term::Terminal,
    today: Date,
    title: &'static str,
    checked: Checked,
    heading: &str,
    intention: &str,
    status: Option<&str>,
) -> Result<Option<Secret>> {
    let prompt = tui::Ask {
        title,
        resting: checked.resting(),
        heading,
        intention,
        status,
    };
    match tui::ask(console, today, &prompt)? {
        tui::Typed::Submitted(entry) => Ok(Some(entry.secret)),
        // Nothing here has a vault to consult: the secret being taken is the one
        // the reader brought with them.
        tui::Typed::Skipped | tui::Typed::Aborted | tui::Typed::Lookup => Ok(None),
    }
}

/// Run something slow with the field taken down and the keyboard drained.
///
/// The dialogs mint and check verifiers outside `tui`, so this is where they
/// meet the same policy the sitting gets: the box goes before the wait, and
/// nothing typed into the wait reaches whatever card comes next. Without a
/// terminal there is no box and nothing to drain, so the work simply runs.
///
/// # Errors
///
/// When the terminal cannot be written to, or the work itself fails.
pub fn working<T>(
    console: Option<&mut term::Terminal>,
    today: Date,
    asking: &Asking<'_>,
    work: impl FnOnce() -> Result<T>,
) -> Result<T> {
    let Some(console) = console else {
        return work();
    };
    let prompt = tui::Ask {
        title: asking.title,
        resting: tui::card::Tone::Calm,
        heading: asking.heading,
        intention: asking.intention,
        status: None,
    };
    let card = tui::waiting(console, today, &prompt);
    tui::while_working(console, &card, work)
}

/// One prompt's own words, carried together because they travel together.
pub struct Asking<'a> {
    /// The card's title, naming the instrument.
    pub title: &'static str,
    /// Whether anything here can check what is typed.
    pub checked: Checked,
    /// What is being asked about.
    pub heading: &'a str,
    /// What this prompt is for.
    pub intention: &'a str,
    /// What giving up here costs, said on the card that gives up.
    ///
    /// The reason has been on the screen for every round it took to get here.
    /// Without this the last card is indistinguishable from one more of them.
    pub cost: &'static str,
}

/// How a double entry ended.
pub enum Twice {
    /// Both entries agreed.
    Agreed(Box<Secret>),
    /// The dialog was walked away from.
    Abandoned,
    /// The rounds ran out, and this is what spent the last one.
    Refused(&'static str),
}

/// Ask for a secret twice until the two agree, for as many rounds as the drill
/// allows tries.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn twice(
    console: &mut term::Terminal,
    today: Date,
    title: &'static str,
    checked: Checked,
    heading: &str,
    intention: &str,
    ctx: &Context,
) -> Result<Twice> {
    let mut pair = Pair::new(ctx.config.max_attempts());
    let mut status: Option<String> = None;
    loop {
        let asking = if pair.holds_one() {
            crate::intake::AGAIN
        } else {
            intention
        };
        let Some(offered) = typed(
            console,
            today,
            title,
            checked,
            heading,
            asking,
            status.as_deref(),
        )?
        else {
            return Ok(Twice::Abandoned);
        };
        // Every refusal says which try the next one is. A bound nobody can see
        // reads as no bound at all, which is what makes a person retype the
        // same pair until they give up on the command rather than on the pair.
        match pair.offer(offered) {
            Pairing::Again => status = None,
            Pairing::Agreed(secret) => return Ok(Twice::Agreed(secret)),
            Pairing::Differed => status = Some(pair.status(DIFFERED)),
            Pairing::Empty => status = Some(pair.status(crate::intake::EMPTY)),
            Pairing::OutOfRounds(reason) => return Ok(Twice::Refused(reason)),
        }
    }
}

/// Prove the current secret before a rotation replaces it.
///
/// Without it a verifier could be replaced by one somebody else knows, and the
/// reading would still say memorised. `Some(code)` means the rotation stopped
/// here and that is the status to leave on.
///
/// # Errors
///
/// When the terminal cannot be read or written, or a pipe offered the wrong
/// secret.
pub fn prove(
    current: &Verifier,
    piped: Option<Secret>,
    console: Option<&mut term::Terminal>,
    today: Date,
    asking: &Asking<'_>,
    ctx: &Context,
) -> Result<Option<u8>> {
    let Asking {
        title,
        heading,
        intention,
        cost,
        ..
    } = *asking;
    if let Some(offered) = piped {
        return if current.accepts(&offered)? {
            Ok(None)
        } else {
            bail!("{NOT_CURRENT}, so nothing was replaced")
        };
    }
    let Some(console) = console else {
        bail!("--stdin wants the current secret first, then the new one");
    };
    let tries = ctx.config.max_attempts();
    for attempt in 1..=tries {
        let status = (attempt > 1).then(|| crate::intake::tried(NOT_CURRENT, attempt, tries));
        let Some(offered) = typed(
            console,
            today,
            title,
            // A proof is checked against the verifier it is proving, which is
            // the whole point of asking for one.
            Checked::Yes,
            heading,
            intention,
            status.as_deref(),
        )?
        else {
            return abandoned(console, today, heading).map(Some);
        };
        let held = working(Some(console), today, asking, || {
            Ok(current.accepts(&offered)?)
        })?;
        if held {
            return Ok(None);
        }
    }
    refused(console, today, heading, NOT_CURRENT, cost).map(Some)
}

/// Close a dialog that was walked away from.
///
/// # Errors
///
/// When the terminal cannot be written.
pub fn abandoned(console: &mut term::Terminal, today: Date, heading: &str) -> Result<u8> {
    tui::outcome(console, today, heading, "nothing was changed", Tint::Dim)?;
    Ok(INCOMPLETE)
}

/// Close a dialog on an outcome that is a refusal.
///
/// Said on the card and held, rather than raised: the screen is erased on the
/// way out, so an error printed after it is an error printed to a person who has
/// already been told nothing.
///
/// # Errors
///
/// When the terminal cannot be written.
pub fn refused(
    console: &mut term::Terminal,
    today: Date,
    heading: &str,
    reason: &str,
    cost: &str,
) -> Result<u8> {
    // The reason has been on the screen for every round it took to get here, so
    // on its own it reads as one more of them. What this card is for is the
    // half that has not been said: the command has stopped, and it wrote
    // nothing.
    //
    // Where the line cannot hold both, that is also which one survives — the
    // same rule the hint under a field follows, and for the same reason.
    let both = format!("{reason} — {cost}");
    let said = if both.chars().count() <= tui::OUTCOME_ROOM {
        both
    } else {
        cost.to_owned()
    };
    tui::outcome(console, today, heading, &said, Tint::Red)?;
    Ok(INCOMPLETE)
}

/// Whether what a dialog wrote was checked against anything.
///
/// The same rule the sitting's glyphs hold: green is only ever something that
/// stood up to a check. An attachment holds nothing to check against, by
/// design, so closing it in green would claim a verdict nobody rendered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Checked {
    /// The secret was proved, or freshly minted from two matching entries.
    Yes,
    /// Nothing here could check it, and nothing did.
    No,
}

impl Checked {
    /// What a closing card is tinted.
    fn tint(self) -> Tint {
        match self {
            Self::Yes => Tint::Green,
            Self::No => Tint::Yellow,
        }
    }

    /// What the field rests as while it is being typed into.
    fn resting(self) -> crate::tui::card::Tone {
        match self {
            Self::Yes => crate::tui::card::Tone::Calm,
            Self::No => crate::tui::card::Tone::Unchecked,
        }
    }
}

/// Close a dialog on the outcome it was opened for, or say it on stdout when
/// there was no dialog at all.
///
/// # Errors
///
/// When the terminal cannot be written.
pub fn settled(
    ctx: &Context,
    console: Option<&mut term::Terminal>,
    today: Date,
    heading: &str,
    said: &str,
    checked: Checked,
) -> Result<u8> {
    match console {
        Some(console) => tui::outcome(console, today, heading, said, checked.tint())?,
        None => {
            if !ctx.quiet {
                println!("{said}");
            }
        }
    }
    Ok(CLEAN)
}

/// Read secrets from a pipe, one per line.
///
/// Refused when stdin is a terminal: a secret typed into an echoing read lands
/// on the screen and in scrollback. That the command line itself must carry no
/// secret is the pipe's own discipline, and no stream check can enforce it.
///
/// # Errors
///
/// When stdin is a terminal, cannot be read, or offered fewer lines than asked
/// for.
pub fn piped(count: usize) -> Result<Vec<Secret>> {
    use std::io::{IsTerminal as _, Read as _};
    if std::io::stdin().is_terminal() {
        bail!("--stdin is for a pipe. At a terminal, type the secret instead");
    }
    let mut buffer = zeroize::Zeroizing::new(Vec::new());
    std::io::stdin()
        .read_to_end(&mut buffer)
        .context("reading stdin")?;
    let mut secrets = Vec::new();
    for line in buffer.split(|byte| *byte == b'\n').take(count) {
        let line = line.strip_suffix(b"\r").unwrap_or(line);
        secrets.push(
            Secret::from_bytes(line).context("that is longer than a secret this tool will take")?,
        );
    }
    if secrets.len() < count {
        bail!("expected {count} lines on stdin, one secret per line");
    }
    Ok(secrets)
}
