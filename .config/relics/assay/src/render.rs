//! Three shapes for one report set.
//!
//! The verdict comes **last** in every human-facing block, because that is how a
//! terminal block is read: from the bottom up, with the remedy above the line
//! that grades it. `check-bedrock` and `yadm doctor` already print that way, and
//! A2 replaces them without moving anyone's eye.

use std::fmt::Write as _;
use std::io::Write;

use anyhow::Result;
use relic_core::finding::{Finding, Grade, Outcome, Report, Severity, StationId};
use relic_core::fmt::plural;
use relic_core::ui::Format;

/// How the run is written out.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// Which shape.
    pub format: Format,
    /// Whether to spend on colour.
    pub color: bool,
    /// Print nothing at all when the run is clean. The dream pre-pass and
    /// `yadm update` both run this way.
    pub quiet: bool,
}

/// Writes the run.
///
/// # Errors
///
/// When the sink refuses the write, or a report will not serialize.
pub fn report(out: &mut impl Write, reports: &[Report], style: Style) -> Result<()> {
    let grade = Grade::across(reports);
    if style.quiet && grade == Grade::Ok {
        return Ok(());
    }
    match style.format {
        Format::Human => human(out, reports, grade, style.color),
        Format::Agent => agent(out, reports, grade),
        Format::Json => json(out, reports, grade),
    }
}

/// The roster, for `--list`.
///
/// # Errors
///
/// When the sink refuses the write.
pub fn list(out: &mut impl Write, stations: &[(&str, &str)], format: Format) -> Result<()> {
    match format {
        Format::Json => {
            let rows: Vec<_> = stations
                .iter()
                .map(|(id, title)| serde_json::json!({ "station": id, "title": title }))
                .collect();
            writeln!(out, "{}", serde_json::to_string_pretty(&rows)?)?;
        }
        Format::Human | Format::Agent => {
            let width = stations.iter().map(|(id, _)| id.len()).max().unwrap_or(0);
            for (id, title) in stations {
                writeln!(out, "{id:<width$}  {title}")?;
            }
        }
    }
    Ok(())
}

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";
const YELLOW: &str = "\x1b[33m";

fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("{code}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

fn tally(reports: &[Report]) -> (usize, usize) {
    let mut broken = 0;
    let mut soft = 0;
    for finding in reports.iter().flat_map(Report::findings) {
        match finding.severity {
            Severity::Broken => broken += 1,
            Severity::Soft => soft += 1,
            // Notes are printed and never counted; counting them into a verdict
            // is exactly what makes a gate get switched off.
            Severity::Note => {}
        }
    }
    (broken, soft)
}

fn human(out: &mut impl Write, reports: &[Report], grade: Grade, color: bool) -> Result<()> {
    let width = reports
        .iter()
        .map(|report| report.station.as_str().len())
        .max()
        .unwrap_or(0);

    for report in reports {
        let name = report.station.as_str();
        match &report.outcome {
            Outcome::Skipped(reason) => {
                writeln!(
                    out,
                    "  {name:<width$}  {}",
                    paint(&format!("skipped — {reason}"), DIM, color)
                )?;
            }
            Outcome::Ran(findings) if findings.is_empty() => {
                writeln!(out, "  {name:<width$}  {}", paint("ok", GREEN, color))?;
            }
            Outcome::Ran(findings) => {
                writeln!(out, "  {}", paint(name, BOLD, color))?;
                for finding in findings {
                    human_finding(out, finding, &report.station, color)?;
                }
            }
        }
    }

    let (broken, soft) = tally(reports);
    let verdict = match grade {
        Grade::Ok => paint("==> assay ok", GREEN, color),
        Grade::Soft => paint(
            &format!("==> assay ok with {}", plural(soft, "warning", "warnings")),
            YELLOW,
            color,
        ),
        Grade::Broken => paint(
            &format!(
                "!!> assay INCOMPLETE — {}, {}",
                plural(broken, "failure", "failures"),
                plural(soft, "warning", "warnings")
            ),
            RED,
            color,
        ),
    };
    writeln!(out, "\n{verdict}")?;
    Ok(())
}

fn human_finding(
    out: &mut impl Write,
    finding: &Finding,
    reported_by: &StationId,
    color: bool,
) -> Result<()> {
    let (label, code) = match finding.severity {
        Severity::Broken => ("broken", RED),
        Severity::Soft => ("soft  ", YELLOW),
        Severity::Note => ("note  ", DIM),
    };
    // A station usually mints its own findings, so the two agree and naming the
    // author twice would be noise. The registry adapter is the exception: it
    // reports what other programs said, and which program said it is the first
    // thing a reader needs.
    let author = if finding.station == *reported_by {
        String::new()
    } else {
        format!("{} ", paint(&format!("[{}]", finding.station), BOLD, color))
    };
    writeln!(
        out,
        "    {}  {author}{}",
        paint(label, code, color),
        finding.summary
    )?;
    if let Some(detail) = &finding.detail {
        for line in detail.as_str().lines() {
            writeln!(out, "            {}", paint(line, DIM, color))?;
        }
    }
    if let Some(location) = &finding.location {
        writeln!(
            out,
            "            {}",
            paint(&location.to_string(), DIM, color)
        )?;
    }
    if let Some(fix) = &finding.fix {
        writeln!(
            out,
            "            {}",
            paint(&format!("fix: {fix}"), DIM, color)
        )?;
    }
    Ok(())
}

fn agent(out: &mut impl Write, reports: &[Report], grade: Grade) -> Result<()> {
    for report in reports {
        let name = report.station.as_str();
        match &report.outcome {
            Outcome::Skipped(reason) => writeln!(out, "skip\t{name}\t{reason}")?,
            Outcome::Ran(findings) if findings.is_empty() => writeln!(out, "ok\t{name}")?,
            Outcome::Ran(findings) => {
                for finding in findings {
                    // The finding's own station, not the report's: for every
                    // built-in station they are the same name, and for the
                    // registry adapter this is the speaker rather than the
                    // collector.
                    let name = finding.station.as_str();
                    let mut line = format!("{}\t{name}\t{}", finding.severity, finding.summary);
                    if let Some(location) = &finding.location {
                        let _ = write!(line, "\tat={location}");
                    }
                    if let Some(fix) = &finding.fix {
                        let _ = write!(line, "\tfix={fix}");
                    }
                    writeln!(out, "{line}")?;
                    if let Some(detail) = &finding.detail {
                        for text in detail.as_str().lines() {
                            writeln!(out, "  {text}")?;
                        }
                    }
                }
            }
        }
    }
    let (broken, soft) = tally(reports);
    writeln!(out, "grade\t{grade}\tbroken={broken}\tsoft={soft}")?;
    Ok(())
}

fn json(out: &mut impl Write, reports: &[Report], grade: Grade) -> Result<()> {
    let (broken, soft) = tally(reports);
    let document = serde_json::json!({
        "grade": grade,
        "broken": broken,
        "soft": soft,
        "reports": reports,
    });
    writeln!(out, "{}", serde_json::to_string_pretty(&document)?)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use relic_core::finding::{StationId, Summary};

    use super::*;

    fn id(name: &str) -> StationId {
        name.parse().expect("a valid id")
    }

    fn line(text: &str) -> Summary {
        text.parse().expect("one line")
    }

    fn rendered(reports: &[Report], style: Style) -> String {
        let mut out = Vec::new();
        report(&mut out, reports, style).expect("rendered");
        String::from_utf8(out).expect("utf-8")
    }

    fn style(format: Format) -> Style {
        Style {
            format,
            color: false,
            quiet: false,
        }
    }

    #[test]
    fn a_quiet_clean_run_says_nothing_at_all() {
        let reports = vec![Report::ran(id("bedrock"), vec![])];
        let quiet = Style {
            quiet: true,
            ..style(Format::Human)
        };
        assert_eq!(rendered(&reports, quiet), "");
    }

    #[test]
    fn a_quiet_run_with_a_finding_still_speaks() {
        let station = id("bedrock");
        let reports = vec![Report::ran(station.clone(), vec![station.soft(line("hm"))])];
        let quiet = Style {
            quiet: true,
            ..style(Format::Human)
        };
        assert!(rendered(&reports, quiet).contains("hm"));
    }

    #[test]
    fn the_verdict_is_the_last_line_a_human_reads() {
        let station = id("bedrock");
        let reports = vec![Report::ran(
            station.clone(),
            vec![station.broken(line("bash 5 is not on PATH"))],
        )];
        let text = rendered(&reports, style(Format::Human));
        let last = text.trim_end().lines().last().expect("a verdict");
        assert!(last.starts_with("!!> assay INCOMPLETE"), "{last}");
        assert!(last.contains("1 failure, 0 warnings"), "{last}");
    }

    #[test]
    fn a_clean_human_run_still_prints_a_verdict() {
        let reports = vec![Report::ran(id("bedrock"), vec![])];
        let text = rendered(&reports, style(Format::Human));
        assert!(text.contains("bedrock"), "{text}");
        assert!(text.trim_end().ends_with("==> assay ok"), "{text}");
    }

    #[test]
    fn colour_is_escapes_or_nothing() {
        let reports = vec![Report::ran(id("bedrock"), vec![])];
        let plain = rendered(&reports, style(Format::Human));
        let painted = rendered(
            &reports,
            Style {
                color: true,
                ..style(Format::Human)
            },
        );
        assert!(!plain.contains('\x1b'), "{plain}");
        assert!(painted.contains('\x1b'), "{painted}");
    }

    #[test]
    fn the_agent_shape_is_one_record_per_line() {
        let station = id("brew-health");
        let reports = vec![
            Report::skipped(id("bedrock"), line("nothing to do")),
            Report::ran(
                station.clone(),
                vec![
                    station
                        .soft(line("sentry-cli is deprecated"))
                        .fixed_by("cask it".parse().expect("one line")),
                ],
            ),
        ];
        let text = rendered(&reports, style(Format::Agent));
        let lines: Vec<_> = text.lines().collect();
        assert_eq!(lines.first().copied(), Some("skip\tbedrock\tnothing to do"));
        assert_eq!(
            lines.get(1).copied(),
            Some("soft\tbrew-health\tsentry-cli is deprecated\tfix=cask it")
        );
        assert_eq!(lines.last().copied(), Some("grade\tsoft\tbroken=0\tsoft=1"));
    }

    #[test]
    fn the_json_shape_carries_the_grade_and_the_reports() {
        let station = id("bedrock");
        let reports = vec![Report::ran(
            station.clone(),
            vec![station.broken(line("gone"))],
        )];
        let text = rendered(&reports, style(Format::Json));
        let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
        assert_eq!(parsed["grade"], "broken");
        assert_eq!(parsed["broken"], 1);
        assert_eq!(parsed["reports"][0]["station"], "bedrock");
    }

    #[test]
    fn an_empty_roster_grades_ok_rather_than_saying_nothing() {
        let text = rendered(&[], style(Format::Agent));
        assert_eq!(text, "grade\tok\tbroken=0\tsoft=0\n");
    }
}
