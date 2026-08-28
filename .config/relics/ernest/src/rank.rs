//! Which files the ranked views cover.
//!
//! `--by file` ranks the repository, and that is the right scope for the
//! headline and the wrong one for the ranking. On the call site that dominates —
//! an agent measuring before and after its own edit across fifty files — a
//! repository-wide ranking is stationary: the same large documents lead it
//! either way, so it says nothing about the change.
//!
//! So the two scopes split. The headline stays repository-wide, because
//! relocation-invariance needs it; only the body narrows.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use anyhow::{Context, Result, bail};
use ignore::overrides::{Override, OverrideBuilder};

use crate::walk::Scope;

/// A predicate over the files a run measured, and the words the caller used to
/// ask for it.
pub struct Selection {
    /// Canonical, in argv order, so two snapshots can be told apart by a reader
    /// and by `ernest diff`.
    pub asked: String,
    focus: Option<Override>,
    changed: Option<HashSet<PathBuf>>,
}

impl Selection {
    /// `None` when nothing narrowed the ranking, so the caller can leave the
    /// whole thing alone rather than build a predicate that admits everything.
    pub fn build(
        focus: &[String],
        changed: Option<&str>,
        roots: &[PathBuf],
        scope: Scope,
    ) -> Result<Option<Self>> {
        if focus.is_empty() && changed.is_none() {
            return Ok(None);
        }

        let mut asked = Vec::new();
        if let Some(reference) = changed {
            asked.push(format!("changed={reference}"));
        }
        asked.extend(focus.iter().map(|pathspec| format!("focus={pathspec}")));

        let matcher = if focus.is_empty() {
            None
        } else {
            // Rooted at the working directory and matched against the path the
            // walk produced, which is both what the report prints and what the
            // caller typed the pathspec against.
            let cwd = std::env::current_dir().context("resolving the working directory")?;
            let mut builder = OverrideBuilder::new(&cwd);
            for pathspec in focus {
                builder
                    .add(pathspec)
                    .with_context(|| format!("--focus {pathspec}"))?;
            }
            Some(builder.build().context("building --focus")?)
        };

        let changed = changed
            .map(|reference| git::changed(roots, reference, scope))
            .transpose()?;

        Ok(Some(Selection {
            asked: asked.join(" "),
            focus: matcher,
            changed,
        }))
    }

    /// Every narrowing flag must admit the path: they intersect, because both
    /// answer "rank less than everything" and no other reading of the two
    /// together is unsurprising.
    pub fn admits(&self, path: &Path) -> bool {
        // A non-match comes back as `Match::Ignore`, per the crate's own
        // inversion of gitignore semantics — so the question is whitelisting,
        // not the absence of an ignore.
        let focused = self
            .focus
            .as_ref()
            .is_none_or(|matcher| matcher.matched(path, false).is_whitelist());

        let touched = self
            .changed
            .as_ref()
            .is_none_or(|set| normalize(path).is_some_and(|path| set.contains(&path)));

        focused && touched
    }
}

/// An absolute path with `.` and `..` collapsed, and symlinks left alone.
///
/// Neither half can be skipped. `std::path::absolute` is lexical but documents
/// that it keeps `..` on Unix, and `git rev-parse --show-cdup` hands back exactly
/// that — `../` from a subdirectory — so without the collapse the two sides never
/// compare equal and `--changed` silently ranks nothing. `canonicalize` would
/// collapse them but also resolve symlinks, and on macOS a temporary directory
/// under `/var` resolves to `/private/var`, which is the disagreement in the
/// other direction.
///
/// Collapsing `..` lexically is not the same question as resolving it, when a
/// symlink sits in the path. Both sides go through this, so they agree — which is
/// all a predicate needs.
fn normalize(path: &Path) -> Option<PathBuf> {
    use std::path::Component;

    let absolute = std::path::absolute(path).ok()?;
    let mut out = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                out.pop();
            }
            other => out.push(other),
        }
    }
    Some(out)
}

/// Asking git what changed, rather than reimplementing it — the same delegation
/// `.gitignore` already gets.
mod git {
    use relic_core::tool::Tool;

    use super::*;

    /// Paths differing from `reference`, plus the untracked files beside them.
    ///
    /// Two-dot, not three-dot: `git diff <ref>` compares the **working tree**
    /// against the reference, while merge-base semantics would answer "what this
    /// branch added" and drop uncommitted work — which is the very thing ernest's
    /// measure-edit-measure loop exists to weigh. `--changed=main` therefore
    /// means "differs from main right now, my uncommitted work included", which
    /// is the literal reading of the flag.
    ///
    /// Untracked files are queried separately because `git diff` never reports
    /// them, and a document an agent has just written is exactly the change worth
    /// ranking.
    pub fn changed(roots: &[PathBuf], reference: &str, scope: Scope) -> Result<HashSet<PathBuf>> {
        let mut touched = HashSet::new();
        let mut seen_repos = HashSet::new();

        for root in roots {
            // A root that names a file is asked about from the directory holding
            // it; `-C` wants a directory either way.
            let from = if root.is_dir() {
                root.clone()
            } else {
                root.parent().unwrap_or(Path::new(".")).to_path_buf()
            };
            let repo = repo_root(&from)?;
            if !seen_repos.insert(repo.clone()) {
                continue;
            }

            let mut listings = vec![run(&from, &["diff", "--name-only", "-z", reference, "--"])?];
            // At the widest scope a gitignored file is in the measurement, so it
            // is in the ranking too if it is new.
            listings.push(if scope == Scope::All {
                run(&from, &["ls-files", "-o", "-z", "--"])?
            } else {
                run(&from, &["ls-files", "-o", "-z", "--exclude-standard", "--"])?
            });

            for listing in listings {
                for name in listing.split('\0').filter(|name| !name.is_empty()) {
                    // git reports repository-root-relative paths, and the root
                    // itself came back as a relative walk up from `from` — so
                    // both halves need the same collapse `admits` applies.
                    if let Some(path) = normalize(&repo.join(name)) {
                        touched.insert(path);
                    }
                }
            }
        }

        Ok(touched)
    }

    /// The work tree root, as a path built lexically from `from`.
    ///
    /// `--show-cdup` rather than `--show-toplevel`, which prints the *resolved*
    /// path with symlinks followed. `std::path::absolute` is purely lexical, and
    /// on macOS a temporary directory sits under `/var`, itself a symlink to
    /// `/private/var` — so the two spellings would never compare equal and the
    /// predicate would admit nothing.
    fn repo_root(from: &Path) -> Result<PathBuf> {
        let cdup = run(from, &["rev-parse", "--show-cdup"])?;
        Ok(from.join(cdup.trim_end_matches(['\n', '\r'])))
    }

    /// One PATH resolution per process, whatever asks.
    fn tool() -> Result<&'static Tool> {
        static FOUND: OnceLock<Option<Tool>> = OnceLock::new();
        FOUND
            .get_or_init(|| Tool::find("git"))
            .as_ref()
            .context("--changed needs git on PATH")
    }

    /// `relic_core::tool::Tool`, deliberately **not** `relic_core::git::Git`:
    /// `Git` strips the ambient `GIT_*` environment, and here that environment
    /// is the answer. `GIT_DIR` and `GIT_WORK_TREE` must work by themselves —
    /// ernest never sets them and never sniffs for yadm, because that would be
    /// guessing at git's job and would make `--changed` mean something other
    /// than what `git diff` means in the same directory.
    ///
    /// What `Tool` supplies is the rest: the `C` locale the `not a git
    /// repository` test below depends on, a closed stdin, and typed failure.
    fn run(from: &Path, args: &[&str]) -> Result<String> {
        let tool = tool()?;
        let mut command = tool.command();
        command.arg("-C").arg(from).args(args);

        match tool.capture(&mut command) {
            Ok(output) => Ok(output.stdout),
            // The one failure worth explaining rather than relaying: this crate
            // lives in a yadm tree — work tree `$HOME`, git dir elsewhere, no
            // `.git` anywhere up the path — so the error is otherwise baffling in
            // exactly the repository ernest was written in.
            Err(relic_core::tool::Error::Failed { ref stderr, .. })
                if stderr.contains("not a git repository") =>
            {
                bail!(
                    "--changed needs a git repository; {} is not in one\n       \
                     a yadm-managed tree needs GIT_DIR and GIT_WORK_TREE set, or `yadm enter`",
                    from.display()
                );
            }
            Err(e) => Err(e).with_context(|| format!("git {}", args.join(" "))),
        }
    }
}
