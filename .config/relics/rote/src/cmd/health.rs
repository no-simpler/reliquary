//! What `assay` and the shell read.

use anyhow::Result;
use relic_core::finding::{FixHint, Grade, Report, StationId, Summary};
use relic_core::ui::Format;

use super::{Context, Reminder, flagship, read_chains, reminder};
use crate::corpus::Corpus;
use crate::doctor::{self, Health};
use crate::exit::{CLEAN, INCOMPLETE, LAPSE};
use crate::render::json;
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
/// It rebuilds its answer from disk on every call, by the same read-only path
/// `status` and `doctor` take — no flagship gate, no subprocess, no hashing.
/// Nothing is cached, so nothing can go stale: `coop` keys the call on the
/// stamp, which every write touches, and on the day rollover.
///
/// # Errors
///
/// When the report cannot be rendered.
pub fn banner(ctx: &Context) -> Result<u8> {
    // A reminder must not depend on state destroyed by the event it exists
    // to announce: a restore brings the corpus back and leaves the verifiers
    // behind, and the state that leaves — every lineage dormant, nothing
    // drillable — is exactly what the attachment verb exists to serve. Reading
    // both afresh is what makes that state answerable rather than silent.
    // Anything that cannot be read is nothing to say.
    let counted = (|| {
        let ladder = ctx.config.ladder().ok()?;
        let chains = read_chains(ctx).ok()?;
        let corpus = Corpus::replay(chains.records(), &ladder);
        let verifiers = Verifiers::load(&ctx.paths.verifiers()).ok()?;
        Some(reminder(&corpus, &verifiers, ctx.today(), &ladder))
    })();
    let Some(Reminder { due, dormant }) = counted else {
        return Ok(CLEAN);
    };

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
