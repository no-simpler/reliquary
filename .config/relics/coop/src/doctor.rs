//! What coop knows about its own health, in the vocabulary `assay` collects.
//!
//! Every producer is asked live, the same way the tick asks it, so a standing
//! here is a measurement rather than a record.

use relic_core::finding::{
    Detail, Finding, FixHint, Grade, Outcome, Report, Severity, StationId, Summary,
};

use crate::ask::{self, ASK_BUDGET, Standing};
use crate::cmd::{Ctx, Gathered};
use crate::records::{FirstSeen, HORIZON_DAYS};
use crate::source::{Broken, Tier};

fn station() -> StationId {
    StationId::from_static("coop")
}

fn sources_station() -> StationId {
    StationId::from_static("coop-sources")
}

fn summary(text: &str) -> Summary {
    Summary::lossy(text)
}

/// Read coop's own state.
pub fn report(ctx: &Ctx, gathered: &Gathered) -> Report {
    let mut findings = Vec::new();
    findings.extend(declaration_findings(&gathered.broken));
    findings.extend(fixless(gathered));
    findings.extend(dormant(gathered));
    findings.extend(failing(gathered));
    findings.extend(slow(gathered));
    findings.extend(overdue(ctx, gathered));
    findings.extend(furniture(ctx, gathered));
    Report::ran(station(), findings)
}

/// A declaration that will not compile guards nothing, and the shell it was
/// meant to warn is the last place that can say so.
fn declaration_findings(broken: &[Broken]) -> Vec<Finding> {
    if broken.is_empty() {
        return Vec::new();
    }
    let detail: Vec<String> = broken
        .iter()
        .map(|entry| format!("{}: {}", entry.path, entry.why))
        .collect();
    vec![
        sources_station()
            .broken(summary(
                "a declaration will not compile, so its source is silent",
            ))
            .detailed_with(Detail::new(detail.join("\n")))
            .fixed_by(FixHint::lossy("coop sources")),
    ]
}

/// Every notice is a nag built to be dismissed, and one that cannot say how is
/// furniture the moment it is read.
fn fixless(gathered: &Gathered) -> Vec<Finding> {
    let mut named: Vec<String> = gathered
        .notices
        .iter()
        .filter(|notice| notice.finding.fix.is_none())
        .map(|notice| notice.source.clone())
        .collect();
    named.sort_unstable();
    named.dedup();
    let produced: Vec<String> = named
        .into_iter()
        .filter(|id| {
            gathered
                .sources
                .iter()
                .any(|source| source.id.as_str() == id && !matches!(source.tier, Tier::When(_)))
        })
        .collect();
    if produced.is_empty() {
        return Vec::new();
    }
    vec![
        sources_station()
            .soft(summary("a source is producing notices nobody can act on"))
            .detailed_with(Detail::new(produced.join("\n")))
            .fixed_by(FixHint::lossy(
                "give the producer a fix hint, or drop the source",
            )),
    ]
}

fn standing(gathered: &Gathered, wanted: Standing) -> Vec<String> {
    gathered
        .standings
        .iter()
        .filter(|(_, standing, _)| *standing == wanted)
        .map(|(id, _, why)| match why {
            Some(why) => format!("{id}: {why}"),
            None => id.clone(),
        })
        .collect()
}

/// A declaration whose producer is not here. Reported, never graded: a tracked
/// declaration arriving on a machine that does not run that producer is the
/// system working.
fn dormant(gathered: &Gathered) -> Vec<Finding> {
    let sleeping = standing(gathered, Standing::Dormant);
    if sleeping.is_empty() {
        return Vec::new();
    }
    vec![
        sources_station()
            .note(summary("a declared source has no producer on this machine"))
            .detailed_with(Detail::new(sleeping.join("\n"))),
    ]
}

/// A producer that will not answer. Not on the card: a broken producer is not
/// a nag, and the person at the prompt cannot tell from a card what went wrong.
fn failing(gathered: &Gathered) -> Vec<Finding> {
    let detail = standing(gathered, Standing::Failing);
    if detail.is_empty() {
        return Vec::new();
    }
    vec![
        sources_station()
            .soft(summary("a source answered with a failure"))
            .detailed_with(Detail::new(detail.join("\n")))
            .fixed_by(FixHint::lossy(
                "run what the source declares, and read what it says",
            )),
    ]
}

/// The tick runs before every prompt, so a producer over its budget is the
/// one thing that can quietly make the whole machine feel slow.
fn slow(gathered: &Gathered) -> Vec<Finding> {
    let detail = standing(gathered, Standing::Slow);
    if detail.is_empty() {
        return Vec::new();
    }
    vec![
        sources_station()
            .soft(summary("a producer is over the prompt budget"))
            .detailed_with(Detail::new(detail.join("\n")))
            .fixed_by(FixHint::lossy(&format!(
                "make it answer within {}ms, or move it to the read tier",
                ASK_BUDGET.as_millis()
            ))),
    ]
}

/// A read report older than its producer promised means the producer has
/// stopped running, and the card is showing whatever it last said.
fn overdue(ctx: &Ctx, gathered: &Gathered) -> Vec<Finding> {
    let detail: Vec<String> = gathered
        .sources
        .iter()
        .filter_map(|source| match &source.tier {
            Tier::Read(read) => ask::overdue(read, ctx.now)
                .map(|age| format!("{}: last written {age} ago", source.id)),
            Tier::When(_) | Tier::Ask(_) => None,
        })
        .collect();
    if detail.is_empty() {
        return Vec::new();
    }
    vec![
        sources_station()
            .soft(summary("a read report is older than its producer promised"))
            .detailed_with(Detail::new(detail.join("\n")))
            .fixed_by(FixHint::lossy(
                "run the producer, or fix whatever schedules it",
            )),
    ]
}

/// A notice that will not go away stopped being a nag some time ago.
fn furniture(ctx: &Ctx, gathered: &Gathered) -> Vec<Finding> {
    let record = FirstSeen::load(&ctx.paths.first_seen());
    let overstayed = record.overstayed(&gathered.notices, ctx.now);
    if overstayed.is_empty() {
        return Vec::new();
    }
    let detail: Vec<String> = overstayed
        .iter()
        .map(|(id, days)| format!("{id}: outstanding {days} days"))
        .collect();
    vec![
        station()
            .soft(summary(
                "a notice has outstayed the horizon and reads as furniture",
            ))
            .detailed_with(Detail::new(detail.join("\n")))
            .fixed_by(FixHint::lossy(&format!(
                "act on it, or retire a source nothing acts on within {HORIZON_DAYS} days"
            ))),
    ]
}

/// The human shape: one line per finding, with the verdict last.
pub fn render(report: &Report, paint: relic_core::style::Style) -> String {
    use relic_core::style::Tint;

    let findings = match &report.outcome {
        Outcome::Ran(findings) => findings.as_slice(),
        Outcome::Skipped(reason) => return format!("coop: skipped — {reason}"),
    };
    let mut lines = Vec::new();
    for finding in findings {
        let tint = match finding.severity {
            Severity::Broken => Tint::Red,
            Severity::Soft => Tint::Yellow,
            Severity::Note => Tint::Dim,
        };
        lines.push(format!(
            "{}  {}",
            paint.paint(tint, &finding.severity.to_string()),
            finding.summary
        ));
        if let Some(detail) = &finding.detail {
            for line in detail.as_str().lines() {
                lines.push(paint.dim(&format!("        {line}")));
            }
        }
        if let Some(fix) = &finding.fix {
            lines.push(paint.dim(&format!("        fix: {fix}")));
        }
    }
    lines.push(match report.grade() {
        Grade::Ok => paint.green("==> coop ok"),
        Grade::Soft => paint.yellow(&format!(
            "!!> coop degraded — {} to look at",
            findings.len()
        )),
        Grade::Broken => paint.bold(&format!("!!> coop BROKEN — {} to look at", findings.len())),
    });
    lines.join("\n")
}
