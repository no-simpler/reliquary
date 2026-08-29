//! Whether the paths a person waits on still cost what the repo says they cost.
//!
//! Every headline latency win the Rustification programme claims is a win that
//! nothing otherwise prevents from eroding. `reliquary/ratchets/perf-budgets.toml`
//! is the committed record of what each hot path costs today; this times them
//! against it.
//!
//! **A smoke alarm, not a benchmark suite.** One run per path, no warm-up, no
//! statistics. Timings are machine-dependent, a laptop under thermal load must
//! not trip a gate people then switch off, and the file's own tolerance is ×3 —
//! so what is being detected is a path that has changed kind, not one that has
//! drifted by a fraction.
//!
//! **Never worse than [`Severity::Soft`].** A slow machine is degraded and
//! entirely reproducible from the repo, which is the definition.
//!
//! **`--deep` only.** Every other station is free; this one spends the time it
//! measures. Running it inside an ordinary `yadm doctor` would put a minute onto
//! the dream pre-pass, and the first thing anyone would do about that is stop
//! running the pre-pass.
//!
//! **The recursion is real and it terminates.** `yadm doctor --quiet` is one of
//! the budgeted paths, and `yadm doctor` runs `assay` — but without `--deep`, so
//! the inner run skips this station. Depth two, and the number is honest: it is
//! what `yadm doctor --quiet` actually costs today, `assay` included, which is
//! the whole of what it now is.

use std::process::Stdio;
use std::time::{Duration, Instant};

use anyhow::{Context as _, Result};
use serde::Deserialize;

use relic_core::finding::{Detail, Finding, FixHint, Outcome, StationId, Summary};
use relic_core::tool::Tool;

use crate::station::{Context, Station};

/// The committed budget file, `$HOME`-relative.
const BUDGETS: &str = ".config/reliquary/ratchets/perf-budgets.toml";

/// How far past `budget × tolerance` a path is allowed to run before it is
/// stopped rather than measured.
///
/// A bound is needed at all because a path that has become pathological would
/// otherwise hold the run open for as long as it likes. Twice the reporting
/// threshold, so a finding still carries a real number in every case a number
/// would tell anyone anything — past that, "over N" is the whole of what a smoke
/// alarm has to say.
const BOUND_FACTOR: u32 = 2;

/// The shortest bound worth applying, whatever the budget says.
///
/// `ske-prompt`'s budget is 10 ms and process startup alone is a few. Without a
/// floor the bound would be noise, and every run of a healthy machine would stop
/// a path that was about to answer.
const BOUND_FLOOR: Duration = Duration::from_secs(1);

/// The file, as committed.
///
/// A schema this repo owns, so an unknown key is a typo worth failing on rather
/// than a field a third party added.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Budgets {
    /// How many times its budget a path may cost before it is worth reporting.
    tolerance: u32,
    /// The paths, in the order the file lists them.
    #[serde(default, rename = "path")]
    paths: Vec<Budgeted>,
}

/// One budgeted path.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Budgeted {
    /// What the path is called in a report.
    name: String,
    /// Argv, not a command line. There is no shell here, so nothing splits,
    /// quotes or globs — what the file says is what runs.
    command: Vec<String>,
    /// What it costs today, in milliseconds.
    budget_ms: u64,
    /// When the budget was measured. Never graded on — a stale date is not a
    /// finding — but carried into one, because a number recorded long ago is
    /// itself a candidate explanation for a path that now exceeds it.
    measured: String,
    /// Fed to the command on stdin. Absent means `/dev/null`.
    #[serde(default)]
    stdin: Option<String>,
    /// Whether this path can be timed at all. A `false` here is a recorded
    /// decision in the same sense as a line in `yadm/unmanaged`: the reason is
    /// beside it in `note`, so the station says nothing about it.
    #[serde(default = "yes")]
    timed: bool,
    /// Why the number is what it is. Never parsed, and carried into a finding
    /// so a reader gets the file's own explanation without opening the file.
    #[serde(default)]
    note: Option<String>,
}

/// `timed`'s default.
const fn yes() -> bool {
    true
}

/// The station.
pub struct PerfBudgets {
    id: StationId,
}

impl Default for PerfBudgets {
    fn default() -> Self {
        Self {
            id: StationId::from_static("perf-budgets"),
        }
    }
}

impl Station for PerfBudgets {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "the hot paths still cost what the repo says they cost"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        if !cx.deep() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "timing the hot paths costs the time it measures; run with --deep",
            )));
        }

        let file = cx.at(BUDGETS);
        let text = fs_err::read_to_string(&file)
            .with_context(|| format!("the budget file at {file} could not be read"))?;
        let budgets: Budgets = toml::from_str(&text)
            .with_context(|| format!("{file} is not a budget file this version understands"))?;

        Ok(Outcome::Ran(self.time_all(cx, &budgets)))
    }
}

impl PerfBudgets {
    /// Time every path that can be timed, and say what could not be.
    fn time_all(&self, cx: &Context, budgets: &Budgets) -> Vec<Finding> {
        let mut findings = Vec::new();
        for path in budgets.paths.iter().filter(|path| path.timed) {
            match self.time_one(cx, path, budgets.tolerance) {
                Ok(Some(finding)) => findings.push(finding),
                Ok(None) => {}
                Err(reason) => findings.push(self.id.note(Summary::lossy(&format!(
                    "{} could not be timed: {reason}",
                    path.name
                )))),
            }
        }
        findings
    }

    /// Run one path once and grade the clock.
    ///
    /// `Err` here is not a broken station — it is one path that could not be
    /// judged, which the caller turns into a note. A budget file naming a
    /// program this machine does not have is a fact about the machine, and the
    /// other eleven paths still have answers.
    fn time_one(
        &self,
        cx: &Context,
        path: &Budgeted,
        tolerance: u32,
    ) -> Result<Option<Finding>, String> {
        let threshold = Duration::from_millis(path.budget_ms.saturating_mul(u64::from(tolerance)));
        let bound = (threshold * BOUND_FACTOR).max(BOUND_FLOOR);

        let (program, rest) = path
            .command
            .split_first()
            .ok_or_else(|| "its command is empty".to_owned())?;
        let tool =
            resolve(cx, program).ok_or_else(|| format!("{program} is not on this machine"))?;

        let mut command = tool.command();
        command.args(rest);
        let feed = stdin_for(path)?;
        command.stdin(feed);

        let started = Instant::now();
        let outcome = tool.run_within(&mut command, bound);
        let elapsed = started.elapsed();

        let summary = match outcome {
            Ok(_) if elapsed <= threshold => return Ok(None),
            Ok(_) => format!(
                "{} took {} ms against a budget of {} ms (×{tolerance} tolerance)",
                path.name,
                elapsed.as_millis(),
                path.budget_ms,
            ),
            Err(relic_core::tool::Error::TimedOut { .. }) => format!(
                "{} was still running after {} ms and was stopped; its budget is {} ms",
                path.name,
                bound.as_millis(),
                path.budget_ms,
            ),
            Err(error) => return Err(error.to_string()),
        };

        let mut evidence = format!("budget recorded {}", path.measured);
        if let Some(note) = path.note.as_deref() {
            evidence.push('\n');
            evidence.push_str(note.trim());
        }

        Ok(Some(
            self.id
                .soft(Summary::lossy(&summary))
                .detailed_with(Detail::new(evidence))
                .fixed_by(FixHint::lossy(&format!(
                    "find what slowed it, or record the new cost in {BUDGETS}"
                ))),
        ))
    }
}

/// What the path reads on stdin.
///
/// A hook reads a payload, and one given nothing does whatever it does when
/// asked nothing — which is not the work the budget is about. A timing taken
/// from a program that bailed at its first read is a timing of the bail. The
/// literal is written to a temporary file rather than a pipe because nothing
/// would be left to write into the pipe once the child is running.
fn stdin_for(path: &Budgeted) -> Result<Stdio, String> {
    let Some(text) = path.stdin.as_deref() else {
        return Ok(Stdio::null());
    };
    let mut file = tempfile::tempfile().map_err(|error| error.to_string())?;
    std::io::Write::write_all(&mut file, text.as_bytes()).map_err(|error| error.to_string())?;
    std::io::Seek::rewind(&mut file).map_err(|error| error.to_string())?;
    Ok(Stdio::from(file))
}

/// Find the program a budgeted path names.
///
/// A name carrying a separator is a path under the home directory being checked,
/// so a budget can name a hook that is not and should not be on `$PATH`. A bare
/// name is resolved against the injected search path, never the process's own.
fn resolve(cx: &Context, program: &str) -> Option<Tool> {
    if program.contains('/') {
        let at = cx.at(program);
        return at
            .is_file()
            .then(|| Tool::at_path(program, at.clone().into_std_path_buf()));
    }
    cx.path()
        .iter()
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
        .map(|candidate| Tool::at_path(program, candidate.into_std_path_buf()))
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use relic_core::finding::Severity;

    use super::*;

    /// A machine with a budget file and a bin directory the tests can fill.
    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
        bin: Utf8PathBuf,
    }

    impl Machine {
        fn new(budgets: &str) -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let home =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            let file = home.join(BUDGETS);
            fs_err::create_dir_all(file.parent().expect("a parent")).expect("the ratchet dir");
            fs_err::write(&file, budgets).expect("the budget file");
            let bin = home.join("bin");
            fs_err::create_dir_all(&bin).expect("a bin dir");
            Self {
                _dir: dir,
                home,
                bin,
            }
        }

        /// Put a script on the injected search path.
        fn program(&self, name: &str, body: &str) -> &Self {
            let at = self.bin.join(name);
            fs_err::write(&at, format!("#!/bin/sh\n{body}\n")).expect("a program");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs_err::set_permissions(&at, std::fs::Permissions::from_mode(0o755))
                    .expect("executable");
            }
            self
        }

        fn context(&self) -> Context {
            Context::new(self.home.clone(), vec![self.bin.clone()]).deeply()
        }

        fn outcome(&self) -> Outcome {
            PerfBudgets::default()
                .check(&self.context())
                .expect("the station ran")
        }

        fn findings(&self) -> Vec<Finding> {
            match self.outcome() {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("unexpectedly skipped: {reason}"),
            }
        }
    }

    /// One budgeted path, spelled the way the committed file spells one.
    fn one(name: &str, command: &str, budget_ms: u64, extra: &str) -> String {
        format!(
            "tolerance = 3\n\n[[path]]\nname = \"{name}\"\ncommand = [\"{command}\"]\n\
             budget_ms = {budget_ms}\nmeasured = \"2026-08-29\"\n{extra}"
        )
    }

    #[test]
    fn timing_is_not_free_so_it_does_not_run_without_deep() {
        let machine = Machine::new(&one("quick", "quick", 5_000, ""));
        machine.program("quick", "exit 0");
        let outcome = PerfBudgets::default()
            .check(&Context::new(
                machine.home.clone(),
                vec![machine.bin.clone()],
            ))
            .expect("the station ran");
        let Outcome::Skipped(reason) = outcome else {
            panic!("a station that spends real time must be opt-in");
        };
        assert!(reason.as_str().contains("--deep"), "{reason}");
    }

    #[test]
    fn a_path_inside_its_budget_has_nothing_to_say() {
        let machine = Machine::new(&one("quick", "quick", 5_000, ""));
        machine.program("quick", "exit 0");
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn a_path_that_exits_non_zero_is_still_a_timing_and_not_a_failure() {
        let machine = Machine::new(&one("cross", "cross", 5_000, ""));
        machine.program("cross", "exit 7");
        assert!(
            machine.findings().is_empty(),
            "the clock is what is being read, not the status"
        );
    }

    #[test]
    fn a_path_past_its_tolerance_is_soft_and_names_both_numbers() {
        // 1 ms budget, ×3 tolerance: any real process start is over it, and the
        // 1 s bound floor is long enough that it still finishes and is measured.
        let machine = Machine::new(&one("slow", "slow", 1, ""));
        machine.program("slow", "exit 0");
        let findings = machine.findings();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].severity, Severity::Soft);
        let said = findings[0].summary.as_str();
        assert!(said.contains("slow"), "{said}");
        assert!(said.contains("budget of 1 ms"), "{said}");
        assert!(said.contains("×3 tolerance"), "{said}");
    }

    #[test]
    fn a_path_that_will_not_finish_is_stopped_rather_than_left_to_run() {
        let machine = Machine::new(&one("hung", "hung", 1, ""));
        machine.program("hung", "sleep 30");
        let started = Instant::now();
        let findings = machine.findings();
        assert!(
            started.elapsed() < Duration::from_secs(10),
            "the bound must hold the run open for its own length, not the child's"
        );
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].severity, Severity::Soft);
        assert!(
            findings[0].summary.as_str().contains("was stopped"),
            "{:?}",
            findings[0].summary
        );
    }

    #[test]
    fn a_budget_that_cannot_be_timed_is_passed_over_without_a_word() {
        let machine = Machine::new(&one("manual", "nowhere", 1, "timed = false\n"));
        assert!(
            machine.findings().is_empty(),
            "a recorded decision is not a finding, the way yadm/unmanaged is not"
        );
    }

    #[test]
    fn a_program_this_machine_does_not_have_is_a_note_and_never_a_verdict() {
        let machine = Machine::new(&one("absent", "not-installed", 10, ""));
        let findings = machine.findings();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].severity, Severity::Note);
        assert!(
            findings[0].summary.as_str().contains("not on this machine"),
            "{:?}",
            findings[0].summary
        );
    }

    #[test]
    fn one_path_that_cannot_be_judged_does_not_cost_the_others_their_answer() {
        let mut budgets = one("absent", "not-installed", 10, "");
        budgets.push_str(
            "\n[[path]]\nname = \"slow\"\ncommand = [\"slow\"]\n\
             budget_ms = 1\nmeasured = \"2026-08-29\"\n",
        );
        let machine = Machine::new(&budgets);
        machine.program("slow", "exit 0");
        let findings = machine.findings();
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert_eq!(findings[0].severity, Severity::Note);
        assert_eq!(findings[1].severity, Severity::Soft);
    }

    #[test]
    fn a_hook_is_named_by_path_and_read_from_the_home_being_checked() {
        let machine = Machine::new(&one("hook", "hooks/probe", 1, ""));
        fs_err::create_dir_all(machine.home.join("hooks")).expect("a hooks dir");
        let at = machine.home.join("hooks/probe");
        fs_err::write(&at, "#!/bin/sh\nexit 0\n").expect("a hook");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs_err::set_permissions(&at, std::fs::Permissions::from_mode(0o755))
                .expect("executable");
        }
        let findings = machine.findings();
        assert_eq!(
            findings.len(),
            1,
            "a path under $HOME resolves: {findings:?}"
        );
        assert_eq!(findings[0].severity, Severity::Soft);
    }

    #[test]
    fn a_path_that_needs_its_stdin_is_given_it() {
        // Refuses an empty stdin. Without the literal it would exit at once and
        // be timed as a refusal rather than as work.
        let machine = Machine::new(&one("fed", "fed", 5_000, "stdin = \"{}\"\n"));
        machine.program("fed", "read line || exit 3\ntest -n \"$line\" || exit 3");
        assert!(
            machine.findings().is_empty(),
            "the literal must reach the child"
        );
    }

    #[test]
    fn an_unusable_budget_file_stops_the_station_rather_than_grading_a_clean_machine() {
        let machine = Machine::new("tolerance = 3\n[[path]]\nname = \"x\"\n");
        let Err(error) = PerfBudgets::default().check(&machine.context()) else {
            panic!("an unreadable ratchet must never look like an empty one");
        };
        assert!(error.to_string().contains("budget file"), "{error}");
    }

    #[test]
    fn an_unknown_key_in_a_schema_we_own_is_a_typo_and_is_refused() {
        let machine = Machine::new(&one("quick", "quick", 5_000, "budget_sec = 5\n"));
        assert!(
            PerfBudgets::default().check(&machine.context()).is_err(),
            "deny_unknown_fields is what makes a misspelled budget visible"
        );
    }

    #[test]
    fn the_committed_budget_file_is_one_this_version_understands() {
        let text = fs_err::read_to_string(
            Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../reliquary/ratchets/perf-budgets.toml"),
        )
        .expect("the committed ratchet");
        let budgets: Budgets = toml::from_str(&text).expect("it parses");
        assert!(budgets.tolerance >= 1);
        assert!(!budgets.paths.is_empty());
        for path in &budgets.paths {
            assert!(!path.command.is_empty(), "{} has no command", path.name);
        }
    }
}
