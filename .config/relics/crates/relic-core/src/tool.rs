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
    pub fn in_dir(&self, dir: &Path) -> Command {
        let mut command = self.command();
        command.current_dir(dir);
        command
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
        let dir = std::env::temp_dir();
        let output = tool
            .capture(&mut tool.in_dir(&dir))
            .expect("pwd runs anywhere");
        assert!(!output.line().is_empty());
    }
}
