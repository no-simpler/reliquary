//! What every binary on `PATH` says about its own health.
//!
//! `assay` is an aggregator, not a tool that absorbs everything. A relic knows
//! its own invariants better than any central checker could, and that knowledge
//! belongs where it is maintained. So this station does not check anything: it
//! **asks**, and folds the answers into the one report.
//!
//! The question is `<name> doctor --format json`, put to every name in
//! `~/.local/bin/.reliquary-managed` — the registry that already exists,
//! written by `install-on-path.sh` and listing every binary a meta-project has
//! published, with its owner. That file is the whole interface. `assay` never
//! reads another repository's source, so a Stage-3 relic in its own repo
//! answers on exactly the same terms as one in this workspace.
//!
//! **A binary that does not answer is a fact, not a fault.** Most do not: the
//! protocol is new, and nineteen of twenty-one registered binaries have no
//! `doctor` subcommand at all. Reporting that as a problem would fill the
//! report with things nobody is going to do anything about, so it is silent.
//! What is *not* silent is a binary that answers with something shaped like an
//! answer and is not one, and a binary that does not answer at all — those are
//! the two states where something is wrong rather than absent.
//!
//! **The probe is bounded.** A registered binary is an arbitrary program; one
//! that never returns would hang the standing audit that runs from `yadm
//! update` and from the dream pre-pass. [`Tool::run_within`] kills it and
//! says so. The probes run concurrently for the same reason — the wall clock is
//! one probe, not twenty-one.
//!
//! **Safety, checked rather than assumed** (2026-08-28): of the eleven
//! registered binaries this workspace does not own, ten have no `doctor`
//! subcommand, and the eleventh — halo's `dewey` — has one whose own test is
//! named `test_doctor_reports_and_never_writes`, and which refuses
//! `--format json` with a usage error. Nothing is mutated by asking.

use std::collections::BTreeMap;
use std::time::Duration;

use anyhow::Result;
use camino::Utf8PathBuf;
use relic_core::finding::{Detail, FixHint, Location, Outcome, Report, StationId, Summary};
use relic_core::tool::{self, Tool};

use crate::station::{Context, Station};

/// The registry, `$HOME`-relative. One `<name>[<TAB><owner>]` per line.
const REGISTRY: &str = ".local/bin/.reliquary-managed";

/// The question every speaker answers.
const PROBE: &[&str] = &["doctor", "--format", "json"];

/// How long one binary has to answer.
///
/// Short, and that is the point: a health report is data the program already
/// holds, and a binary without the subcommand refuses in milliseconds. Anything
/// still working after this is doing something other than answering — which is
/// its own finding to make, in its own report, and not something this station
/// can wait out.
///
/// The budget is what made `ske` fit. Measured 2026-08-28 it ran for **20.3 s**,
/// and 89.65 s by the time it was ported; a budget generous enough to sit
/// through that would have put a minute onto every standing audit, which is how
/// a station gets switched off. It answers in ~130 ms now, and it is the
/// protocol's first speaker.
const BUDGET: Duration = Duration::from_secs(2);

/// The station.
pub struct RegistryAdapter {
    id: StationId,
    /// How long one binary has to answer. A field rather than the constant, so
    /// a test can prove the bound in milliseconds instead of spending the
    /// shipped budget to do it.
    budget: Duration,
}

impl Default for RegistryAdapter {
    fn default() -> Self {
        Self {
            id: StationId::from_static("registry"),
            budget: BUDGET,
        }
    }
}

impl Station for RegistryAdapter {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "every registered binary's own account of its health"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let path = cx.at(REGISTRY);
        let Some(registry) = Registry::read(&path)? else {
            return Ok(Outcome::Skipped(Summary::lossy(&format!(
                "{REGISTRY} is not there, so nothing is registered on PATH"
            ))));
        };
        if registry.entries.is_empty() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "the registry is empty, so there is nobody to ask",
            )));
        }

        let answers = ask_all(&registry.entries, cx, self.budget);
        let mut findings = Vec::new();
        let mut spoke = 0_usize;
        for (entry, answer) in registry.entries.iter().zip(answers) {
            match answer {
                Answer::Spoke(report) => {
                    spoke += 1;
                    findings.extend(report.findings().iter().cloned());
                }
                Answer::Silent => {}
                Answer::Malformed(why) => findings.push(
                    self.id
                        .note(Summary::lossy(&format!(
                            "{} answered `doctor --format json` with something that is not a report",
                            entry.name
                        )))
                        .detailed_with(Detail::new(why))
                        .at(Location::file(REGISTRY))
                        .fixed_by(FixHint::lossy(
                            "have it print a relic-core finding report, or no JSON at all",
                        )),
                ),
                // A `Note`, because from outside a program there is no telling
                // a hang from slow work, and both mean the same thing here:
                // this one could not be judged. It is the per-item counterpart
                // of a skip, and it grades the same way — nothing.
                Answer::Hung => findings.push(
                    self.id
                        .note(Summary::lossy(&format!(
                            "{} was still running `doctor` after {}ms and was stopped, so nothing could be collected from it",
                            entry.name,
                            self.budget.as_millis()
                        )))
                        .at(Location::file(REGISTRY))
                        .fixed_by(FixHint::lossy(
                            "a doctor that answers slowly cannot be aggregated — make it answer, or it stays uncollected",
                        )),
                ),
            }
        }

        if spoke == 0 && findings.is_empty() {
            return Ok(Outcome::Skipped(Summary::lossy(&format!(
                "none of the {} registered binaries answers `doctor --format json` yet",
                registry.entries.len()
            ))));
        }
        Ok(Outcome::Ran(findings))
    }
}

/// What one binary said when it was asked.
#[derive(Debug)]
enum Answer {
    /// It answered, and the answer is a report.
    Spoke(Box<Report>),
    /// It does not speak the protocol: no such subcommand, no such binary, a
    /// refusal, or output that is not JSON at all. Not a finding.
    Silent,
    /// It printed JSON, and the JSON is not a report — so it meant to answer
    /// and got it wrong, which is worth saying.
    Malformed(String),
    /// It outlasted its budget and was killed.
    Hung,
}

/// One line of the registry.
#[derive(Debug, PartialEq, Eq)]
struct Entry {
    /// The published name, which is also the binary to run.
    name: String,
    /// The meta-project that published it, when the line says.
    owner: Option<String>,
}

/// The registry, parsed.
#[derive(Debug, Default, PartialEq, Eq)]
struct Registry {
    entries: Vec<Entry>,
}

impl Registry {
    /// The registry at `path`, or nothing when there is none.
    ///
    /// Absence is not a failure: a machine that has published nothing has no
    /// registry, and `install-on-path.sh` writes it on the first publish.
    fn read(path: &camino::Utf8Path) -> Result<Option<Self>> {
        match fs_err::read_to_string(path.as_std_path()) {
            Ok(text) => Ok(Some(Self::parse(&text))),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(error) => Err(anyhow::anyhow!(error).context(format!("reading {path}"))),
        }
    }

    /// `#` comments and blank lines are ignored, and membership is keyed on the
    /// first field — the same reading `install-on-path.sh` documents.
    fn parse(text: &str) -> Self {
        let mut entries = Vec::new();
        let mut seen = Vec::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut fields = line.split('\t');
            let Some(name) = fields.next().map(str::trim).filter(|name| !name.is_empty()) else {
                continue;
            };
            if seen.contains(&name.to_owned()) {
                continue;
            }
            seen.push(name.to_owned());
            entries.push(Entry {
                name: name.to_owned(),
                owner: fields
                    .next()
                    .map(str::trim)
                    .filter(|owner| !owner.is_empty())
                    .map(ToOwned::to_owned),
            });
        }
        Self { entries }
    }
}

/// Ask everyone at once, and give the answers back in registry order.
///
/// Concurrent because the cost is twenty-one process starts, several of them
/// interpreters: serially that is seconds on the path `yadm update` and the
/// dream pre-pass both take. Order is restored afterwards, because a report
/// that reshuffles between runs is a report nobody can diff.
fn ask_all(entries: &[Entry], cx: &Context, budget: Duration) -> Vec<Answer> {
    let mut answers: BTreeMap<usize, Answer> = std::thread::scope(|scope| {
        let handles: Vec<_> = entries
            .iter()
            .enumerate()
            .map(|(index, entry)| scope.spawn(move || (index, ask(entry, cx, budget))))
            .collect();
        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .collect()
    });
    (0..entries.len())
        .map(|index| answers.remove(&index).unwrap_or(Answer::Silent))
        .collect()
}

/// Put the question to one binary.
fn ask(entry: &Entry, cx: &Context, budget: Duration) -> Answer {
    let Some(program) = crate::probe::resolve(&entry.name, cx.path()) else {
        // Registered and not on PATH is real drift, and it is `relic doctor`'s
        // finding to report — this station only collects. Asking a binary that
        // is not there is simply not possible.
        return Answer::Silent;
    };
    let tool = Tool::at_path(&entry.name, Utf8PathBuf::into_std_path_buf(program));
    let mut command = tool.command();
    command.args(PROBE);
    // `run_within`, not `capture_within`: the exit status is **the grade of the
    // answer**, not a verdict on whether there was one. A speaker that found
    // something exits 1 or 2 by contract, so reading non-zero as a failure
    // discards every report that had anything in it — which is precisely what
    // the first draft of this station did.
    match tool.run_within(&mut command, budget) {
        Ok(exit) => interpret(&entry.name, &exit.stdout),
        Err(tool::Error::TimedOut { .. }) => Answer::Hung,
        // Not being runnable is how a name that is registered and not really
        // there says so.
        Err(
            tool::Error::Spawn { .. } | tool::Error::Failed { .. } | tool::Error::NotUtf8 { .. },
        ) => Answer::Silent,
    }
}

/// What a binary's stdout amounts to.
///
/// The discrimination that matters: output that is not JSON at all is a program
/// that never meant to answer, and output that is JSON but not a report is one
/// that did and got it wrong. Only the second is worth a word.
fn interpret(name: &str, stdout: &str) -> Answer {
    if stdout.trim().is_empty() {
        return Answer::Silent;
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(stdout) else {
        return Answer::Silent;
    };
    let report: Report = match serde_json::from_value(value) {
        Ok(report) => report,
        Err(error) => return Answer::Malformed(error.to_string()),
    };
    match namespaced(name, &report) {
        Ok(()) => Answer::Spoke(Box::new(report)),
        Err(why) => Answer::Malformed(why),
    }
}

/// Whether a report is about the binary that printed it.
///
/// A station id may be the binary's name or a name under it — `docket`,
/// `docket-git` — and nothing else. Findings are minted through a `StationId`
/// locally so a station cannot stamp another station's name on its own report;
/// across a process boundary that cannot be enforced, only checked, and this is
/// the check.
fn namespaced(name: &str, report: &Report) -> Result<(), String> {
    let owns = |id: &StationId| {
        let id = id.as_str();
        id == name
            || id
                .strip_prefix(name)
                .is_some_and(|rest| rest.starts_with('-'))
    };
    if !owns(&report.station) {
        return Err(format!(
            "it reported as station {:?}, which is not {name} or a name under it",
            report.station.as_str()
        ));
    }
    for finding in report.findings() {
        if !owns(&finding.station) {
            return Err(format!(
                "one of its findings is stamped {:?}, which is not {name} or a name under it",
                finding.station.as_str()
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use camino::Utf8Path;
    use relic_core::finding::{Finding, Grade, Severity};

    use super::*;

    /// A `PATH` with binaries a test wrote, and a registry naming them.
    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
        bin: Utf8PathBuf,
    }

    impl Machine {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let root =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            let home = root.join("home");
            let bin = root.join("bin");
            fs_err::create_dir_all(home.join(".local/bin")).expect("home");
            fs_err::create_dir_all(&bin).expect("bin");
            Self {
                _dir: dir,
                home,
                bin,
            }
        }

        /// A binary that answers `doctor --format json` with `stdout`, exits
        /// with `code`, and is otherwise inert.
        fn speaker(&self, name: &str, stdout: &str, code: i32) -> &Self {
            self.binary(
                name,
                &format!("#!/bin/sh\ncat <<'JSON'\n{stdout}\nJSON\nexit {code}\n"),
            )
        }

        fn binary(&self, name: &str, body: &str) -> &Self {
            let path = self.bin.join(name);
            fs_err::write(&path, body).expect("written");
            fs_err::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            self
        }

        fn registry(&self, body: &str) -> &Self {
            fs_err::write(self.home.join(REGISTRY), body).expect("written");
            self
        }

        fn context(&self) -> Context {
            Context::new(self.home.clone(), vec![self.bin.clone()])
        }

        fn outcome(&self) -> Outcome {
            station().check(&self.context()).expect("the station ran")
        }

        fn findings(&self) -> Vec<Finding> {
            match self.outcome() {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("unexpectedly skipped: {reason}"),
            }
        }
    }

    /// The station with a budget a test can afford to spend.
    fn station() -> RegistryAdapter {
        RegistryAdapter {
            budget: Duration::from_millis(600),
            ..RegistryAdapter::default()
        }
    }

    /// A report a speaker would print, as JSON.
    fn report_json(station: &str, findings: &[(&str, &str)]) -> String {
        let station: StationId = station.parse().expect("a valid id");
        let findings: Vec<Finding> = findings
            .iter()
            .map(|(id, summary)| {
                let id: StationId = id.parse().expect("a valid id");
                id.soft(Summary::lossy(summary))
            })
            .collect();
        serde_json::to_string(&Report::ran(station, findings)).expect("serialised")
    }

    // --- Reading the registry ----------------------------------------------

    #[test]
    fn the_registry_is_names_first_owners_second_and_comments_never() {
        let registry = Registry::parse(
            "# what this is\n\ndocket\tdocket\nbare\n  spaced  \towner  \ndocket\tdocket\n",
        );
        assert_eq!(
            registry.entries,
            vec![
                Entry {
                    name: "docket".to_owned(),
                    owner: Some("docket".to_owned()),
                },
                Entry {
                    name: "bare".to_owned(),
                    owner: None,
                },
                Entry {
                    name: "spaced".to_owned(),
                    owner: Some("owner".to_owned()),
                },
            ],
            "a name is asked once however often it is listed"
        );
    }

    #[test]
    fn no_registry_is_a_skip_and_never_a_finding() {
        let machine = Machine::new();
        let Outcome::Skipped(reason) = machine.outcome() else {
            panic!("a machine that has published nothing has nobody to ask");
        };
        assert!(reason.as_str().contains("nothing is registered"));
    }

    #[test]
    fn a_registry_nobody_answers_is_a_skip_that_says_so() {
        let machine = Machine::new();
        machine.registry("quiet\towner\n");
        machine.binary("quiet", "#!/bin/sh\necho 'usage: quiet ...' >&2\nexit 2\n");

        let Outcome::Skipped(reason) = machine.outcome() else {
            panic!("nothing answered, so there is nothing to report");
        };
        assert!(reason.as_str().contains("answers"), "{reason}");
    }

    // --- Collecting -------------------------------------------------------

    #[test]
    fn a_speakers_findings_arrive_under_its_own_name() {
        let machine = Machine::new();
        machine.registry("docket\tdocket\n");
        machine.speaker(
            "docket",
            &report_json("docket", &[("docket", "the depot has no remote")]),
            1,
        );

        let findings = machine.findings();
        assert_eq!(findings.len(), 1);
        let finding = findings.first().expect("a finding");
        assert_eq!(finding.station.as_str(), "docket");
        assert_eq!(finding.summary.as_str(), "the depot has no remote");
        assert_eq!(Grade::of(&findings), Grade::Soft);
    }

    #[test]
    fn a_speaker_that_is_healthy_contributes_nothing_and_still_counts_as_answering() {
        let machine = Machine::new();
        machine.registry("docket\tdocket\nmidden\tmidden\n");
        machine.speaker("docket", &report_json("docket", &[]), 0);
        machine.binary("midden", "#!/bin/sh\nexit 2\n");

        let findings = machine.findings();
        assert!(findings.is_empty(), "{findings:#?}");
    }

    #[test]
    fn a_station_under_the_binarys_name_is_its_own_to_use() {
        let machine = Machine::new();
        machine.registry("docket\n");
        machine.speaker(
            "docket",
            &report_json(
                "docket-git",
                &[("docket-git", "the depot is not a repository")],
            ),
            1,
        );

        assert_eq!(
            machine.findings().len(),
            1,
            "a sub-station is still its own"
        );
    }

    #[test]
    fn a_report_stamped_with_someone_elses_name_is_refused() {
        let machine = Machine::new();
        machine.registry("docket\n");
        machine.speaker("docket", &report_json("bedrock", &[]), 0);

        let findings = machine.findings();
        assert_eq!(findings.len(), 1);
        let finding = findings.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Note);
        assert!(
            finding
                .detail
                .as_ref()
                .is_some_and(|why| why.as_str().contains("station \"bedrock\"")),
            "{finding:#?}"
        );
    }

    #[test]
    fn a_finding_stamped_with_someone_elses_name_is_refused_too() {
        let machine = Machine::new();
        machine.registry("docket\n");
        machine.speaker(
            "docket",
            &report_json("docket", &[("bedrock", "not docket's to say")]),
            1,
        );

        let findings = machine.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(
            findings.first().map(|finding| finding.severity),
            Some(Severity::Note)
        );
        assert!(
            findings
                .first()
                .and_then(|finding| finding.detail.as_ref())
                .is_some_and(|why| why.as_str().contains("findings is stamped")),
            "{findings:#?}"
        );
    }

    // --- Not answering ------------------------------------------------------

    #[test]
    fn human_output_is_a_program_that_never_meant_to_answer() {
        let machine = Machine::new();
        machine.registry("relic\nspeaker\n");
        machine.binary(
            "relic",
            "#!/bin/sh\necho 'Orphan registry entries'\necho '  (none)'\n",
        );
        machine.speaker("speaker", &report_json("speaker", &[]), 0);

        assert!(
            machine.findings().is_empty(),
            "a table is not a malformed report, it is a program that ignores the flag"
        );
    }

    #[test]
    fn json_that_is_not_a_report_is_a_note_because_it_meant_to_answer() {
        let machine = Machine::new();
        machine.registry("dewey\thalo\n");
        // halo's own doctor shape, which is not this contract.
        machine.speaker("dewey", r#"{"checks":[],"problems":0}"#, 0);

        let findings = machine.findings();
        assert_eq!(findings.len(), 1);
        let finding = findings.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Note);
        assert_eq!(finding.station.as_str(), "registry");
        assert!(finding.summary.as_str().contains("dewey"));
    }

    #[test]
    fn a_registered_binary_that_is_not_on_the_path_is_left_to_relic_doctor() {
        let machine = Machine::new();
        machine.registry("absent\towner\n");

        let Outcome::Skipped(_) = machine.outcome() else {
            panic!("a name nobody can run is not this station's finding");
        };
    }

    #[test]
    fn a_binary_that_never_returns_is_killed_and_reported() {
        let machine = Machine::new();
        machine.registry("hangs\n");
        machine.binary("hangs", "#!/bin/sh\nsleep 30\n");

        // The shipped bound is proven in `relic_core::tool`; what this proves is
        // that the station applies one at all, which is the only thing standing
        // between a registered binary and a hung `yadm update`.
        let started = std::time::Instant::now();
        let findings = machine.findings();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "the bound was not honoured"
        );
        assert_eq!(findings.len(), 1);
        let finding = findings.first().expect("a finding");
        assert_eq!(
            finding.severity,
            Severity::Note,
            "from outside a program, a hang and slow work are the same fact: not judged"
        );
        assert!(finding.summary.as_str().contains("hangs"));
    }

    // --- Order --------------------------------------------------------------

    #[test]
    fn answers_come_back_in_registry_order_however_they_arrive() {
        let machine = Machine::new();
        machine.registry("first\nsecond\nthird\n");
        for (name, delay) in [("first", "0.15"), ("second", "0.05"), ("third", "0")] {
            let body = format!(
                "#!/bin/sh\nsleep {delay}\ncat <<'JSON'\n{}\nJSON\nexit 1\n",
                report_json(name, &[(name, name)])
            );
            machine.binary(name, &body);
        }

        let names: Vec<String> = machine
            .findings()
            .iter()
            .map(|finding| finding.station.to_string())
            .collect();
        assert_eq!(names, vec!["first", "second", "third"]);
    }

    #[test]
    fn the_probes_run_at_once_rather_than_one_after_another() {
        let machine = Machine::new();
        let names = ["a", "b", "c", "d", "e", "f"];
        machine.registry(&names.join("\n"));
        for name in names {
            machine.binary(name, "#!/bin/sh\nsleep 0.4\nexit 2\n");
        }

        let started = std::time::Instant::now();
        let _ = machine.outcome();
        assert!(
            started.elapsed() < Duration::from_millis(1600),
            "six 400ms probes took {:?}, which is serial",
            started.elapsed()
        );
    }

    #[test]
    fn resolution_reads_the_injected_path_and_never_the_process_one() {
        let machine = Machine::new();
        machine.registry("sh\n");
        let cx = Context::new(machine.home.clone(), Vec::new());
        let Outcome::Skipped(_) = station().check(&cx).expect("ran") else {
            panic!("an empty search path resolves nothing, whatever is on the real one");
        };
        assert!(Utf8Path::new("/bin/sh").exists(), "sh is on the real PATH");
    }
}
