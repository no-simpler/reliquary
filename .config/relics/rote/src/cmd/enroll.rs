//! Everything that takes a secret in, proves one, or lets one go: enrollment,
//! rotation, retirement, and the stretch horizon.
//!
//! Every command here opens the store before it touches the verifier file, so
//! a log this binary must not write to is refused before anything else has
//! changed.

use anyhow::{Context as _, Result, bail};
use jiff::civil::Date;
use relic_core::style::Tint;

use super::{Context, stamped, write_cache};
use crate::cli::{AddArgs, ProbeArgs, RekeyArgs, RetireArgs};
use crate::exit::{CLEAN, INCOMPLETE};
use crate::log::{Added, Event, Probed, Rekeyed, Retired};
use crate::model::State;
use crate::secret::Secret;
use crate::store::{Store, Verifiers};
use crate::tui::{self, term};
use crate::verifier::Verifier;

/// The furthest a horizon may sit, in days. Beyond a year it is a retirement
/// wearing a probe's clothes.
const HORIZON_MAX_DAYS: u32 = 365;

pub fn add(ctx: &Context, args: &AddArgs) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    if let Some(slug) = state.get(&args.slug) {
        if slug.retired {
            bail!(
                "{} is retired. rote rekey --force {} puts it back on the schedule with a new verifier",
                args.slug,
                args.slug
            );
        }
        bail!(
            "{} is already enrolled. Use rote rekey to replace its verifier",
            args.slug
        );
    }
    let today = ctx.today();
    let heading = format!("enroll {}", args.slug);
    let mut dialog = dialog(args.stdin, ctx)?;
    let secret = match dialog.as_mut() {
        None => piped(1)?.into_iter().next().unwrap_or_else(Secret::new),
        Some(console) => match twice(console, today, &heading, "type it twice", ctx)? {
            Twice::Agreed(secret) => secret,
            Twice::Abandoned => return abandoned(console, today, &heading),
            Twice::Differed => return refused(console, today, &heading, DIFFERED),
        },
    };
    if secret.is_empty() {
        return match dialog.as_mut() {
            Some(console) => refused(console, today, &heading, EMPTY),
            None => bail!("{EMPTY}"),
        };
    }
    let verifier = make_verifier(&secret)?;
    drop(secret);

    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    verifiers.set(&args.slug, &verifier);
    verifiers.save(&ctx.paths.verifiers())?;

    store.append(&stamped(
        ctx,
        today,
        Event::Add(Added {
            slug: args.slug.clone(),
            version: 1,
            critical: args.critical,
        }),
    ))?;
    let after = State::replay(store.journal().lines());
    write_cache(ctx, &after, today)?;
    let said = format!("{} enrolled · first review tomorrow", args.slug);
    match dialog.as_mut() {
        Some(console) => tui::outcome(console, today, &heading, &said, Tint::Green)?,
        None => {
            if !ctx.quiet {
                println!("{said}");
            }
        }
    }
    Ok(CLEAN)
}

pub fn rekey(ctx: &Context, args: &RekeyArgs) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    let Some(slug) = state.get(&args.slug) else {
        bail!("there is no slug called {}", args.slug);
    };
    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    // Decided before a screen opens: an error printed after an empty alternate
    // screen has closed is an error nobody saw the reason for.
    let current = if args.force {
        None
    } else {
        Some(verifiers.get(&args.slug)?.ok_or_else(|| {
            anyhow::anyhow!(
                "there is no verifier for {} on this machine, so there is nothing to prove against. Use --force to re-enroll",
                args.slug
            )
        })?)
    };
    let today = ctx.today();
    let heading = format!("rekey {}", args.slug);
    let mut dialog = dialog(args.stdin, ctx)?;

    let mut piped_secrets = if args.stdin {
        piped(if args.force { 1 } else { 2 })?.into_iter()
    } else {
        Vec::new().into_iter()
    };

    if let Some(current) = current
        && let Some(code) = prove(
            &current,
            piped_secrets.next(),
            &mut dialog,
            today,
            &heading,
            ctx,
        )?
    {
        return Ok(code);
    }

    let secret = if let Some(secret) = piped_secrets.next() {
        secret
    } else {
        let Some(console) = dialog.as_mut() else {
            bail!("--stdin wants a new secret on its own line");
        };
        match twice(console, today, &heading, "the new secret, typed twice", ctx)? {
            Twice::Agreed(secret) => secret,
            Twice::Abandoned => return abandoned(console, today, &heading),
            Twice::Differed => return refused(console, today, &heading, DIFFERED),
        }
    };
    if secret.is_empty() {
        return match dialog.as_mut() {
            Some(console) => refused(console, today, &heading, EMPTY),
            None => bail!("{EMPTY}"),
        };
    }
    let verifier = make_verifier(&secret)?;
    drop(secret);

    let version = slug.version.saturating_add(1);
    verifiers.set(&args.slug, &verifier);
    verifiers.save(&ctx.paths.verifiers())?;

    store.append(&stamped(
        ctx,
        today,
        Event::Rekey(Rekeyed {
            slug: args.slug.clone(),
            version,
            proved: !args.force,
        }),
    ))?;
    let after = State::replay(store.journal().lines());
    write_cache(ctx, &after, today)?;
    let said = format!(
        "{} rekeyed to version {version} · the ladder starts over",
        args.slug
    );
    match dialog.as_mut() {
        Some(console) => tui::outcome(console, today, &heading, &said, Tint::Green)?,
        None => {
            if !ctx.quiet {
                println!("{said}");
            }
        }
    }
    Ok(CLEAN)
}

pub fn retire(ctx: &Context, args: &RetireArgs) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    let Some(slug) = state.get(&args.slug) else {
        bail!("there is no slug called {}", args.slug);
    };
    if slug.retired {
        bail!("{} is already retired", args.slug);
    }
    // The verifier goes with it: an oracle for a secret nobody drills any more
    // is cost with no benefit.
    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    verifiers.remove(&args.slug);
    verifiers.save(&ctx.paths.verifiers())?;

    let today = ctx.today();
    store.append(&stamped(
        ctx,
        today,
        Event::Retire(Retired {
            slug: args.slug.clone(),
        }),
    ))?;
    let after = State::replay(store.journal().lines());
    write_cache(ctx, &after, today)?;
    if !ctx.quiet {
        println!(
            "{} retired · its history stays, its verifier does not",
            args.slug
        );
    }
    Ok(CLEAN)
}

pub fn probe(ctx: &Context, args: &ProbeArgs) -> Result<u8> {
    let mut store = Store::open(ctx.paths.clone())?;
    let state = State::replay(store.journal().lines());
    let Some(slug) = state.get(&args.slug) else {
        bail!("there is no slug called {}", args.slug);
    };
    if slug.retired {
        bail!(
            "{} is retired, and a horizon on it would never be reached",
            args.slug
        );
    }
    let today = ctx.today();
    let until = if args.clear {
        None
    } else {
        let Some(days) = args.days else {
            bail!("say how far out the horizon sits, with --in <DAYS>, or drop it with --clear");
        };
        if days == 0 || days > HORIZON_MAX_DAYS {
            bail!("a horizon sits one day to a year out, and {days} is neither");
        }
        Some(
            today
                .checked_add(jiff::Span::new().days(i64::from(days)))
                .context("that horizon does not land on a date")?,
        )
    };
    store.append(&stamped(
        ctx,
        today,
        Event::Probe(Probed {
            slug: args.slug.clone(),
            until,
        }),
    ))?;
    let after = State::replay(store.journal().lines());
    write_cache(ctx, &after, today)?;
    if !ctx.quiet {
        match until {
            Some(day) => println!("{} held out of the reminder until {day}", args.slug),
            None => println!("{} back on the schedule", args.slug),
        }
    }
    Ok(CLEAN)
}

const DIFFERED: &str = "the two entries differ";
const EMPTY: &str = "an empty secret is not a secret";
const NOT_CURRENT: &str = "that is not the current secret, so nothing was replaced";

/// Prove the current secret before a rotation replaces it.
///
/// Without it a verifier could be replaced by one somebody else knows, and the
/// reading would still say memorised. At a terminal a slipped key costs a try,
/// not the command; a pipe gets the one line it sent. `Some(code)` means the
/// rotation stopped here and that is the status to leave on.
fn prove(
    current: &Verifier,
    piped: Option<Secret>,
    dialog: &mut Option<term::Terminal>,
    today: Date,
    heading: &str,
    ctx: &Context,
) -> Result<Option<u8>> {
    if let Some(offered) = piped {
        return if current.accepts(&offered)? {
            Ok(None)
        } else {
            bail!("{NOT_CURRENT}")
        };
    }
    let Some(console) = dialog.as_mut() else {
        bail!("--stdin wants the current secret first, then the new one");
    };
    let tries = ctx.config.max_attempts();
    for attempt in 1..=tries {
        let status =
            (attempt > 1).then(|| format!("not the current secret · try {attempt} of {tries}"));
        let Some(offered) = typed(
            console,
            today,
            heading,
            "prove the current secret before it is replaced",
            status.as_deref(),
        )?
        else {
            return abandoned(console, today, heading).map(Some);
        };
        if current.accepts(&offered)? {
            return Ok(None);
        }
    }
    refused(console, today, heading, NOT_CURRENT).map(Some)
}

/// The screen a secret is typed on, or nothing when this is a pipe.
///
/// The only place a dialog is opened. Raw mode and the alternate screen are
/// process-wide, so a command that opens a second one puts the terminal back
/// under the first — and holding that to one call site is what keeps a later
/// edit from reintroducing it. `Terminal::enter` refuses a second all the same.
fn dialog(stdin: bool, ctx: &Context) -> Result<Option<term::Terminal>> {
    if stdin {
        return Ok(None);
    }
    term::Terminal::enter(ctx.style).map(Some)
}

/// Ask for one secret. `None` means the dialog was abandoned rather than
/// answered.
fn typed(
    console: &mut term::Terminal,
    today: Date,
    heading: &str,
    intention: &str,
    status: Option<&str>,
) -> Result<Option<Secret>> {
    let prompt = tui::Ask {
        heading,
        intention,
        status,
    };
    match tui::ask(console, today, &prompt)? {
        tui::Typed::Submitted(entry) => Ok(Some(entry.secret)),
        // Nothing here has a vault to consult: the secret being enrolled is the
        // one the reader brought with them.
        tui::Typed::Skipped | tui::Typed::Aborted | tui::Typed::Lookup => Ok(None),
    }
}

/// How a double entry ended.
enum Twice {
    /// Both entries agreed.
    Agreed(Secret),
    /// The dialog was walked away from.
    Abandoned,
    /// The rounds ran out with the two entries still apart.
    Differed,
}

/// Ask for a secret twice until the two agree, for as many rounds as the drill
/// allows tries. A mismatch is said on the line under the field and the pair
/// is asked for again; a blind field gives no other way to find the slip.
fn twice(
    console: &mut term::Terminal,
    today: Date,
    heading: &str,
    intention: &str,
    ctx: &Context,
) -> Result<Twice> {
    let rounds = ctx.config.max_attempts();
    for round in 1..=rounds {
        let status = (round > 1).then_some(DIFFERED);
        let Some(first) = typed(console, today, heading, intention, status)? else {
            return Ok(Twice::Abandoned);
        };
        let Some(again) = typed(console, today, heading, "again", None)? else {
            return Ok(Twice::Abandoned);
        };
        if first.same_as(&again) {
            return Ok(Twice::Agreed(first));
        }
    }
    Ok(Twice::Differed)
}

/// Close a dialog that was walked away from.
fn abandoned(console: &mut term::Terminal, today: Date, heading: &str) -> Result<u8> {
    tui::outcome(console, today, heading, "nothing was changed", Tint::Dim)?;
    Ok(INCOMPLETE)
}

/// Close a dialog on an outcome that is a refusal.
///
/// Said on the card and held, rather than raised: the screen is erased on the
/// way out, so an error printed after it is an error printed to a person who has
/// already been told nothing.
fn refused(console: &mut term::Terminal, today: Date, heading: &str, text: &str) -> Result<u8> {
    tui::outcome(console, today, heading, text, Tint::Red)?;
    Ok(INCOMPLETE)
}

/// Read secrets from a pipe, one per line.
///
/// Refused when stdin is a terminal: a secret typed into an echoing read lands
/// on the screen and in scrollback. That the command line itself must carry no
/// secret is the pipe's own discipline, and no stream check can enforce it.
fn piped(count: usize) -> Result<Vec<Secret>> {
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

fn make_verifier(secret: &Secret) -> Result<Verifier> {
    let mut salt = [0u8; crate::verifier::SALT_LEN];
    getrandom::fill(&mut salt).context("drawing a salt")?;
    Ok(Verifier::create(secret, &salt)?)
}
