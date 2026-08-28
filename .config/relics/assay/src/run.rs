//! Running the roster.

use relic_core::finding::{Detail, FixHint, Grade, Outcome, Report, Severity, Summary};

use crate::station::{Context, Station};

/// Runs every station given and collects what they said.
///
/// The run never stops early. A station that returns `Err` becomes a `Broken`
/// finding naming the station, because the alternative — reporting a crashed
/// check as no findings — is exactly the silent pass the surface exists to
/// prevent. Nothing here inspects a finding to decide the run's fate; that is
/// [`Grade::across`], derived at the end from what came back.
pub fn run(stations: &[Box<dyn Station>], cx: &Context) -> Vec<Report> {
    stations
        .iter()
        .map(|station| match station.check(cx) {
            Ok(outcome) => Report {
                station: station.id().clone(),
                outcome: stale(station.as_ref(), cx, outcome),
            },
            Err(error) => {
                let summary = Summary::lossy(&format!(
                    "the {} station could not run: {error}",
                    station.id()
                ));
                let chain = error
                    .chain()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join("\n  caused by: ");
                let mut finding = station.id().broken(summary);
                if let Some(detail) = Detail::new(chain) {
                    finding = finding.detailed(detail);
                }
                Report::ran(station.id().clone(), vec![finding])
            }
        })
        .collect()
}

/// Add the derivation-drift note to a station's own outcome, when it has one.
///
/// Appended to the station that owns the transcription rather than raised as the
/// runner's own finding: the reader's next move is to re-derive *that* table,
/// and a finding attributed to the runner would make them go looking for which.
///
/// A `Note`, never a verdict. The machine is not degraded because a checker was
/// written against an older release — most upgrades change nothing the table
/// describes. What has degraded is the confidence in the check, which is exactly
/// the "worth reading, does not grade" case.
///
/// A skipped station stays skipped: it did not consult its table, so whether the
/// table is current says nothing.
fn stale(station: &dyn Station, cx: &Context, outcome: Outcome) -> Outcome {
    let Outcome::Ran(mut findings) = outcome else {
        return outcome;
    };
    let Some(derivation) = station.derived_from() else {
        return Outcome::Ran(findings);
    };
    let Some(installed) = (derivation.installed)(cx) else {
        return Outcome::Ran(findings);
    };
    if installed == derivation.version {
        return Outcome::Ran(findings);
    }
    findings.push(
        station
            .id()
            .finds(
                Severity::Note,
                Summary::lossy(&format!(
                    "these rules were read against {} {}; {installed} is installed",
                    derivation.artefact, derivation.version
                )),
            )
            .detailed_with(Detail::new(derivation.recipe))
            .fixed_by(FixHint::lossy(
                "re-derive the table, then record the version it was read against",
            )),
    );
    Outcome::Ran(findings)
}

/// What the run amounts to.
pub fn grade(reports: &[Report]) -> Grade {
    Grade::across(reports)
}

#[cfg(test)]
mod tests {
    use anyhow::{Result, anyhow};
    use relic_core::finding::{Outcome, Severity, StationId};

    use super::*;
    use crate::station::Station;

    struct Fixture {
        id: StationId,
        answer: fn() -> Result<Outcome>,
    }

    impl Station for Fixture {
        fn id(&self) -> &StationId {
            &self.id
        }
        fn title(&self) -> &'static str {
            "a fixture"
        }
        fn check(&self, _cx: &Context) -> Result<Outcome> {
            (self.answer)()
        }
    }

    fn station(id: &str, answer: fn() -> Result<Outcome>) -> Box<dyn Station> {
        Box::new(Fixture {
            id: id.parse().expect("a valid id"),
            answer,
        })
    }

    fn cx() -> Context {
        Context::new("/nowhere", Vec::new())
    }

    #[test]
    fn a_clean_run_grades_ok() {
        let reports = run(&[station("clean", || Ok(Outcome::Ran(Vec::new())))], &cx());
        assert_eq!(grade(&reports), Grade::Ok);
    }

    #[test]
    fn a_station_that_throws_becomes_a_broken_finding_not_a_silent_pass() {
        let reports = run(
            &[station("thrower", || {
                Err(anyhow!("the file was unreadable").context("reading the manifest"))
            })],
            &cx(),
        );
        assert_eq!(grade(&reports), Grade::Broken);
        let finding = reports
            .first()
            .and_then(|report| report.findings().first())
            .expect("a finding");
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("thrower"), "{finding:?}");
        let detail = finding.detail.as_ref().expect("the cause chain");
        assert!(detail.as_str().contains("unreadable"), "{detail}");
    }

    #[test]
    fn a_thrown_station_does_not_stop_the_ones_after_it() {
        let reports = run(
            &[
                station("thrower", || Err(anyhow!("no"))),
                station("later", || Ok(Outcome::Ran(Vec::new()))),
            ],
            &cx(),
        );
        assert_eq!(reports.len(), 2);
        assert_eq!(reports.get(1).map(Report::grade), Some(Grade::Ok));
    }

    #[test]
    fn a_skip_does_not_move_the_grade() {
        let reports = run(
            &[station("skipper", || {
                Ok(Outcome::Skipped(Summary::lossy("nothing to check here")))
            })],
            &cx(),
        );
        assert_eq!(grade(&reports), Grade::Ok);
    }
}
