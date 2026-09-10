//! One row model, three readers.

use anyhow::{Context, Result};
use relic_core::ui::Format;
use serde::Serialize;

use crate::ask::State;
use crate::cmd::{Ctx, Gathered};
use crate::notice::Notice;
use crate::source::Tier;

#[derive(Serialize)]
struct Row<'a> {
    source: &'a str,
    severity: String,
    summary: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    fix: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    detail: Option<&'a str>,
}

fn row(notice: &Notice) -> Row<'_> {
    Row {
        source: &notice.source,
        severity: notice.finding.severity.to_string(),
        summary: notice.finding.summary.as_str(),
        fix: notice
            .finding
            .fix
            .as_ref()
            .map(relic_core::finding::FixHint::as_str),
        detail: notice
            .finding
            .detail
            .as_ref()
            .map(relic_core::finding::Detail::as_str),
    }
}

/// One line per outstanding notice.
///
/// # Errors
///
/// When the JSON shape cannot be serialised.
pub fn list(ctx: &Ctx, notices: &[Notice]) -> Result<()> {
    let rows: Vec<Row<'_>> = notices.iter().map(row).collect();
    match ctx.format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&rows).context("serialising notices")?
        ),
        Format::Agent | Format::Human => {
            for row in &rows {
                let fix = row
                    .fix
                    .map(|fix| format!("  fix: {fix}"))
                    .unwrap_or_default();
                println!("{}  {}  {}{fix}", row.severity, row.source, row.summary);
            }
        }
    }
    Ok(())
}

/// One notice, with whatever its producer supplied.
///
/// # Errors
///
/// When the JSON shape cannot be serialised.
pub fn show(ctx: &Ctx, notices: &[&Notice]) -> Result<()> {
    if ctx.format == Format::Json {
        let rows: Vec<Row<'_>> = notices.iter().map(|notice| row(notice)).collect();
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).context("serialising notices")?
        );
        return Ok(());
    }
    for notice in notices {
        println!("{}  {}", notice.finding.severity, notice.finding.summary);
        if let Some(detail) = &notice.finding.detail {
            for line in detail.as_str().lines() {
                println!("        {line}");
            }
        }
        if let Some(fix) = &notice.finding.fix {
            println!("        fix: {fix}");
        }
    }
    Ok(())
}

#[derive(Serialize)]
struct SourceRow<'a> {
    id: &'a str,
    tier: &'static str,
    state: &'static str,
    outstanding: usize,
}

/// Every declared source, and what it is doing.
///
/// # Errors
///
/// When the JSON shape cannot be serialised.
pub fn sources(ctx: &Ctx, gathered: &Gathered) -> Result<()> {
    let rows: Vec<SourceRow<'_>> = gathered
        .sources
        .iter()
        .map(|source| {
            let id = source.id.as_str();
            SourceRow {
                id,
                tier: match source.tier {
                    Tier::When(_) => "when",
                    Tier::Ask(_) => "ask",
                },
                state: gathered
                    .states
                    .iter()
                    .find(|(named, _)| named == id)
                    .map_or("-", |(_, state)| name(*state)),
                outstanding: gathered
                    .notices
                    .iter()
                    .filter(|notice| notice.source == id)
                    .count(),
            }
        })
        .collect();

    if ctx.format == Format::Json {
        println!(
            "{}",
            serde_json::to_string_pretty(&rows).context("serialising sources")?
        );
        return Ok(());
    }
    for row in &rows {
        println!(
            "{}  {}  {}  {}",
            row.tier, row.state, row.outstanding, row.id
        );
    }
    for entry in &gathered.broken {
        println!("broken  -  0  {}  ({})", entry.path, entry.why);
    }
    Ok(())
}

fn name(state: State) -> &'static str {
    match state {
        State::Fresh => "fresh",
        State::Stale => "stale",
        State::Cold => "cold",
        State::Dormant => "dormant",
    }
}
