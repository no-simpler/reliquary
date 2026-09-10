//! What `assay` collects and what the shell reads.

use anyhow::Result;
use relic_core::finding::{FixHint, Grade, Report, StationId, Summary};
use relic_core::ui::Format;

use super::Context;
use crate::exit::{CLEAN, INCOMPLETE, LAPSE};
use crate::model::State;
use crate::render::json;
use crate::store::{Cache, Journal, Verifiers};

pub fn doctor(ctx: &Context) -> Result<u8> {
    let today = ctx.today();
    let report = match Journal::load(&ctx.paths.log()) {
        Ok(journal) => {
            let state = State::replay(journal.lines());
            let verifiers = Verifiers::load(&ctx.paths.verifiers()).unwrap_or_default();
            crate::doctor::report(
                &journal,
                &state,
                &verifiers,
                &ctx.paths,
                today,
                &ctx.env.host,
            )
        }
        Err(error) => crate::doctor::unreadable(&format!("{error:#}")),
    };
    if ctx.format == Format::Json {
        println!("{}", json::document(&report)?);
    } else {
        println!("{}", crate::doctor::render(&report, ctx.style));
    }
    Ok(match report.grade() {
        Grade::Ok => CLEAN,
        Grade::Soft => LAPSE,
        Grade::Broken => INCOMPLETE,
    })
}

pub fn banner(ctx: &Context) -> Result<u8> {
    // Decoration, and decoration fails silent: a reminder that cannot read its
    // own cache prints nothing rather than a diagnostic in every new shell.
    let due = Cache::load(&ctx.paths.cache()).map_or(0, |cache| cache.due_by(ctx.today()));

    // The nag is not the health check. This fires the day a drill comes due;
    // doctor waits out a grace, because a drill taken a day late is a drill
    // taken. coop reads this one, and assay's registry station reads that one.
    if ctx.format == Format::Json {
        let station = StationId::from_static("rote");
        let findings = if due == 0 {
            Vec::new()
        } else {
            vec![
                station
                    .soft(Summary::lossy(&format!(
                        "{} due",
                        relic_core::fmt::plural(due, "drill", "drills")
                    )))
                    .fixed_by(FixHint::lossy("rote")),
            ]
        };
        println!("{}", json::document(&Report::ran(station, findings))?);
        return Ok(CLEAN);
    }

    if due == 0 {
        return Ok(CLEAN);
    }
    let text = format!(
        "rote: {} due.",
        relic_core::fmt::plural(due, "drill", "drills")
    );
    println!("{} {}", ctx.style.bold("==>"), ctx.style.yellow(&text));
    Ok(CLEAN)
}
