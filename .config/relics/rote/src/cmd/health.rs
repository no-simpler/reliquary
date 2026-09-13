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
/// Decoration, and decoration fails silent: a reminder that cannot read its own
/// cache prints nothing rather than a diagnostic in every new shell. It reads
/// only the cache — never the corpus, never a verifier, never the machine
/// identity, which costs a subprocess.
///
/// # Errors
///
/// When the report cannot be rendered.
pub fn banner(ctx: &Context) -> Result<u8> {
    let cache = Cache::load(&ctx.paths.cache());
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
