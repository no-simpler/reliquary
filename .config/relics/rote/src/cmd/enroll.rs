//! Everything that takes a secret or drops one.
//!
//! Four verbs, and the distinction between three of them is the whole point:
//!
//! - **enroll** opens a lineage with its first engram.
//! - **rotate** proves the current secret, supersedes its engram, and starts the
//!   ladder over on a new one. A different secret, a different memory.
//! - **attach** makes a verifier here for the engram that is already current,
//!   and leaves its record untouched. The same secret, a machine that lost its
//!   copy.
//! - **retire** takes a lineage off the schedule and drops its verifiers.
//!
//! `rote` cannot tell an attach from a rotate by looking at what was typed: it
//! holds no recoverable form of a secret and has nothing to check a claim
//! against. So it records **which claim was made** and verifies neither. That is
//! the good faith principle at its sharpest — and it is a better trade than the
//! alternative, where losing a laptop costs the whole record of a memory you
//! still hold.

use anyhow::{Result, bail};
use jiff::civil::Date;

use super::{Context, dialog, open_store, save_verifiers};
use crate::cli::{AttachArgs, EnrollArgs, RetireArgs, RotateArgs};
use crate::cmd::dialog::{Asking, Checked, Twice};
use crate::corpus::record::{Attached, EngramId, Enrolled, Event, Retired, Rotated};
use crate::corpus::{Corpus, Lineage};
use crate::exit::{CLEAN, INCOMPLETE};
use crate::intake::{
    ATTACH_INTENTION, EMPTY, ENROLL_INTENTION, ROTATE_NEW_INTENTION, ROTATE_PROVE_INTENTION,
    make_verifier,
};
use crate::secret::Secret;
use crate::tui::term;
use crate::verifier::file::Verifiers;

/// `rote enroll`.
///
/// # Errors
///
/// When the lineage already exists, the terminal refuses, or nothing can be
/// written.
pub fn enroll(ctx: &Context, args: &EnrollArgs) -> Result<u8> {
    let mut store = open_store(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(store.chains().records(), &ladder);
    if let Some(lineage) = corpus.lineage(&args.slug) {
        if lineage.retired {
            bail!(
                "{} is retired. rote rotate --force {} puts it back with a new engram",
                args.slug,
                args.slug
            );
        }
        bail!(
            "{} is already enrolled. rote rotate {} replaces the secret; \
             rote attach {} makes a verifier on this machine",
            args.slug,
            args.slug,
            args.slug
        );
    }

    let today = ctx.today();
    let heading = args.slug.to_string();
    let asking = Asking {
        title: "enroll",
        // Typed twice and compared, which is the whole of the check there can
        // be for a secret nothing else here has ever seen.
        checked: Checked::Yes,
        heading: &heading,
        intention: ENROLL_INTENTION,
        cost: "nothing was enrolled",
    };
    let mut console = dialog::open(args.stdin, ctx)?;
    let Some(secret) = take_one(ctx, &mut console, today, &asking, args.stdin)? else {
        return Ok(INCOMPLETE);
    };

    let engram = EngramId::mint()?;
    let verifier = dialog::working(console.as_mut(), today, &asking, || make_verifier(&secret))?;
    drop(secret);

    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    verifiers.set(engram, &args.slug, today, &verifier);
    save_verifiers(ctx, &verifiers)?;

    store.append(
        ctx.clock.now(),
        today,
        Event::Enroll(Enrolled {
            slug: args.slug.clone(),
            engram,
            critical: args.critical,
        }),
    )?;

    let said = format!("{}@1 enrolled · first review tomorrow", args.slug);
    dialog::settled(ctx, console.as_mut(), today, &heading, &said, Checked::Yes)
}

/// `rote attach`.
///
/// # Errors
///
/// When the lineage is unknown or retired, the terminal refuses, or nothing can
/// be written.
pub fn attach(ctx: &Context, args: &AttachArgs) -> Result<u8> {
    let mut store = open_store(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(store.chains().records(), &ladder);
    let lineage = live(&corpus, &args.slug)?;
    let Some(dossier) = lineage.current() else {
        bail!("{} holds no engram to attach", args.slug);
    };
    let engram = dossier.engram;
    let label = corpus.label(&engram);

    let today = ctx.today();
    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    let replacing = verifiers.holds(&engram);

    // The accident guard: name what is being continued before asking for it, so
    // attaching the wrong lineage or the wrong generation has a moment to be
    // noticed. It is not a check — there is nothing here to check against.
    let heading = crate::tui::card::fit(&if replacing {
        format!("{label} · replacing the verifier held here")
    } else {
        crate::sitting::describe(&label, dossier, today)
    });
    let asking = Asking {
        title: "attach",
        checked: Checked::No,
        heading: &heading,
        intention: ATTACH_INTENTION,
        cost: "nothing was attached",
    };

    let mut console = dialog::open(args.stdin, ctx)?;
    let Some(secret) = take_one(ctx, &mut console, today, &asking, args.stdin)? else {
        return Ok(INCOMPLETE);
    };

    let verifier = dialog::working(console.as_mut(), today, &asking, || make_verifier(&secret))?;
    drop(secret);
    verifiers.set(engram, &args.slug, today, &verifier);
    save_verifiers(ctx, &verifiers)?;

    store.append(
        ctx.clock.now(),
        today,
        Event::Attach(Attached {
            slug: args.slug.clone(),
            engram,
        }),
    )?;

    let said = format!("{label} attached here · rote took your word for it");
    dialog::settled(ctx, console.as_mut(), today, &heading, &said, Checked::No)
}

/// `rote rotate`.
///
/// # Errors
///
/// When the lineage is unknown, the current secret cannot be proved, the
/// terminal refuses, or nothing can be written.
pub fn rotate(ctx: &Context, args: &RotateArgs) -> Result<u8> {
    let mut store = open_store(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(store.chains().records(), &ladder);
    let Some(lineage) = corpus.lineage(&args.slug) else {
        bail!("there is no lineage called {}", args.slug);
    };
    let Some(dossier) = lineage.current() else {
        bail!("{} holds no engram to rotate", args.slug);
    };
    let from = dossier.engram;
    let label = corpus.label(&from);

    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    // Resolved before any screen opens: an error after an empty alternate
    // screen is an error nobody saw.
    let current = if args.force {
        None
    } else {
        match verifiers.get(&from)? {
            Some(verifier) => Some(verifier),
            None => bail!(
                "{label} is dormant on this machine, so there is nothing to prove against.\n\
                 rote attach {} if the secret is unchanged; rote rotate --force {} if it is not",
                args.slug,
                args.slug
            ),
        }
    };

    let today = ctx.today();
    let heading = label.clone();
    let lines = if args.force { 1 } else { 2 };
    let mut fed = if args.stdin {
        dialog::piped(lines)?
    } else {
        Vec::new()
    };
    fed.reverse();
    let mut console = dialog::open(args.stdin, ctx)?;

    if let Some(current) = current
        && let Some(code) = dialog::prove(
            &current,
            fed.pop(),
            console.as_mut(),
            today,
            &Asking {
                title: "rotate",
                checked: Checked::Yes,
                heading: &heading,
                intention: ROTATE_PROVE_INTENTION,
                cost: "nothing was replaced",
            },
            ctx,
        )?
    {
        return Ok(code);
    }

    let asking = Asking {
        title: "rotate",
        checked: Checked::Yes,
        heading: &heading,
        intention: ROTATE_NEW_INTENTION,
        cost: "nothing was replaced",
    };
    let Some(secret) = settle(ctx, &mut console, today, &asking, fed.pop())? else {
        return Ok(INCOMPLETE);
    };

    let to = EngramId::mint()?;
    let verifier = dialog::working(console.as_mut(), today, &asking, || make_verifier(&secret))?;
    drop(secret);

    // The rotation owes the secret it retires this: keyed by engram, setting the
    // new one leaves the old in place, and a verifier for a secret that has been
    // rotated away is a live oracle for it.
    verifiers.remove(&from);
    verifiers.set(to, &args.slug, today, &verifier);
    save_verifiers(ctx, &verifiers)?;

    store.append(
        ctx.clock.now(),
        today,
        Event::Rotate(Rotated {
            slug: args.slug.clone(),
            from,
            to,
            proved: !args.force,
        }),
    )?;

    let after = Corpus::replay(store.chains().records(), &ladder);
    let ordinal = after
        .lineage(&args.slug)
        .and_then(|lineage| lineage.ordinal(&to))
        .unwrap_or(1);
    let said = format!("{}@{ordinal} rotated · the ladder starts over", args.slug);
    dialog::settled(ctx, console.as_mut(), today, &heading, &said, Checked::Yes)
}

/// `rote retire`.
///
/// # Errors
///
/// When the lineage is unknown or already retired, or nothing can be written.
pub fn retire(ctx: &Context, args: &RetireArgs) -> Result<u8> {
    let mut store = open_store(ctx)?;
    let ladder = ctx.config.ladder()?;
    let corpus = Corpus::replay(store.chains().records(), &ladder);
    let lineage = live(&corpus, &args.slug)?;

    let mut verifiers = Verifiers::load(&ctx.paths.verifiers())?;
    for dossier in &lineage.engrams {
        verifiers.remove(&dossier.engram);
    }
    save_verifiers(ctx, &verifiers)?;

    let today = ctx.today();
    store.append(
        ctx.clock.now(),
        today,
        Event::Retire(Retired {
            slug: args.slug.clone(),
        }),
    )?;

    if !ctx.quiet {
        println!(
            "{} retired · its history stays, its verifiers do not",
            args.slug
        );
    }
    Ok(CLEAN)
}

fn live<'a>(corpus: &'a Corpus, slug: &crate::slug::Slug) -> Result<&'a Lineage> {
    let Some(lineage) = corpus.lineage(slug) else {
        bail!("there is no lineage called {slug}");
    };
    if lineage.retired {
        bail!("{slug} is retired, so nothing drills it");
    }
    Ok(lineage)
}

/// Take one secret, from a pipe once or from a terminal twice.
fn take_one(
    ctx: &Context,
    console: &mut Option<term::Terminal>,
    today: Date,
    asking: &Asking<'_>,
    stdin: bool,
) -> Result<Option<Secret>> {
    let fed = if stdin { dialog::piped(1)?.pop() } else { None };
    settle(ctx, console, today, asking, fed)
}

/// A secret from the pipe if there is one, else a double entry at the terminal.
fn settle(
    ctx: &Context,
    console: &mut Option<term::Terminal>,
    today: Date,
    asking: &Asking<'_>,
    fed: Option<Secret>,
) -> Result<Option<Secret>> {
    let Asking {
        title,
        checked,
        heading,
        intention,
        cost,
    } = *asking;
    if let Some(secret) = fed {
        if secret.is_empty() {
            bail!("{EMPTY}");
        }
        return Ok(Some(secret));
    }
    let Some(console) = console.as_mut() else {
        bail!("--stdin wants the secret on its own line");
    };
    match dialog::twice(console, today, title, checked, heading, intention, ctx)? {
        Twice::Agreed(secret) => Ok(Some(*secret)),
        Twice::Abandoned => {
            dialog::abandoned(console, today, heading)?;
            Ok(None)
        }
        Twice::Refused(reason) => {
            dialog::refused(console, today, heading, reason, cost)?;
            Ok(None)
        }
    }
}
