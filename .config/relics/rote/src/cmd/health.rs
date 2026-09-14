//! What `assay` and the shell read.

use anyhow::Result;
use relic_core::finding::{FixHint, Grade, Report, StationId, Summary};
use relic_core::ui::Format;

use super::{Context, flagship, read_chains};
use crate::corpus::Corpus;
use crate::doctor::{self, Health};
use crate::exit::{CLEAN, INCOMPLETE, LAPSE};
use crate::render::json;
use crate::store::Cache;
use crate::verifier::file::Verifiers;

/// `rote doctor`.
///
/// # Errors
///
/// When the report cannot be rendered.
pub fn doctor(ctx: &Context) -> Result<u8> {
    let ladder = ctx.config.ladder()?;
    let machine = ctx.machine()?;
    let chains = match read_chains(ctx) {
        Ok(chains) => chains,
        Err(error) => {
            let report = doctor::unreadable(&error.to_string());
            return emit(ctx, &report);
        }
    };
    let corpus = Corpus::replay(chains.records(), &ladder);
    let verifiers = match Verifiers::load(&ctx.paths.verifiers()) {
        Ok(verifiers) => verifiers,
        Err(error) => {
            let report = doctor::unreadable(&error.to_string());
            return emit(ctx, &report);
        }
    };
    let state = flagship(ctx, &machine)?;
    let report = doctor::report(&Health {
        chains: &chains,
        corpus: &corpus,
        verifiers: &verifiers,
        paths: &ctx.paths,
        machine: &machine,
        host: &ctx.env.host,
        flagship: &state,
        today: ctx.today(),
        ladder: &ladder,
    });
    emit(ctx, &report)
}

fn emit(ctx: &Context, report: &Report) -> Result<u8> {
    match ctx.format {
        Format::Json | Format::Agent => println!("{}", json::document(report)?),
        Format::Human => println!("{}", doctor::render(report, ctx.style)),
    }
    Ok(match report.grade() {
        Grade::Ok => CLEAN,
        Grade::Soft => LAPSE,
        Grade::Broken => INCOMPLETE,
    })
}

/// `rote banner`.
///
/// Decoration, and decoration fails silent: a reminder that cannot work out
/// what to say prints nothing rather than a diagnostic in every new shell. It
/// never resolves the machine identity, which costs a subprocess.
///
/// It reads the cache, and rebuilds it when the cache has stopped answering for
/// what is on disk. The rebuild is the same read-only path `status` and
/// `doctor` take — no flagship gate, no subprocess, no hashing — and it happens
/// once per change rather than once per prompt, so the steady state is still a
/// `stat` and a small read.
///
/// # Errors
///
/// When the report cannot be rendered.
pub fn banner(ctx: &Context) -> Result<u8> {
    let cache = current(ctx);
    let due = cache.as_ref().map_or(0, |cache| cache.due_by(ctx.today()));
    let dormant = cache.as_ref().map_or(0, |cache| cache.dormant);

    // The nag is not the health check. This fires the day a drill comes due;
    // doctor waits out a grace, because a drill taken a day late is a drill
    // taken. coop reads this one, and assay's registry station reads that one.
    if ctx.format == Format::Json {
        let station = StationId::from_static("rote");
        let mut findings = Vec::new();
        if dormant > 0 {
            findings.push(
                station
                    .soft(Summary::lossy(&format!(
                        "{} dormant on this machine",
                        relic_core::fmt::plural(dormant, "lineage", "lineages")
                    )))
                    .fixed_by(FixHint::lossy("rote")),
            );
        }
        if due > 0 {
            findings.push(
                station
                    .soft(Summary::lossy(&format!(
                        "{} due",
                        relic_core::fmt::plural(due, "drill", "drills")
                    )))
                    .fixed_by(FixHint::lossy("rote")),
            );
        }
        println!("{}", json::document(&Report::ran(station, findings))?);
        return Ok(CLEAN);
    }

    let mut said = Vec::new();
    if dormant > 0 {
        said.push(format!(
            "{} dormant here",
            relic_core::fmt::plural(dormant, "lineage", "lineages")
        ));
    }
    if due > 0 {
        said.push(format!(
            "{} due",
            relic_core::fmt::plural(due, "drill", "drills")
        ));
    }
    if said.is_empty() {
        return Ok(CLEAN);
    }
    let text = format!("rote: {}.", said.join(" · "));
    println!("{} {}", ctx.style.bold("==>"), ctx.style.yellow(&text));
    Ok(CLEAN)
}

/// What the reminder should say today, cache or no cache.
///
/// The principle: a reminder must not depend on state destroyed by the event it
/// exists to announce. The cache lives in the machine-local tree that a restore
/// does not bring back, and the state a restore leaves behind — every lineage
/// dormant, nothing drillable — is exactly what the attachment verb exists to
/// serve. Treating absence as silence meant nothing said so until some other
/// command happened to rewrite the cache as a side effect.
fn current(ctx: &Context) -> Option<Cache> {
    let today = ctx.today();
    let held = Cache::load(&ctx.paths.cache());
    if let Some(cache) = &held
        && cache.still_true(&ctx.paths, today)
    {
        return held;
    }
    let Some(rebuilt) = rebuild(ctx, today) else {
        // Nothing could be derived. Whatever was cached is still a better
        // answer than none, and staying quiet here is what decoration owes.
        return held;
    };
    // Best effort: a reminder that cannot write its own cache still reminds.
    let _ = rebuilt.save(&ctx.paths.cache());
    Some(rebuilt)
}

/// Redo the projection from what is actually on disk.
fn rebuild(ctx: &Context, today: jiff::civil::Date) -> Option<Cache> {
    let ladder = ctx.config.ladder().ok()?;
    let chains = read_chains(ctx).ok()?;
    let corpus = Corpus::replay(chains.records(), &ladder);
    let verifiers = Verifiers::load(&ctx.paths.verifiers()).ok()?;
    Some(crate::cmd::project(
        ctx, &corpus, &verifiers, today, &ladder,
    ))
}
