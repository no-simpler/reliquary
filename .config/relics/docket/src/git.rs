//! The git layer: the only place in this crate that names git.
//!
//! Additive by construction. Everything above asks [`detect`] first and takes
//! the ungit path when it answers `None` — project keying falls back to the
//! working directory, and the depot simply has no history. The one exception is
//! closing, which is refused without a repository rather than performed blind.
//!
//! Two rules bind every caller:
//!
//! - **Lock ordering.** The depot lock is coarser than the per-project lock and
//!   is always taken first. [`Repo`] never takes a lock itself; `cmd` holds the
//!   depot lock for the whole of a mutating command.
//! - **No ambient repository.** Every invocation is built by [`Git::command`],
//!   which strips the inherited `GIT_*` environment. Without that, docket run
//!   from inside a git hook answers for that hook's repository.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::OnceLock;

use anyhow::{Context, Result, anyhow, bail};

use crate::id::Id;

/// Overrides the binary, and disables the layer outright when set to nothing.
/// Tests reach the ungit path through it.
const OVERRIDE: &str = "DOCKET_GIT";

/// Written into the depot repository's own config, so no invocation has to
/// carry them and no commit depends on the user's identity.
const CONFIG: &[(&str, &str)] = &[
    ("user.name", "docket"),
    ("user.email", "docket@localhost"),
    // This machine signs through a 1Password-backed program: a signed depot
    // commit means a Touch ID prompt, or a failure, on every mutation.
    ("commit.gpgsign", "false"),
    ("tag.gpgsign", "false"),
    // The machine-wide excludes file must never silently drop an item.
    ("core.excludesFile", ""),
    // Nothing templated or global runs inside the depot. The path is relative
    // to the working tree and deliberately absent, and sits under .git so it
    // could never be mistaken for depot content.
    ("core.hooksPath", ".git/no-hooks"),
    // Auto-gc must not print into a session-start hook. scripts/update.sh
    // compacts instead.
    ("gc.auto", "0"),
];

/// Kept out of history: the lock files are transient, and the temporary files
/// atomic writes leave behind belong to a write in flight. Repo-local, so the
/// depot an agent reads stays free of an ignore file.
const EXCLUDE: &str = ".lock\n*.tmp\n";

/// Git as a capability. Zero-sized: holding one is the proof that git answered.
#[derive(Clone, Copy)]
pub struct Git;

/// One `git --version` per process, whatever asks.
pub fn detect() -> Option<Git> {
    static FOUND: OnceLock<Option<Git>> = OnceLock::new();
    *FOUND.get_or_init(|| {
        if std::env::var_os(OVERRIDE).is_some_and(|value| value.is_empty()) {
            return None;
        }
        Git.command()
            .arg("--version")
            .output()
            .ok()
            .filter(|out| out.status.success())
            .map(|_| Git)
    })
}

fn program() -> PathBuf {
    match std::env::var_os(OVERRIDE) {
        Some(value) if !value.is_empty() => PathBuf::from(value),
        _ => PathBuf::from("git"),
    }
}

impl Git {
    /// The single constructor. Strips the ambient repository so `-C` is the
    /// only thing that decides which tree is acted on, and forbids anything
    /// that could block on a prompt.
    fn command(self) -> Command {
        let mut command = Command::new(program());
        for key in [
            "GIT_DIR",
            "GIT_WORK_TREE",
            "GIT_INDEX_FILE",
            "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY",
            "GIT_ALTERNATE_OBJECT_DIRECTORIES",
            "GIT_CEILING_DIRECTORIES",
            "GIT_PREFIX",
            "GIT_CONFIG",
            "GIT_CONFIG_GLOBAL",
            "GIT_CONFIG_COUNT",
            "GIT_INDEX_VERSION",
        ] {
            command.env_remove(key);
        }
        command
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .stdin(Stdio::null());
        command
    }

    fn at(self, dir: &Path) -> Command {
        let mut command = self.command();
        command.arg("-C").arg(dir);
        command
    }

    /// The main checkout root of the repository containing `cwd`. Linked
    /// worktrees fold into it, because `git worktree list` reports the main
    /// checkout first; a submodule reports its own root, which is what a
    /// per-aspect repository layout needs.
    pub fn main_worktree(self, cwd: &Path) -> Option<PathBuf> {
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
        Some(PathBuf::from(main))
    }
}

/// The depot's history. One repository covers every project's docket, because
/// the depot is one tree and its commits read better in one log.
pub struct Repo {
    git: Git,
    root: PathBuf,
}

impl Repo {
    /// Opens the depot's repository, creating it on first use. `None` when git
    /// is absent — every caller treats that as "no history", except closing.
    ///
    /// Creation is also the migration: the tree is committed as it stands, and
    /// only then is the retired archive shelf removed, so what sat on it is in
    /// history before it leaves the disk.
    pub fn ensure(root: &Path) -> Result<Option<Repo>> {
        let Some(git) = detect() else {
            return Ok(None);
        };
        if !root.is_dir() {
            return Ok(None);
        }
        let repo = Repo {
            git,
            root: root.to_owned(),
        };
        if root.join(".git").exists() {
            return Ok(Some(repo));
        }
        repo.init()?;
        Ok(Some(repo))
    }

    /// Opens only an existing repository, and never creates one. Reads use this
    /// so a listing is never the thing that writes a repository into the depot.
    pub fn open(root: &Path) -> Option<Repo> {
        let git = detect()?;
        root.join(".git").exists().then(|| Repo {
            git,
            root: root.to_owned(),
        })
    }

    fn run<S: AsRef<OsStr>>(&self, args: &[S]) -> Result<Output> {
        let output = self
            .git
            .at(&self.root)
            .args(args)
            .output()
            .with_context(|| format!("running git in {}", self.root.display()))?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git failed: {}", stderr.trim());
        }
        Ok(output)
    }

    fn stdout<S: AsRef<OsStr>>(&self, args: &[S]) -> Result<String> {
        let output = self.run(args)?;
        String::from_utf8(output.stdout).context("git printed something that is not utf-8")
    }

    fn init(&self) -> Result<()> {
        let created = self
            .git
            .command()
            .args(["-c", "init.defaultBranch=main", "init", "--quiet"])
            .arg(&self.root)
            .output()
            .with_context(|| format!("initialising {}", self.root.display()))?;
        if !created.status.success() {
            bail!(
                "git could not initialise {}: {}",
                self.root.display(),
                String::from_utf8_lossy(&created.stderr).trim()
            );
        }
        for (key, value) in CONFIG {
            self.run(&["config", key, value])?;
        }
        let exclude = self.root.join(".git").join("info").join("exclude");
        if let Some(parent) = exclude.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&exclude, EXCLUDE)
            .with_context(|| format!("writing {}", exclude.display()))?;

        self.snapshot("init depot")?;
        self.retire_archive_shelves()?;
        Ok(())
    }

    /// The archive shelf, retired. Called once, after the initial commit has
    /// already recorded everything that sat on it.
    fn retire_archive_shelves(&self) -> Result<()> {
        let mut shelves = Vec::new();
        let Ok(entries) = std::fs::read_dir(&self.root) else {
            return Ok(());
        };
        for entry in entries.flatten() {
            let shelf = entry.path().join("archive");
            if shelf.is_dir() {
                shelves.push(shelf);
            }
        }
        if shelves.is_empty() {
            return Ok(());
        }
        for shelf in &shelves {
            let relative = shelf.strip_prefix(&self.root).unwrap_or(shelf);
            self.run(&[
                OsStr::new("rm"),
                OsStr::new("-r"),
                OsStr::new("--quiet"),
                OsStr::new("--ignore-unmatch"),
                OsStr::new("--"),
                relative.as_os_str(),
            ])?;
            // Untracked leftovers survive `git rm --ignore-unmatch`, and the
            // shelf must be gone from the disk either way.
            let _ = std::fs::remove_dir_all(shelf);
        }
        self.commit("migrate: retire the archive shelf")?;
        Ok(())
    }

    /// Stages the whole depot and commits when anything differs. Returns the
    /// commit, or `None` when there was nothing to record.
    pub fn snapshot(&self, message: &str) -> Result<Option<String>> {
        self.run(&["add", "-A"])?;
        self.commit(message)
    }

    fn commit(&self, message: &str) -> Result<Option<String>> {
        if self.run(&["diff", "--cached", "--quiet"]).is_ok() {
            return Ok(None);
        }
        self.run(&["commit", "--quiet", "--no-verify", "-m", message])?;
        Ok(Some(self.head()?))
    }

    fn head(&self) -> Result<String> {
        Ok(self
            .stdout(&["rev-parse", "--short", "HEAD"])?
            .trim()
            .into())
    }

    /// Which ids the working tree has drifted on, for a snapshot's message.
    pub fn drifted_ids(&self) -> Result<Vec<Id>> {
        let porcelain = self.stdout(&["status", "--porcelain", "-z"])?;
        let mut ids = Vec::new();
        for entry in porcelain.split('\0').filter(|e| e.len() > 3) {
            // "XY <path>", and a rename's second path follows in its own field.
            for id in ids_in(&entry[3..]) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        ids.sort();
        Ok(ids)
    }

    /// Whether history already holds this path exactly as it sits on disk. The
    /// precondition for removing it.
    pub fn is_recorded(&self, path: &Path) -> Result<bool> {
        let Ok(relative) = path.strip_prefix(&self.root) else {
            return Ok(false);
        };
        let tracked = self.stdout(&[
            OsStr::new("ls-files"),
            OsStr::new("--"),
            relative.as_os_str(),
        ])?;
        if tracked.trim().is_empty() {
            return Ok(false);
        }
        let dirty = self.stdout(&[
            OsStr::new("status"),
            OsStr::new("--porcelain"),
            OsStr::new("--"),
            relative.as_os_str(),
        ])?;
        Ok(dirty.trim().is_empty())
    }

    /// Removes a path from the tree and records the removal. History keeps it.
    ///
    /// Everything else outstanding is staged first, so a removal that comes
    /// paired with a write — a relay minting its successor — lands in one
    /// commit rather than leaving half the exchange unrecorded.
    pub fn remove(&self, path: &Path, message: &str) -> Result<String> {
        let relative = path
            .strip_prefix(&self.root)
            .map_err(|_| anyhow!("{} is not inside the depot", path.display()))?;
        self.run(&["add", "-A"])?;
        self.run(&[
            OsStr::new("rm"),
            OsStr::new("-r"),
            OsStr::new("--quiet"),
            OsStr::new("--"),
            relative.as_os_str(),
        ])?;
        self.commit(message)?
            .ok_or_else(|| anyhow!("git recorded no removal for {}", path.display()))
    }

    /// Every id any commit ever added, so a closed item's id is never minted a
    /// second time. History is the ledger; nothing else has to be.
    pub fn ids_ever(&self) -> Result<Vec<Id>> {
        let log = self.stdout(&[
            "log",
            "--all",
            "--diff-filter=A",
            "--name-only",
            "--pretty=format:",
        ])?;
        let mut ids = Vec::new();
        for line in log.lines().filter(|line| !line.trim().is_empty()) {
            for id in ids_in(line) {
                if !ids.contains(&id) {
                    ids.push(id);
                }
            }
        }
        Ok(ids)
    }

    /// Directories inside the depot that hold a repository of their own. Git
    /// records one as a gitlink rather than as content, so an item under it
    /// would have no history at all.
    pub fn nested_repositories(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        walk(&self.root, 0, &mut found);
        found
    }
}

/// Item ids appearing in one depot-relative path. A handoff or relay names its
/// id in the filename, a spec in its directory, so both are covered by reading
/// the leading field of every component.
fn ids_in(path: &str) -> Vec<Id> {
    path.split('/')
        .filter_map(|component| component.split('-').next())
        .filter_map(|head| head.parse::<Id>().ok())
        .collect()
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<PathBuf>) {
    // Deep enough for a spec's own subdirectories, shallow enough that the
    // check stays free.
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name() == Some(OsStr::new(".git")) {
            if depth > 0 {
                found.push(path);
            }
            continue;
        }
        walk(&path, depth + 1, found);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_come_out_of_both_shapes() {
        assert_eq!(
            ids_in("-Users-x/handoffs/b71c-a-title.md")
                .iter()
                .map(Id::to_string)
                .collect::<Vec<_>>(),
            vec!["b71c"]
        );
        assert_eq!(
            ids_in("-Users-x/specs/4mve-a-title/spec.md")
                .iter()
                .map(Id::to_string)
                .collect::<Vec<_>>(),
            vec!["4mve"]
        );
    }

    #[test]
    fn paths_without_an_id_yield_none() {
        assert!(ids_in("-Users-x/.project").is_empty());
        assert!(ids_in("handoffs/notanid-title.md").is_empty());
    }
}
