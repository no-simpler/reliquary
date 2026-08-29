//! Git as a capability, and the one constructor allowed to invoke it.
//!
//! **No ambient repository.** Every invocation is built by [`Git::command`],
//! which strips the inherited `GIT_*` environment, so `-C` is the only thing
//! that decides which tree is acted on. Without that, a relic run from inside a
//! git hook answers for that hook's repository — which is not the repository the
//! user is in, and not the project the relic is being asked about. That part is
//! not a choice; [`SystemConfig`] is the one thing about an invocation that is,
//! and it governs git's behaviour rather than its subject.
//!
//! The same constructor forbids anything that could block: a relic is run from
//! session hooks and from `up`, where a credential prompt is a hang, not a
//! question.
//!
//! Holding a [`Git`] is the proof that git is there. Callers that can work
//! without it ask [`detect`] and take the ungit path on `None`; callers that
//! cannot should refuse rather than proceed blind.
//!
//! Built on [`crate::tool`], which supplies the guarantees every external
//! program needs — `C` locale, closed stdin. What is git-specific stays here.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

use camino::{Utf8Path, Utf8PathBuf};

use crate::tool::Tool;

/// Overrides the binary, and disables the layer outright when set to nothing.
/// Tests reach the ungit path through it.
const OVERRIDE: &str = "RELIC_GIT";

/// The environment that would otherwise decide which repository answers. Git
/// reads all of these ahead of `-C`, and a session hook exports the first two.
///
/// `GIT_CONFIG_PARAMETERS` is here for a channel that is easy to miss: `git -c
/// k=v …` does not pass the setting to its own process alone, it exports this
/// variable to **every** child. So a relic invoked from a hook that git itself
/// started inherits that hook's `-c` overrides unless they are stripped.
const AMBIENT: &[&str] = &[
    "GIT_DIR",
    "GIT_WORK_TREE",
    "GIT_INDEX_FILE",
    "GIT_COMMON_DIR",
    "GIT_OBJECT_DIRECTORY",
    "GIT_ALTERNATE_OBJECT_DIRECTORIES",
    "GIT_CEILING_DIRECTORIES",
    "GIT_PREFIX",
    "GIT_NAMESPACE",
    "GIT_CONFIG",
    "GIT_CONFIG_GLOBAL",
    "GIT_CONFIG_SYSTEM",
    "GIT_CONFIG_COUNT",
    "GIT_CONFIG_PARAMETERS",
    "GIT_INDEX_VERSION",
];

/// Whether an invocation reads git's machine-wide configuration.
///
/// Separate from [`AMBIENT`], and the distinction is the whole point of the
/// choice. The ambient variables are *inherited* — they vary with who invoked
/// this process, so reading them means answering for somebody else's
/// repository. The system config file does not vary that way: it is a property
/// of the machine, and stripping it changes what git *does* rather than which
/// repository it does it to.
///
/// [`SystemConfig::Ignored`] is right for a relic asking a question, where the
/// answer should not depend on machine-wide settings. It is wrong for one whose
/// git invocation **produces an artifact** — `bundle create` packs under
/// `pack.*`, `core.compression` and friends — because a backup written with the
/// machine's packing settings disabled is not the backup the machine would
/// otherwise make.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SystemConfig {
    /// `GIT_CONFIG_NOSYSTEM=1`. The default, and what a reader wants.
    Ignored,
    /// Leave it alone. What a writer of artifacts wants.
    Honoured,
}

/// Git as a capability. Zero-sized: holding one is the proof that git answered.
#[derive(Clone, Copy, Debug)]
pub struct Git;

/// One PATH resolution per process, whatever asks.
///
/// Presence, not a handshake: `which` proves an executable file, and a git that
/// is present but broken fails the first real invocation with its own message,
/// which is more legible than this returning `None`. That also keeps a fork off
/// the session-start hook path, where the whole budget is milliseconds.
#[must_use]
pub fn detect() -> Option<Git> {
    tool().map(|_| Git)
}

fn tool() -> Option<&'static Tool> {
    static FOUND: OnceLock<Option<Tool>> = OnceLock::new();
    FOUND
        .get_or_init(|| Tool::find_with_override("git", OVERRIDE))
        .as_ref()
}

impl Git {
    /// The single constructor. Strips the ambient repository so `-C` is the
    /// only thing that decides which tree is acted on, and forbids anything
    /// that could block on a prompt.
    #[must_use]
    pub fn command(self) -> Command {
        self.command_with(SystemConfig::Ignored)
    }

    /// [`Git::command`], choosing whether the machine's own git configuration
    /// is read.
    ///
    /// The ambient scrub is not a choice and never becomes one — see
    /// [`SystemConfig`] for why the two are different questions.
    #[must_use]
    pub fn command_with(self, system: SystemConfig) -> Command {
        let mut command = match tool() {
            Some(tool) => tool.command(),
            // Only reachable when a caller built a `Git` without asking
            // `detect`. Naming the program still produces a legible failure at
            // spawn rather than a silent no-op.
            None => Tool::at_path("git", PathBuf::from("git")).command(),
        };
        for key in AMBIENT {
            command.env_remove(key);
        }
        match system {
            SystemConfig::Ignored => {
                command.env("GIT_CONFIG_NOSYSTEM", "1");
            }
            SystemConfig::Honoured => {
                command.env_remove("GIT_CONFIG_NOSYSTEM");
            }
        }
        command
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0");
        command
    }

    /// [`Git::command`], aimed at one directory.
    #[must_use]
    pub fn at(self, dir: &Utf8Path) -> Command {
        let mut command = self.command();
        command.arg("-C").arg(dir);
        command
    }

    /// [`Git::command_with`], aimed at one directory.
    #[must_use]
    pub fn at_with(self, dir: &Utf8Path, system: SystemConfig) -> Command {
        let mut command = self.command_with(system);
        command.arg("-C").arg(dir);
        command
    }

    /// Run a command built here and take what it said.
    ///
    /// The same capture [`Tool`] gives every other program, reached through the
    /// constructor that strips the ambient repository. A caller that wants a
    /// typed failure — which command, which status, what it printed — has one
    /// without reassembling it from `Output` and `ExitStatus` by hand.
    ///
    /// # Errors
    ///
    /// [`crate::tool::Error`]: the program could not start, it refused, or its
    /// answer is not text.
    pub fn capture(self, command: &mut Command) -> Result<crate::tool::Output, crate::tool::Error> {
        match tool() {
            Some(tool) => tool.capture(command),
            // Unreachable through `detect`, and still not a place to panic: a
            // `Git` built without asking produces the spawn failure naming the
            // program, which is what any other caller would see.
            None => Tool::at_path("git", PathBuf::from("git")).capture(command),
        }
    }

    /// The main checkout root of the repository containing `cwd`. Linked
    /// worktrees fold into it, because `git worktree list` reports the main
    /// checkout first; a submodule reports its own root, which is what a
    /// per-aspect repository layout needs.
    #[must_use]
    pub fn main_worktree(self, cwd: &Utf8Path) -> Option<Utf8PathBuf> {
        let output = self
            .at(cwd)
            .args(["worktree", "list", "--porcelain"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let text = String::from_utf8(output.stdout).ok()?;
        let main = text.lines().next()?.strip_prefix("worktree ")?;
        if main.is_empty() {
            return None;
        }
        Some(Utf8PathBuf::from(main))
    }

    /// The branch `cwd` is on, when there is one. A detached head names no
    /// branch, and neither does a directory outside a repository.
    #[must_use]
    pub fn branch(self, cwd: &Utf8Path) -> Option<String> {
        let output = self
            .at(cwd)
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let name = String::from_utf8(output.stdout).ok()?.trim().to_owned();
        (!name.is_empty() && name != "HEAD").then_some(name)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::ffi::OsStr;

    use super::*;

    /// The rule the module doc states, asserted rather than trusted: nothing a
    /// hook exported survives into an invocation.
    #[test]
    fn the_ambient_repository_is_stripped() {
        let command = Git.command();
        let env: HashMap<&OsStr, Option<&OsStr>> = command.get_envs().collect();

        for key in AMBIENT {
            assert_eq!(
                env.get(OsStr::new(key)),
                Some(&None),
                "{key} is not removed, so a hook's repository reaches this invocation"
            );
        }

        for (key, value) in [
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_OPTIONAL_LOCKS", "0"),
        ] {
            assert_eq!(
                env.get(OsStr::new(key)),
                Some(&Some(OsStr::new(value))),
                "{key} is not set to {value}"
            );
        }
    }

    /// Honouring the machine's config is a choice about git's *behaviour*, and
    /// must not quietly become a choice about which repository answers.
    #[test]
    fn honouring_the_system_config_keeps_the_ambient_scrub() {
        let command = Git.command_with(SystemConfig::Honoured);
        let env: HashMap<&OsStr, Option<&OsStr>> = command.get_envs().collect();

        for key in AMBIENT {
            assert_eq!(
                env.get(OsStr::new(key)),
                Some(&None),
                "{key} is not removed"
            );
        }
        assert_eq!(env.get(OsStr::new("GIT_CONFIG_NOSYSTEM")), Some(&None));
        assert_eq!(
            env.get(OsStr::new("GIT_TERMINAL_PROMPT")),
            Some(&Some(OsStr::new("0")))
        );
    }

    #[test]
    fn aiming_at_a_directory_keeps_the_scrub() {
        let command = Git.at(Utf8Path::new("/tmp"));
        let env: HashMap<&OsStr, Option<&OsStr>> = command.get_envs().collect();
        assert_eq!(env.get(OsStr::new("GIT_DIR")), Some(&None));
        let args: Vec<&OsStr> = command.get_args().collect();
        assert_eq!(args, vec![OsStr::new("-C"), OsStr::new("/tmp")]);
    }
}
