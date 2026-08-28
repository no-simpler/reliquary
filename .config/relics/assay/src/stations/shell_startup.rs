//! What an interactive shell actually does when it starts.
//!
//! Everything else about the shell layer is read from files. This is the one
//! check that can only be answered by starting a shell and looking, because the
//! question is not what the configuration says — it is what survives sourcing
//! all of it in order, in the dialect that shell speaks.
//!
//! Three facts come out of one probe per shell, which is why they are one
//! station rather than three: starting a shell is the expensive part, and doing
//! it once per question would triple the cost of the whole run.
//!
//! - **`yadm` resolves to the wrapper.** `~/.config/bin/yadm` is a symlink to
//!   `yadm-wrapper` and must win over Homebrew's, or the wrapper-only
//!   subcommands are not there and `yadm encrypt` stops recording the archive's
//!   hash. This has been broken before, silently, by an inherited `ZDOTDIR`.
//! - **Startup completes.** A shell that errors part-way through still gives you
//!   a prompt, so the only way to know is to ask it to say something afterwards.
//! - **`$PATH` holds no duplicates.** Harmless until one copy is stale, and then
//!   it decides which binary runs.
//!
//! The probe is **tagged**, because a machine's own interactive configuration
//! prints to stdout — an update notice, a greeting, a prompt fragment — and
//! untagged output would be read as an answer. It is also **bounded**: a shell
//! that hangs during startup would hang the standing audit, and from outside a
//! process a hang and a very slow start are the same fact. That fact is exactly
//! what "startup did not complete" means, so a timeout is not a separate case.

use std::time::Duration;

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use relic_core::finding::{Detail, Finding, FixHint, Outcome, StationId, Summary};
use relic_core::tool::{self, Tool};

use crate::station::{Context, Station};

/// The shells this machine is configured for, in the order they are reported.
const SHELLS: &[Shell] = &[
    Shell {
        name: "bash",
        dialect: Dialect::Posix,
    },
    Shell {
        name: "zsh",
        dialect: Dialect::Posix,
    },
    Shell {
        name: "fish",
        dialect: Dialect::Fish,
    },
];

/// Where the wrapper must be found, `$HOME`-relative.
const WRAPPER: &str = ".config/bin/yadm";

/// How long a shell has to start and answer.
///
/// Measured 2026-08-28 on this machine: bash 0.41 s, zsh 0.59 s, fish 0.33 s.
/// The margin is wide because a cold plugin manager, a first `compinit`, or a
/// machine under load are all legitimately slower — and a check that reddens on
/// a slow morning is a check that gets ignored on a broken one.
const BUDGET: Duration = Duration::from_secs(20);

/// What the probe stamps on each datum, so a shell's own chatter cannot be read
/// as an answer.
const YADM_TAG: &str = "__D_YADM__";
/// The marker that only prints if everything before it ran.
const DONE_TAG: &str = "__D_OK__";
/// The tag on the reported search path.
const PATH_TAG: &str = "__D_PATH__";

/// Which syntax a shell speaks. The two dialects differ in exactly one place
/// here — how a list variable is joined — and nowhere else.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Dialect {
    /// bash and zsh: `$PATH` is already a colon-joined string.
    Posix,
    /// fish: `$PATH` is a list, and joining it is explicit.
    Fish,
}

impl Dialect {
    /// The one-liner the shell is asked to run.
    ///
    /// `command -v` rather than `which`: it is a builtin in all three, so it
    /// answers about the shell's own resolution rather than about whichever
    /// `which` is on the path.
    fn probe(self) -> String {
        let path = match self {
            Self::Posix => format!("printf '{PATH_TAG}%s\\n' \"$PATH\""),
            Self::Fish => format!("printf '{PATH_TAG}%s\\n' (string join : -- $PATH)"),
        };
        format!("command -v yadm | sed 's|^|{YADM_TAG}|'; echo {DONE_TAG}; {path}")
    }
}

/// One shell this station knows how to ask.
#[derive(Clone, Copy, Debug)]
struct Shell {
    name: &'static str,
    dialect: Dialect,
}

/// The station.
pub struct ShellStartup {
    id: StationId,
    /// How long a shell has to answer. A field rather than the constant, so a
    /// test can prove the bound without spending it.
    budget: Duration,
}

impl Default for ShellStartup {
    fn default() -> Self {
        Self {
            id: StationId::from_static("shell-startup"),
            budget: BUDGET,
        }
    }
}

impl Station for ShellStartup {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "every interactive shell starts clean, finds the yadm wrapper, and has no duplicate PATH entries"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let present: Vec<Shell> = SHELLS
            .iter()
            .copied()
            .filter(|shell| crate::probe::resolve(shell.name, cx.path()).is_some())
            .collect();
        if present.is_empty() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "none of bash, zsh or fish is on the search path",
            )));
        }

        // Serial, unlike the registry adapter's probes. Three interactive
        // startups at once contend for the caches they build on a cold machine —
        // a `compinit` dump, a plugin manager's clone — and a health check must
        // not be the thing that creates a race nothing else would hit. Three
        // shells at well under a second each is a cost worth paying for that.
        let mut findings = Vec::new();
        for shell in present {
            findings.extend(self.examine(cx, shell));
        }
        Ok(Outcome::Ran(findings))
    }
}

impl ShellStartup {
    /// Everything one shell's probe says.
    fn examine(&self, cx: &Context, shell: Shell) -> Vec<Finding> {
        let answer = match probe(cx, shell, self.budget) {
            Ok(answer) => answer,
            Err(why) => {
                return vec![
                    self.id
                        .broken(Summary::lossy(&format!(
                            "{} did not complete an interactive startup",
                            shell.name
                        )))
                        .detailed_with(Detail::new(why))
                        .fixed_by(FixHint::lossy(&format!(
                            "run `{} -ic true` and read what it says",
                            shell.name
                        ))),
                ];
            }
        };

        let mut findings = Vec::new();
        let wrapper = cx.at(WRAPPER);
        match answer.yadm.as_deref() {
            Some(resolved) if resolved == wrapper.as_str() => {}
            Some(resolved) => findings.push(
                self.id
                    .broken(Summary::lossy(&format!(
                        "{} resolves yadm to {resolved}, not the wrapper at {wrapper}",
                        shell.name
                    )))
                    .fixed_by(FixHint::lossy(
                        "~/.config/bin must come before Homebrew on PATH — see env.d/999-path",
                    )),
            ),
            None => findings.push(
                self.id
                    .broken(Summary::lossy(&format!(
                        "{} resolves yadm to nothing at all",
                        shell.name
                    )))
                    .fixed_by(FixHint::lossy(
                        "~/.config/bin must be on PATH — see env.d/999-path",
                    )),
            ),
        }

        let duplicates = duplicates(&answer.path);
        if !duplicates.is_empty() {
            findings.push(
                self.id
                    .soft(Summary::lossy(&format!(
                        "{} has {} PATH {} listed more than once",
                        shell.name,
                        duplicates.len(),
                        if duplicates.len() == 1 {
                            "entry"
                        } else {
                            "entries"
                        }
                    )))
                    .detailed_with(Detail::new(duplicates.join("\n")))
                    .fixed_by(FixHint::lossy(
                        "harmless until one copy is stale, and then it decides which binary runs",
                    )),
            );
        }
        findings
    }
}

/// What one shell said.
#[derive(Debug, Default, PartialEq, Eq)]
struct Answer {
    /// What `command -v yadm` resolved to, when it resolved to anything.
    yadm: Option<String>,
    /// The search path the shell reported, in order.
    path: Vec<Utf8PathBuf>,
}

/// Start one shell interactively and read what it stamps.
///
/// `Err` carries what to say about a shell that never got to the end, which is
/// the same finding whether it errored, was killed, or simply never returned.
fn probe(cx: &Context, shell: Shell, budget: Duration) -> Result<Answer, String> {
    let Some(program) = crate::probe::resolve(shell.name, cx.path()) else {
        return Err(format!("{} is not on the search path", shell.name));
    };
    let tool = Tool::at_path(shell.name, Utf8PathBuf::into_std_path_buf(program));
    let mut command = tool.command();
    command.arg("-ic").arg(shell.dialect.probe());

    // `run_within`, not `capture_within`: an interactive shell's exit status is
    // whatever its last command left behind, and a rc file ending in a failed
    // `[ -f … ]` is not a broken shell. What is asked is whether the probe got
    // to the end, and the marker answers that.
    let exit = match tool.run_within(&mut command, budget) {
        Ok(exit) => exit,
        Err(tool::Error::TimedOut { .. }) => {
            return Err(format!(
                "it was still starting after {}ms and was stopped",
                budget.as_millis()
            ));
        }
        Err(error) => return Err(error.to_string()),
    };

    let mut answer = Answer::default();
    let mut done = false;
    for line in exit.stdout.lines() {
        if line == DONE_TAG {
            done = true;
        } else if let Some(rest) = line.strip_prefix(YADM_TAG) {
            // The first hit, which is what the shell would actually run.
            if answer.yadm.is_none() {
                answer.yadm = Some(rest.trim().to_owned());
            }
        } else if let Some(rest) = line.strip_prefix(PATH_TAG) {
            answer.path = rest
                .split(':')
                .filter(|entry| !entry.is_empty())
                .map(Utf8PathBuf::from)
                .collect();
        }
    }
    if !done {
        let said = exit.stderr.trim();
        return Err(if said.is_empty() {
            "it printed no completion marker and said nothing".to_owned()
        } else {
            said.to_owned()
        });
    }
    Ok(answer)
}

/// The entries listed more than once, each named once, in the order they first
/// appear — so two runs of the same machine produce the same evidence.
fn duplicates(path: &[Utf8PathBuf]) -> Vec<String> {
    let mut seen: Vec<&Utf8Path> = Vec::new();
    let mut repeated: Vec<String> = Vec::new();
    for entry in path {
        if seen.contains(&entry.as_path()) {
            if !repeated.contains(&entry.to_string()) {
                repeated.push(entry.to_string());
            }
        } else {
            seen.push(entry);
        }
    }
    repeated
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use relic_core::finding::Severity;

    use super::*;

    /// A machine whose shells are scripts a test wrote.
    ///
    /// The real thing runs `bash -ic`, which answers for the machine the test
    /// happens to run on. These answer for the test.
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
            fs_err::create_dir_all(home.join(".config/bin")).expect("home");
            fs_err::create_dir_all(&bin).expect("bin");
            Self {
                _dir: dir,
                home,
                bin,
            }
        }

        /// A shell that answers the probe with `yadm` at `resolves` and `path`.
        fn shell(&self, name: &str, resolves: Option<&str>, path: &str) -> &Self {
            let yadm = resolves.map_or_else(String::new, |at| {
                format!("printf '{YADM_TAG}%s\\n' '{at}'\n")
            });
            self.script(
                name,
                &format!("#!/bin/sh\n{yadm}echo {DONE_TAG}\nprintf '{PATH_TAG}%s\\n' '{path}'\n"),
            )
        }

        fn script(&self, name: &str, body: &str) -> &Self {
            let path = self.bin.join(name);
            fs_err::write(&path, body).expect("written");
            fs_err::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).expect("chmod");
            self
        }

        fn wrapper(&self) -> String {
            self.home.join(WRAPPER).to_string()
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

    /// The station with a bound a test can afford to spend.
    fn station() -> ShellStartup {
        ShellStartup {
            budget: Duration::from_millis(600),
            ..ShellStartup::default()
        }
    }

    #[test]
    fn a_machine_with_none_of_the_three_shells_is_skipped_not_passed() {
        let machine = Machine::new();
        let Outcome::Skipped(reason) = machine.outcome() else {
            panic!("there is nothing to answer for");
        };
        assert!(reason.as_str().contains("bash, zsh or fish"));
    }

    #[test]
    fn a_shell_that_is_not_installed_is_simply_not_asked() {
        let machine = Machine::new();
        machine.shell("bash", Some(&machine.wrapper()), "/usr/bin");
        assert!(
            machine.findings().is_empty(),
            "zsh and fish are absent, which is a fact about the machine and not a defect"
        );
    }

    #[test]
    fn a_shell_that_finds_the_wrapper_has_nothing_to_report() {
        let machine = Machine::new();
        for name in ["bash", "zsh", "fish"] {
            machine.shell(name, Some(&machine.wrapper()), "/usr/bin:/bin");
        }
        assert!(machine.findings().is_empty(), "{:#?}", machine.findings());
    }

    #[test]
    fn a_shell_that_finds_homebrews_yadm_instead_is_broken() {
        let machine = Machine::new();
        machine.shell("zsh", Some("/opt/homebrew/bin/yadm"), "/opt/homebrew/bin");

        let findings = machine.findings();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        let finding = findings.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("zsh resolves yadm to"));
        assert!(
            finding.summary.as_str().contains("/opt/homebrew/bin/yadm"),
            "{finding:#?}"
        );
    }

    #[test]
    fn a_shell_that_finds_no_yadm_at_all_says_so_differently() {
        let machine = Machine::new();
        machine.shell("fish", None, "/usr/bin");

        let findings = machine.findings();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        assert!(
            findings
                .first()
                .is_some_and(|finding| finding.summary.as_str().contains("nothing at all")),
            "{findings:#?}"
        );
    }

    #[test]
    fn a_shell_that_never_reaches_the_marker_did_not_start() {
        let machine = Machine::new();
        // Everything before the marker, and then nothing — an rc file that died
        // halfway leaves exactly this.
        machine.script(
            "bash",
            &format!("#!/bin/sh\nprintf '{YADM_TAG}%s\\n' /somewhere\necho boom >&2\n"),
        );

        let findings = machine.findings();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        let finding = findings.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("did not complete"));
        assert!(
            finding
                .detail
                .as_ref()
                .is_some_and(|why| why.as_str().contains("boom")),
            "what the shell said is the whole diagnosis"
        );
    }

    #[test]
    fn a_shell_that_never_returns_is_a_startup_that_did_not_complete() {
        let machine = Machine::new();
        machine.script("zsh", "#!/bin/sh\nsleep 30\n");

        let started = std::time::Instant::now();
        let findings = machine.findings();
        assert!(
            started.elapsed() < Duration::from_secs(5),
            "a hanging shell must not hang the audit"
        );
        assert_eq!(
            findings.first().map(|finding| finding.severity),
            Some(Severity::Broken)
        );
        assert!(
            findings
                .first()
                .is_some_and(|finding| finding.summary.as_str().contains("did not complete")),
            "from outside, a hang and a very slow start are the same fact"
        );
    }

    #[test]
    fn a_shells_own_chatter_is_not_read_as_an_answer() {
        let machine = Machine::new();
        machine.script(
            "fish",
            &format!(
                "#!/bin/sh\n\
                 echo 'Welcome to fish, the friendly interactive shell'\n\
                 echo '{YADM_TAG} is a red herring in prose'\n\
                 printf '{YADM_TAG}%s\\n' '{}'\n\
                 echo {DONE_TAG}\n\
                 printf '{PATH_TAG}%s\\n' '/usr/bin'\n",
                machine.wrapper()
            ),
        );
        // The first tagged line wins, and it is the herring — which is the point:
        // a tag is a convention, not a guarantee, and the first hit is what the
        // shell would actually have run.
        let findings = machine.findings();
        assert_eq!(findings.len(), 1);
        assert!(
            findings
                .first()
                .is_some_and(|finding| finding.summary.as_str().contains("red herring")),
            "{findings:#?}"
        );
    }

    #[test]
    fn a_duplicated_path_entry_is_soft_and_named_once() {
        let machine = Machine::new();
        machine.shell(
            "bash",
            Some(&machine.wrapper()),
            "/usr/bin:/bin:/usr/bin:/bin:/opt/x",
        );

        let findings = machine.findings();
        assert_eq!(findings.len(), 1, "{findings:#?}");
        let finding = findings.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Soft);
        assert!(finding.summary.as_str().contains("2 PATH entries"));
        let detail = finding.detail.as_ref().expect("the entries");
        assert_eq!(detail.as_str(), "/usr/bin\n/bin");
    }

    #[test]
    fn duplicates_are_named_once_each_in_the_order_they_first_repeat() {
        let path: Vec<Utf8PathBuf> = ["/a", "/b", "/a", "/c", "/b", "/a"]
            .iter()
            .map(Utf8PathBuf::from)
            .collect();
        assert_eq!(duplicates(&path), vec!["/a".to_owned(), "/b".to_owned()]);
        assert!(duplicates(&[]).is_empty());
    }

    #[test]
    fn every_shell_is_reported_and_one_bad_one_does_not_hide_the_rest() {
        let machine = Machine::new();
        machine.shell("bash", Some(&machine.wrapper()), "/usr/bin");
        machine.shell("zsh", Some("/opt/homebrew/bin/yadm"), "/usr/bin");
        machine.shell("fish", Some(&machine.wrapper()), "/usr/bin:/usr/bin");

        let findings = machine.findings();
        assert_eq!(findings.len(), 2, "{findings:#?}");
        assert!(
            findings
                .iter()
                .any(|f| f.summary.as_str().starts_with("zsh"))
        );
        assert!(
            findings
                .iter()
                .any(|f| f.summary.as_str().starts_with("fish"))
        );
    }

    #[test]
    fn the_fish_probe_joins_a_list_and_the_posix_one_does_not() {
        assert!(Dialect::Fish.probe().contains("string join : --"));
        assert!(Dialect::Posix.probe().contains("\"$PATH\""));
        for dialect in [Dialect::Posix, Dialect::Fish] {
            let probe = dialect.probe();
            assert!(probe.contains(YADM_TAG));
            assert!(probe.contains(DONE_TAG));
            assert!(probe.contains(PATH_TAG));
        }
    }
}
