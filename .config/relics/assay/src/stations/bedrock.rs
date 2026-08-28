//! The bedrock: the members guaranteed present, configured and PATH-accessible
//! on every machine, with their sub-APIs.
//!
//! Verification only — installing is the base Brewfile's job. Offline and
//! side-effect-free: the docker daemon is never woken, because that would launch
//! `OrbStack` from an unattended `yadm doctor`. `--deep` asks for it explicitly.
//!
//! See `~/.config/reliquary/BEDROCK.md` for the contract this checks.

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use relic_core::finding::{Detail, Finding, FixHint, Outcome, StationId, Summary};
use relic_core::tool::Tool;

use crate::probe;
use crate::station::{Context, Station};

/// One bedrock member and what proves it whole.
struct Member {
    /// The name that must resolve on PATH.
    name: &'static str,
    /// What installs it, for the fix hint.
    fix: &'static str,
    /// Copies of this name that are the OS baseline bedrock deliberately
    /// shadows by PATH order. Their presence behind the winner is not drift.
    expected_extra: &'static [&'static str],
    /// The sub-API probe, run against the winner.
    probe: fn(&Probe<'_>) -> Vec<Finding>,
}

/// Every member, in the order they are reported.
const MEMBERS: &[Member] = &[
    Member {
        name: "bash",
        fix: "brew install bash, then put its bin ahead of /bin on PATH",
        expected_extra: &["/bin/bash"],
        probe: bash,
    },
    Member {
        name: "python3",
        fix: "brew install python",
        expected_extra: &["/usr/bin/python3"],
        probe: python3,
    },
    Member {
        name: "uv",
        fix: "brew install uv",
        expected_extra: &[],
        probe: uv,
    },
    Member {
        name: "docker",
        fix: "install a docker implementation with compose and buildx",
        expected_extra: &[],
        probe: docker,
    },
    Member {
        name: "git",
        fix: "brew install git",
        expected_extra: &["/usr/bin/git"],
        probe: presence_only,
    },
    Member {
        name: "curl",
        fix: "brew install curl",
        expected_extra: &["/usr/bin/curl"],
        probe: presence_only,
    },
    Member {
        name: "just",
        fix: "brew install just",
        expected_extra: &[],
        probe: just,
    },
    Member {
        name: "cargo",
        fix: "install rustup, then rustup component add rustfmt clippy",
        expected_extra: &[],
        probe: cargo,
    },
];

/// The station.
pub struct Bedrock {
    id: StationId,
}

impl Default for Bedrock {
    fn default() -> Self {
        Self {
            id: StationId::from_static("bedrock"),
        }
    }
}

impl Station for Bedrock {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "bedrock members present, configured and PATH-accessible with their sub-APIs"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        Ok(Outcome::Ran(examine(&self.id, cx)))
    }
}

/// What a probe is given: the member, the winner, and the machine.
struct Probe<'a> {
    station: &'a StationId,
    member: &'a Member,
    winner: &'a Utf8Path,
    cx: &'a Context,
}

impl Probe<'_> {
    /// Runs the winner and takes its first line, or nothing when it refused.
    fn ask(&self, args: &[&str]) -> Option<String> {
        self.run(self.winner, args)
    }

    /// Whether the winner accepts a sub-command at all.
    fn accepts(&self, args: &[&str]) -> bool {
        self.ask(args).is_some()
    }

    fn run(&self, program: &Utf8Path, args: &[&str]) -> Option<String> {
        let tool = Tool::at_path(self.member.name, program.as_std_path().to_owned());
        let mut command = tool.command();
        command.args(args);
        tool.capture(&mut command)
            .ok()
            .map(|output| output.line().to_owned())
    }

    /// Whether a companion program is on PATH beside the member.
    fn beside(&self, name: &str) -> bool {
        probe::resolve(name, self.cx.path()).is_some()
    }

    fn broken(&self, text: &str) -> Finding {
        self.station.broken(Summary::lossy(text))
    }

    fn soft(&self, text: &str) -> Finding {
        self.station.soft(Summary::lossy(text))
    }
}

/// Every finding the bedrock has to offer.
fn examine(station: &StationId, cx: &Context) -> Vec<Finding> {
    let brew_prefix = brew_prefix(cx);
    let mut findings = Vec::new();

    for member in MEMBERS {
        let hits = probe::resolve_all(member.name, cx.path());
        let Some(winner) = hits.first() else {
            findings.push(
                station
                    .broken(Summary::lossy(&format!(
                        "{} is not on PATH, and bedrock requires it",
                        member.name
                    )))
                    .fixed_by(FixHint::lossy(member.fix)),
            );
            continue;
        };

        let probe = Probe {
            station,
            member,
            winner,
            cx,
        };
        findings.extend((member.probe)(&probe));
        findings.extend(shadows(
            station,
            member,
            winner,
            &hits,
            brew_prefix.as_deref(),
        ));
    }

    findings
}

/// Homebrew's prefix, when there is one. Absent is not a finding: bedrock is
/// cross-platform and Homebrew is one way to satisfy it.
fn brew_prefix(cx: &Context) -> Option<String> {
    let brew = probe::resolve("brew", cx.path())?;
    let tool = Tool::at_path("brew", brew.as_std_path().to_owned());
    let mut command = tool.command();
    command.arg("--prefix");
    let line = tool.capture(&mut command).ok()?.line().to_owned();
    (!line.is_empty()).then_some(line)
}

/// The one-install goal, as warnings. Never a failure: the winner's own probe
/// already catches a genuinely wrong winner, and a second copy behind it is a
/// PATH-order question rather than a broken machine.
fn shadows(
    station: &StationId,
    member: &Member,
    winner: &Utf8Path,
    hits: &[Utf8PathBuf],
    brew_prefix: Option<&str>,
) -> Vec<Finding> {
    let mut findings = Vec::new();

    if let Some(prefix) = brew_prefix {
        let brewed = Utf8Path::new(prefix).join("bin").join(member.name);
        if probe::is_executable(&brewed) && brewed != winner {
            findings.push(station.soft(Summary::lossy(&format!(
                "{}: Homebrew has {brewed}, and {winner} wins on PATH",
                member.name
            ))));
        }
    }

    let unexpected: Vec<&Utf8PathBuf> = hits
        .iter()
        .skip(1)
        .filter(|extra| !member.expected_extra.contains(&extra.as_str()))
        .filter(|extra| !probe::same_file(winner, extra))
        .collect();

    if !unexpected.is_empty() {
        let listed = unexpected
            .iter()
            .map(|path| path.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        let mut finding = station.soft(Summary::lossy(&format!(
            "{}: another install is on PATH behind {winner}",
            member.name
        )));
        if let Some(detail) = Detail::new(listed) {
            finding = finding.detailed(detail);
        }
        findings.push(finding);
    }

    findings
}

fn presence_only(_probe: &Probe<'_>) -> Vec<Finding> {
    Vec::new()
}

/// macOS ships bash 3.2 at `/bin/bash` and it is frozen there. Bedrock's whole
/// claim about bash is the major version of whatever wins.
fn bash(probe: &Probe<'_>) -> Vec<Finding> {
    let Some(version) = probe.ask(&["-c", "echo \"$BASH_VERSION\""]) else {
        return vec![probe.broken(&format!("bash: {} did not run as bash", probe.winner))];
    };
    let major = version
        .split('.')
        .next()
        .and_then(|head| head.parse::<u32>().ok());
    match major {
        Some(major) if major >= 5 => Vec::new(),
        _ => vec![
            probe
                .broken(&format!(
                    "bash on PATH is {version}, and bedrock needs 5 or later"
                ))
                .fixed_by(FixHint::lossy(probe.member.fix)),
        ],
    }
}

fn python3(probe: &Probe<'_>) -> Vec<Finding> {
    if probe
        .ask(&["-c", "import sys; print(sys.version_info[0])"])
        .is_none()
    {
        return vec![probe.broken(&format!("python3: {} failed to run", probe.winner))];
    }
    if probe.accepts(&["-m", "pip", "--version"]) {
        return Vec::new();
    }
    vec![probe.soft("python3: `python3 -m pip` is unavailable, so ensurepip is missing")]
}

fn uv(probe: &Probe<'_>) -> Vec<Finding> {
    if probe.beside("uvx") {
        return Vec::new();
    }
    vec![probe.soft("uv: `uvx` is not on PATH, so this uv predates it")]
}

/// The member is the full docker API, so a missing plugin is a failure rather
/// than a warning — the same treatment cargo's rustup components get.
fn docker(probe: &Probe<'_>) -> Vec<Finding> {
    let mut findings = Vec::new();
    if !probe.accepts(&["compose", "version"]) {
        findings.push(probe.broken("docker: the compose plugin is missing"));
    }
    if !probe.accepts(&["buildx", "version"]) {
        findings.push(probe.broken("docker: the buildx plugin is missing"));
    }
    // Liveness is runtime state, not configuration, and asking for it starts the
    // daemon. Off unless the run asked for the expensive checks.
    if probe.cx.deep() && !probe.accepts(&["info"]) {
        findings.push(probe.soft("docker: the daemon is not reachable"));
    }
    findings
}

fn just(probe: &Probe<'_>) -> Vec<Finding> {
    if probe.ask(&["--version"]).is_some() {
        return Vec::new();
    }
    vec![probe.broken(&format!("just: {} failed to run", probe.winner))]
}

/// The member is `cargo`, and the sub-APIs are what is actually used. `rustc`,
/// because cargo without a compiler builds nothing; `fmt` and `clippy`, because
/// `relic test` runs both and a toolchain without them makes the verification
/// gate unenforceable. `rustup` is only a warning: another toolchain still
/// builds, it just loses the self-healing `up` relies on.
fn cargo(probe: &Probe<'_>) -> Vec<Finding> {
    if probe.ask(&["--version"]).is_none() {
        return vec![probe.broken("cargo: it failed to run, so there is no default toolchain")];
    }
    let mut findings = Vec::new();
    if !probe.beside("rustc") {
        findings.push(probe.broken("cargo: `rustc` is not on PATH, so nothing can be built"));
    }
    if !probe.accepts(&["fmt", "--version"]) {
        findings.push(
            probe.broken("cargo: the rustfmt component is missing, and `relic test` runs it"),
        );
    }
    if !probe.accepts(&["clippy", "--version"]) {
        findings
            .push(probe.broken("cargo: the clippy component is missing, and `relic test` runs it"));
    }
    if !probe.beside("rustup") {
        findings.push(
            probe.soft(
                "cargo: `rustup` is not on PATH, so the toolchain will not self-heal on `up`",
            ),
        );
    }
    findings
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::*;

    /// A stand-in for a bedrock member: a shell script that answers the probes
    /// the way the real program would. Fixtures rather than the machine's own
    /// tools, so the station's *policy* is under test and the result does not
    /// depend on what happens to be installed.
    fn fake(dir: &Utf8Path, name: &str, body: &str) {
        use std::os::unix::fs::PermissionsExt as _;

        let path = dir.join(name);
        fs_err::write(&path, format!("#!/bin/sh\n{body}\n")).expect("written");
        fs_err::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
            .expect("made executable");
    }

    struct Machine {
        _keep: tempfile::TempDir,
        bin: Utf8PathBuf,
    }

    impl Machine {
        /// Every member present and whole.
        fn whole() -> Self {
            let keep = tempfile::tempdir().expect("a scratch dir");
            let root = Utf8PathBuf::from_path_buf(keep.path().to_path_buf()).expect("utf-8");
            let bin = root.join("bin");
            fs_err::create_dir_all(&bin).expect("created");

            fake(&bin, "bash", "echo 5.3.0");
            fake(&bin, "python3", "exit 0");
            fake(&bin, "uv", "exit 0");
            fake(&bin, "uvx", "exit 0");
            fake(&bin, "docker", "exit 0");
            fake(&bin, "git", "exit 0");
            fake(&bin, "curl", "exit 0");
            fake(&bin, "just", "exit 0");
            fake(&bin, "cargo", "exit 0");
            fake(&bin, "rustc", "exit 0");
            fake(&bin, "rustup", "exit 0");

            Self { _keep: keep, bin }
        }

        fn cx(&self) -> Context {
            Context::new("/nowhere", vec![self.bin.clone()])
        }

        fn findings(&self) -> Vec<Finding> {
            examine(&StationId::from_static("bedrock"), &self.cx())
        }

        fn summaries(&self) -> Vec<String> {
            self.findings()
                .iter()
                .map(|finding| finding.summary.to_string())
                .collect()
        }
    }

    fn broken(findings: &[Finding]) -> usize {
        findings
            .iter()
            .filter(|f| f.severity == relic_core::finding::Severity::Broken)
            .count()
    }

    fn soft(findings: &[Finding]) -> usize {
        findings
            .iter()
            .filter(|f| f.severity == relic_core::finding::Severity::Soft)
            .count()
    }

    #[test]
    fn a_whole_bedrock_has_nothing_to_say() {
        assert_eq!(Machine::whole().summaries(), Vec::<String>::new());
    }

    #[test]
    fn a_missing_member_is_broken_and_says_how_to_get_it() {
        let machine = Machine::whole();
        fs_err::remove_file(machine.bin.join("just")).expect("removed");
        let findings = machine.findings();
        assert_eq!(broken(&findings), 1);
        let finding = findings.first().expect("a finding");
        assert!(finding.summary.as_str().contains("just is not on PATH"));
        assert!(
            finding
                .fix
                .as_ref()
                .is_some_and(|fix| fix.as_str().contains("just"))
        );
    }

    #[test]
    fn stock_bash_is_broken_rather_than_merely_old() {
        let machine = Machine::whole();
        fake(&machine.bin, "bash", "echo '3.2.57(1)-release'");
        let findings = machine.findings();
        assert_eq!(broken(&findings), 1);
        assert!(
            machine
                .summaries()
                .iter()
                .any(|s| s.contains("needs 5 or later")),
            "{:?}",
            machine.summaries()
        );
    }

    #[test]
    fn something_that_is_not_bash_at_all_is_broken() {
        let machine = Machine::whole();
        fake(&machine.bin, "bash", "exit 1");
        assert_eq!(broken(&machine.findings()), 1);
    }

    #[test]
    fn a_missing_sub_api_grades_by_whether_the_member_needs_it() {
        // uvx and rustup are conveniences: the member still works without them.
        let machine = Machine::whole();
        fs_err::remove_file(machine.bin.join("uvx")).expect("removed");
        fs_err::remove_file(machine.bin.join("rustup")).expect("removed");
        let findings = machine.findings();
        assert_eq!((broken(&findings), soft(&findings)), (0, 2));

        // compose, buildx, rustfmt and clippy are the member itself.
        let machine = Machine::whole();
        fake(&machine.bin, "docker", "exit 1");
        fake(
            &machine.bin,
            "cargo",
            "[ \"$1\" = \"--version\" ] && exit 0; exit 1",
        );
        let findings = machine.findings();
        assert_eq!((broken(&findings), soft(&findings)), (4, 0));
    }

    #[test]
    fn python_without_ensurepip_is_soft() {
        let machine = Machine::whole();
        fake(
            &machine.bin,
            "python3",
            "[ \"$1\" = \"-m\" ] && exit 1; exit 0",
        );
        let findings = machine.findings();
        assert_eq!((broken(&findings), soft(&findings)), (0, 1));
    }

    #[test]
    fn the_docker_daemon_is_left_asleep_unless_the_run_asks() {
        let machine = Machine::whole();
        // Answers the plugin probes, refuses `info` — a docker with no daemon.
        fake(
            &machine.bin,
            "docker",
            "[ \"$1\" = \"info\" ] && exit 1; exit 0",
        );
        let station = StationId::from_static("bedrock");
        assert!(examine(&station, &machine.cx()).is_empty());

        let woken = examine(&station, &machine.cx().deeply());
        assert_eq!((broken(&woken), soft(&woken)), (0, 1));
        assert!(
            woken
                .first()
                .is_some_and(|f| f.summary.as_str().contains("daemon")),
            "{woken:?}"
        );
    }

    #[test]
    fn the_os_baseline_behind_the_winner_is_not_drift() {
        let machine = Machine::whole();
        let cx = Context::new(
            "/nowhere",
            vec![machine.bin.clone(), Utf8PathBuf::from("/bin")],
        );
        // /bin/bash is on the real machine and is the one expected extra.
        let findings = examine(&StationId::from_static("bedrock"), &cx);
        assert!(
            !findings
                .iter()
                .any(|f| f.summary.as_str().starts_with("bash:")),
            "{findings:?}"
        );
    }

    #[test]
    fn a_second_install_behind_the_winner_is_soft() {
        let machine = Machine::whole();
        let other = machine.bin.parent().expect("a parent").join("other");
        fs_err::create_dir_all(&other).expect("created");
        fake(&other, "just", "exit 0");

        let cx = Context::new("/nowhere", vec![machine.bin.clone(), other]);
        let findings = examine(&StationId::from_static("bedrock"), &cx);
        assert_eq!((broken(&findings), soft(&findings)), (0, 1));
        assert!(
            findings
                .first()
                .is_some_and(|f| f.summary.as_str().contains("another install")),
            "{findings:?}"
        );
    }

    #[test]
    fn one_install_reached_twice_is_not_two_installs() {
        let machine = Machine::whole();
        let linked = machine.bin.parent().expect("a parent").join("linked");
        fs_err::create_dir_all(&linked).expect("created");
        std::os::unix::fs::symlink(machine.bin.join("just"), linked.join("just")).expect("linked");

        let cx = Context::new("/nowhere", vec![machine.bin.clone(), linked]);
        assert!(examine(&StationId::from_static("bedrock"), &cx).is_empty());
    }
}
