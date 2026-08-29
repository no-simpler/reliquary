//! An external program as a capability.
//!
//! Holding a [`Tool`] is the proof that the program exists; a relic that cannot
//! work without one refuses instead of proceeding blind, and one that can takes
//! the other path on `None`. That is the same bargain [`crate::git`] strikes,
//! generalised — `Git` is built on this.
//!
//! Two properties are guaranteed **by construction**, because a caller who has
//! to remember them is a caller who will not:
//!
//! - **The locale is `C`.** House rule: never parse human-facing output. Where a
//!   machine-readable interface exists — `--porcelain`, `-z`, `--format json`,
//!   an exit code — use it. Where none does, the message must at least be the
//!   one the parser was written against, not whatever the user's locale renders.
//! - **stdin is closed.** A relic runs from session hooks and from `up`. A
//!   program that stops to ask something there is a hang, not a question.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};

use camino::Utf8Path;
use std::process::{Command, Stdio};

/// Why a tool run did not produce an answer.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The program could not be started.
    #[error("running {program}")]
    Spawn {
        /// What was being run.
        program: String,
        /// The underlying failure.
        #[source]
        source: std::io::Error,
    },
    /// It ran and refused.
    #[error("{program} failed ({code}): {stderr}")]
    Failed {
        /// What was run.
        program: String,
        /// Its exit status, or `signal` when it was killed.
        code: String,
        /// What it said, trimmed.
        stderr: String,
    },
    /// It answered with bytes that are not text.
    #[error("{program} produced output that is not UTF-8")]
    NotUtf8 {
        /// What was run.
        program: String,
    },
    /// It outlasted the budget it was given and was killed.
    #[error("{program} did not answer within {}ms", budget.as_millis())]
    TimedOut {
        /// What was run.
        program: String,
        /// How long it was given.
        budget: std::time::Duration,
    },
}

/// What a tool said, and how it exited — for a caller to whom the exit status
/// is **data** rather than a verdict.
///
/// [`Tool::capture`] is right for a program whose non-zero exit means it
/// failed, which is most of them. It is wrong for one whose exit status carries
/// an answer: [`crate::finding::Grade`] is reported as `0`/`1`/`2`, so a health
/// check that found something exits non-zero *by design*, and a caller reading
/// that as a failure would discard every report that had anything in it.
#[derive(Debug, Clone)]
pub struct Exit {
    /// Its exit status, or `None` when a signal killed it.
    pub code: Option<i32>,
    /// Standard output, verbatim.
    pub stdout: String,
    /// Standard error, verbatim.
    pub stderr: String,
}

impl Exit {
    /// Whether it exited zero.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.code == Some(0)
    }
}

/// What a tool said.
#[derive(Debug, Clone)]
pub struct Output {
    /// Standard output, verbatim.
    pub stdout: String,
    /// Standard error, verbatim.
    pub stderr: String,
}

impl Output {
    /// Standard output with trailing whitespace removed — what a one-line
    /// answer actually is.
    #[must_use]
    pub fn line(&self) -> &str {
        self.stdout.trim_end()
    }
}

/// An external program, proven present.
#[derive(Debug, Clone)]
pub struct Tool {
    name: String,
    program: PathBuf,
}

impl Tool {
    /// Resolve `name` on `PATH`.
    ///
    /// Returns `None` when it is not there, which is a fact rather than a
    /// failure: a relic decides for itself whether it can carry on without.
    #[must_use]
    pub fn find(name: &str) -> Option<Self> {
        which::which(name).ok().map(|program| Self {
            name: name.to_owned(),
            program,
        })
    }

    /// Resolve `name`, letting an override replace or disable it.
    ///
    /// The override is a **seam**, not a convenience: a path in it replaces the
    /// program, and an *empty* value disables the tool outright, which is how a
    /// caller reaches the code path taken when the program is absent without
    /// uninstalling it.
    ///
    /// Takes the value rather than reading it, so the rule is testable without
    /// mutating a process environment no two tests can safely share.
    #[must_use]
    pub fn resolve(name: &str, over: Option<&OsStr>) -> Option<Self> {
        match over {
            Some(value) if value.is_empty() => None,
            Some(value) => Some(Self {
                name: name.to_owned(),
                program: PathBuf::from(value),
            }),
            None => Self::find(name),
        }
    }

    /// [`Tool::resolve`], reading the override from the environment.
    ///
    /// The one place that touches the environment, so every other path is pure.
    #[must_use]
    pub fn find_with_override(name: &str, var: &str) -> Option<Self> {
        Self::resolve(name, std::env::var_os(var).as_deref())
    }

    /// Take a program on trust, without looking for it.
    ///
    /// For a caller that has already proven presence some other way. Prefer
    /// [`Tool::find`], which proves it here.
    #[must_use]
    pub fn at_path(name: &str, program: PathBuf) -> Self {
        Self {
            name: name.to_owned(),
            program,
        }
    }

    /// What this tool is called.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// The resolved program.
    #[must_use]
    pub fn program(&self) -> &Path {
        &self.program
    }

    /// A command carrying both guarantees: `C` locale, closed stdin.
    #[must_use]
    pub fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command
            .env("LC_ALL", "C")
            .env("LANG", "C")
            .env_remove("LANGUAGE")
            .stdin(Stdio::null());
        command
    }

    /// [`Tool::command`], run from `dir`.
    #[must_use]
    pub fn in_dir(&self, dir: &Utf8Path) -> Command {
        let mut command = self.command();
        command.current_dir(dir);
        command
    }

    /// Run it, and kill it if it outlasts `budget`.
    ///
    /// The bound is the same rule [`crate::lock`] states for waiting on a file:
    /// **bound the wait, never the hold.** A relic that asks another program a
    /// question runs from session hooks and from `up`, where a program that
    /// never answers is a hang rather than a slow answer — and a caller cannot
    /// tell the difference by waiting longer.
    ///
    /// Both pipes are drained on threads while the wait runs. A child that
    /// fills a pipe buffer blocks on the write, and a parent that waited
    /// without reading would time out a program that was answering perfectly
    /// well.
    ///
    /// # Errors
    ///
    /// Everything [`Tool::capture`] reports, plus [`Error::TimedOut`] when the
    /// budget runs out. A timed-out child is killed and reaped, so nothing is
    /// left running behind the answer.
    pub fn capture_within(
        &self,
        command: &mut Command,
        budget: std::time::Duration,
    ) -> Result<Output, Error> {
        let exit = self.run_within(command, budget)?;
        if !exit.ok() {
            return Err(Error::Failed {
                program: self.name.clone(),
                code: match exit.code {
                    Some(code) => code.to_string(),
                    None => "signal".to_owned(),
                },
                stderr: exit.stderr.trim().to_owned(),
            });
        }
        Ok(Output {
            stdout: exit.stdout,
            stderr: exit.stderr,
        })
    }

    /// Run it within `budget`, handing it `input` on stdin, and report how it
    /// exited without judging that.
    ///
    /// Some programs take their argument on stdin **because** the argument is a
    /// secret: argv is world-readable through `ps`, and a temporary file is a
    /// secret at rest. `ssh-add -` is the case this exists for.
    ///
    /// The write runs on its own thread. A child that answers before reading
    /// everything, or one whose output fills a pipe buffer while this process is
    /// still writing, deadlocks a caller that writes and then waits.
    ///
    /// # Errors
    ///
    /// [`Error::Spawn`] when it could not start and [`Error::TimedOut`] when it
    /// outlasted the budget. A broken pipe is not an error here: the child
    /// exiting early is the answer, and its status carries it.
    pub fn feed_within(
        &self,
        command: &mut Command,
        input: &str,
        budget: std::time::Duration,
    ) -> Result<Exit, Error> {
        command.stdin(Stdio::piped());
        self.wait_within(command, Some(input.to_owned()), budget)
    }

    /// Run it within `budget` and report how it exited, without judging that.
    ///
    /// See [`Exit`] for when a status is an answer rather than a failure. Both
    /// streams are decoded lossily: a caller reading a status is reading the
    /// output for a shape, and one byte that is not text is no reason to lose
    /// the rest of it.
    ///
    /// # Errors
    ///
    /// [`Error::Spawn`] when it could not start and [`Error::TimedOut`] when it
    /// outlasted the budget. Nothing else — exiting non-zero *is* the answer.
    pub fn run_within(
        &self,
        command: &mut Command,
        budget: std::time::Duration,
    ) -> Result<Exit, Error> {
        self.wait_within(command, None, budget)
    }

    /// The one bounded wait. `input` is written to stdin when the caller has
    /// opened one.
    fn wait_within(
        &self,
        command: &mut Command,
        input: Option<String>,
        budget: std::time::Duration,
    ) -> Result<Exit, Error> {
        let mut child = command
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| Error::Spawn {
                program: self.name.clone(),
                source,
            })?;

        // Detached, not scoped. A scope joins before it returns, and after a
        // timeout these threads may not return at all: killing a shell does not
        // kill the `sleep` it started, and that grandchild still holds the write
        // end of the pipe. Nothing portable kills a process this one did not
        // start, so the readers are left to end when the pipe finally closes,
        // and the caller is not made to wait for it.
        // Detached for the same reason the readers are: a child that never
        // reads leaves this blocked on the write, and the caller asked for a
        // bounded wait.
        if let Some(input) = input {
            let pipe = child.stdin.take();
            std::thread::spawn(move || {
                if let Some(mut pipe) = pipe {
                    use std::io::Write as _;
                    let _ = pipe.write_all(input.as_bytes());
                }
            });
        }
        let out = std::thread::spawn({
            let pipe = child.stdout.take();
            move || drain(pipe)
        });
        let err = std::thread::spawn({
            let pipe = child.stderr.take();
            move || drain(pipe)
        });
        let deadline = std::time::Instant::now() + budget;

        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break Some(status),
                Ok(None) => {}
                Err(_) => break None,
            }
            if std::time::Instant::now() >= deadline {
                // Killed and then reaped: a child left unreaped is a zombie,
                // and a child left running is the side effect this bound exists
                // to prevent.
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
            std::thread::sleep(POLL);
        };

        let Some(status) = status else {
            return Err(Error::TimedOut {
                program: self.name.clone(),
                budget,
            });
        };
        Ok(Exit {
            code: status.code(),
            stdout: String::from_utf8_lossy(&out.join().unwrap_or_default()).into_owned(),
            stderr: String::from_utf8_lossy(&err.join().unwrap_or_default()).into_owned(),
        })
    }

    /// Run it and take what it said.
    ///
    /// # Errors
    ///
    /// [`Error::Spawn`] when it could not start, [`Error::Failed`] when it
    /// refused, [`Error::NotUtf8`] when its answer is not text.
    pub fn capture(&self, command: &mut Command) -> Result<Output, Error> {
        let output = command.output().map_err(|source| Error::Spawn {
            program: self.name.clone(),
            source,
        })?;
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        if !output.status.success() {
            return Err(Error::Failed {
                program: self.name.clone(),
                code: match output.status.code() {
                    Some(code) => code.to_string(),
                    None => "signal".to_owned(),
                },
                stderr: stderr.trim().to_owned(),
            });
        }
        let stdout = String::from_utf8(output.stdout).map_err(|_| Error::NotUtf8 {
            program: self.name.clone(),
        })?;
        Ok(Output { stdout, stderr })
    }
}

/// Everything a pipe holds, or nothing when there was no pipe to read.
///
/// A read error is not distinguished from an empty pipe: the caller is about to
/// judge the child by its exit status, and a half-read stream is reported as
/// what arrived rather than as a failure of this process.
fn drain<R: std::io::Read>(pipe: Option<R>) -> Vec<u8> {
    let mut buffer = Vec::new();
    if let Some(mut pipe) = pipe {
        let _ = pipe.read_to_end(&mut buffer);
    }
    buffer
}

/// How often a bounded wait looks at a child that has not finished.
///
/// Short enough that a fast program is not held up by the granularity, long
/// enough that waiting costs no measurable CPU.
const POLL: std::time::Duration = std::time::Duration::from_millis(5);

#[cfg(test)]
mod feeding {
    use super::*;

    fn cat() -> Tool {
        Tool::find("cat").expect("cat is on PATH everywhere this runs")
    }

    #[test]
    fn what_is_written_to_stdin_comes_back_out() {
        let tool = cat();
        let mut command = tool.command();
        let exit = tool
            .feed_within(&mut command, "secret\n", std::time::Duration::from_secs(10))
            .expect("cat ran");
        assert_eq!(exit.stdout, "secret\n");
        assert!(exit.ok());
    }

    #[test]
    fn a_child_that_never_reads_does_not_hold_the_writer() {
        // `true` exits without touching stdin, so the write gets a broken pipe.
        // A caller that wrote before waiting would block here forever on input
        // larger than the pipe buffer.
        let tool = Tool::find("true").expect("true is on PATH");
        let mut command = tool.command();
        let big = "x".repeat(1024 * 1024);
        let exit = tool
            .feed_within(&mut command, &big, std::time::Duration::from_secs(10))
            .expect("true ran");
        assert!(exit.ok());
    }

    #[test]
    fn a_fed_program_is_still_bounded() {
        let tool = Tool::find("sh").expect("sh is on PATH");
        let mut command = tool.command();
        command.args(["-c", "sleep 30"]);
        let error = tool
            .feed_within(
                &mut command,
                "ignored",
                std::time::Duration::from_millis(200),
            )
            .expect_err("it outlasts the budget");
        assert!(matches!(error, Error::TimedOut { .. }), "{error:?}");
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsStr;

    use super::*;

    fn envs(command: &Command) -> HashMap<&OsStr, Option<&OsStr>> {
        command.get_envs().collect()
    }

    fn echo() -> Tool {
        Tool::find("echo").expect("echo is on PATH everywhere this runs")
    }

    #[test]
    fn a_command_is_born_in_the_c_locale() {
        let command = echo().command();
        let env = envs(&command);
        assert_eq!(env.get(OsStr::new("LC_ALL")), Some(&Some(OsStr::new("C"))));
        assert_eq!(env.get(OsStr::new("LANG")), Some(&Some(OsStr::new("C"))));
        assert_eq!(env.get(OsStr::new("LANGUAGE")), Some(&None));
    }

    #[test]
    fn a_bounded_run_answers_like_an_unbounded_one() {
        let tool = echo();
        let mut command = tool.command();
        command.arg("hello");
        let answer = tool
            .capture_within(&mut command, std::time::Duration::from_secs(10))
            .expect("echo answers");
        assert_eq!(answer.line(), "hello");
    }

    #[test]
    fn a_program_that_never_answers_is_killed_rather_than_waited_on() {
        let Some(tool) = Tool::find("sleep") else {
            return;
        };
        let mut command = tool.command();
        command.arg("30");
        let started = std::time::Instant::now();
        let error = tool
            .capture_within(&mut command, std::time::Duration::from_millis(120))
            .expect_err("a sleeping program does not answer");
        assert!(
            matches!(error, Error::TimedOut { .. }),
            "{error:?} is not a timeout"
        );
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "the bound was not honoured"
        );
    }

    #[test]
    fn an_answer_larger_than_a_pipe_buffer_is_not_mistaken_for_a_hang() {
        let Some(tool) = Tool::find("sh") else {
            return;
        };
        let mut command = tool.command();
        // Well past the 64 KiB a pipe holds, so a parent that waited without
        // reading would deadlock and then report a timeout.
        command.args(["-c", "yes abcdefghij | head -n 40000"]);
        let answer = tool
            .capture_within(&mut command, std::time::Duration::from_secs(20))
            .expect("a large answer is still an answer");
        assert_eq!(answer.stdout.lines().count(), 40_000);
    }

    #[test]
    fn a_bounded_refusal_reports_the_code_and_the_message() {
        let Some(tool) = Tool::find("sh") else {
            return;
        };
        let mut command = tool.command();
        command.args(["-c", "echo nope >&2; exit 3"]);
        let error = tool
            .capture_within(&mut command, std::time::Duration::from_secs(10))
            .expect_err("it refused");
        let Error::Failed { code, stderr, .. } = error else {
            panic!("expected a refusal");
        };
        assert_eq!(code, "3");
        assert_eq!(stderr, "nope");
    }

    #[test]
    fn a_non_zero_exit_is_an_answer_rather_than_a_failure() {
        let Some(tool) = Tool::find("sh") else {
            return;
        };
        let mut command = tool.command();
        command.args(["-c", "echo answered; exit 2"]);
        let exit = tool
            .run_within(&mut command, std::time::Duration::from_secs(10))
            .expect("running is not judging");
        assert_eq!(exit.code, Some(2));
        assert!(!exit.ok());
        assert_eq!(
            exit.stdout.trim(),
            "answered",
            "a program whose status carries a verdict still has something to say"
        );
    }

    #[test]
    fn a_grandchild_holding_the_pipe_does_not_extend_the_bound() {
        let Some(tool) = Tool::find("sh") else {
            return;
        };
        let mut command = tool.command();
        // Killing the shell leaves `sleep` running with the write end of the
        // pipe, which is why the readers cannot be joined before returning.
        command.args(["-c", "sleep 30"]);
        let started = std::time::Instant::now();
        let error = tool
            .run_within(&mut command, std::time::Duration::from_millis(150))
            .expect_err("it never answered");
        assert!(matches!(error, Error::TimedOut { .. }), "{error:?}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "waited {:?}, so the reader threads were joined after all",
            started.elapsed()
        );
    }

    #[test]
    fn a_missing_program_is_a_fact_not_a_failure() {
        assert!(Tool::find("relic-core-no-such-program").is_none());
    }

    #[test]
    fn an_empty_override_disables_the_tool() {
        assert!(Tool::resolve("echo", Some(OsStr::new(""))).is_none());
    }

    #[test]
    fn an_override_replaces_the_program_without_searching() {
        let tool = Tool::resolve("git", Some(OsStr::new("/nowhere/git"))).expect("named");
        assert_eq!(tool.program(), Path::new("/nowhere/git"));
        assert_eq!(tool.name(), "git");
    }

    #[test]
    fn no_override_falls_through_to_the_path_search() {
        assert!(Tool::resolve("echo", None).is_some());
        assert!(Tool::resolve("relic-core-no-such-program", None).is_none());
    }

    #[test]
    fn capture_returns_what_the_tool_said() {
        let tool = echo();
        let output = tool
            .capture(tool.command().arg("hello"))
            .expect("echo runs");
        assert_eq!(output.line(), "hello");
    }

    #[test]
    fn a_refusal_carries_the_code_and_the_message() {
        let Some(tool) = Tool::find("false") else {
            return;
        };
        let err = tool.capture(&mut tool.command()).unwrap_err();
        assert!(matches!(err, Error::Failed { .. }), "{err:?}");
    }

    #[test]
    fn in_dir_runs_where_it_was_told() {
        let Some(tool) = Tool::find("pwd") else {
            return;
        };
        let dir = crate::path::utf8(std::env::temp_dir()).expect("nameable");
        let output = tool
            .capture(&mut tool.in_dir(&dir))
            .expect("pwd runs anywhere");
        assert!(!output.line().is_empty());
    }
}
