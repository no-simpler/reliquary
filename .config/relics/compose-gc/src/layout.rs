//! Which stacks are this repository's, and which of them still have a worktree.
//!
//! Two layouts, because this machine has used both: the nested
//! `<repo>/.claude/worktrees/<name>/` and the older sibling checkout
//! `<root>/<repo>-<name>/`. Anchoring on the nested shape alone leaves every
//! stack from the older era permanently unreachable.

use std::collections::BTreeSet;

use camino::{Utf8Path, Utf8PathBuf};

use crate::project::ProjectName;

/// Where this repository's worktrees may live.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Anchor {
    /// The main worktree.
    pub repo: Utf8PathBuf,
    /// Its parent — where sibling checkouts sit.
    pub root: Utf8PathBuf,
    /// The main worktree's directory name.
    pub name: String,
}

impl Anchor {
    /// Anchor on a main worktree.
    ///
    /// `None` for a path with no parent or no name — a filesystem root is not
    /// a repository this program can be scoped to.
    #[must_use]
    pub fn at(repo: &Utf8Path) -> Option<Self> {
        Some(Self {
            repo: repo.to_owned(),
            root: repo.parent()?.to_owned(),
            name: repo.file_name()?.to_owned(),
        })
    }

    /// Where nested worktrees live.
    #[must_use]
    pub fn nest(&self) -> Utf8PathBuf {
        self.repo.join(".claude/worktrees")
    }

    /// The worktree root `dir` belongs to, when it is one of this
    /// repository's.
    ///
    /// Returns the **worktree root**, not `dir` itself, so a compose file in a
    /// subdirectory still resolves to the tree whose removal orphaned it.
    /// The main worktree is deliberately not one of them: its stack is live
    /// state, never a leftover.
    #[must_use]
    pub fn worktree_of(&self, dir: &Utf8Path) -> Option<Utf8PathBuf> {
        let nest = self.nest();
        if let Ok(rest) = dir.strip_prefix(&nest) {
            let first = rest.components().next()?;
            return Some(nest.join(first.as_str()));
        }
        let rest = dir.strip_prefix(&self.root).ok()?;
        let first = rest.components().next()?.as_str();
        let sibling = first.strip_prefix(&self.name)?;
        if sibling.starts_with('-') {
            Some(self.root.join(first))
        } else {
            None
        }
    }
}

/// The worktrees that still exist, and the projects they account for.
///
/// Resolved once, ahead of planning, so that planning is a pure function of
/// what was observed rather than of what the filesystem does next.
#[derive(Clone, Debug, Default)]
pub struct Liveness {
    /// Worktree roots that are still there, or that git still registers.
    pub dirs: BTreeSet<Utf8PathBuf>,
    /// Project names accounted for by something live.
    pub projects: BTreeSet<ProjectName>,
}

impl Liveness {
    /// Whether a worktree root is spared.
    #[must_use]
    pub fn spares(&self, dir: &Utf8Path) -> bool {
        self.dirs.contains(dir)
    }

    /// Whether a project name is accounted for by something live.
    #[must_use]
    pub fn accounts_for(&self, project: &ProjectName) -> bool {
        self.projects.contains(project)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn anchor() -> Anchor {
        Anchor::at(Utf8Path::new("/dev/gmrepo")).unwrap()
    }

    fn worktree_of(dir: &str) -> Option<String> {
        anchor()
            .worktree_of(Utf8Path::new(dir))
            .map(|path| path.to_string())
    }

    #[test]
    fn a_nested_worktree_resolves_to_its_root() {
        assert_eq!(
            worktree_of("/dev/gmrepo/.claude/worktrees/wt-a").as_deref(),
            Some("/dev/gmrepo/.claude/worktrees/wt-a")
        );
    }

    #[test]
    fn a_compose_file_below_a_worktree_resolves_to_that_worktree() {
        // The retired script tested liveness on the compose file's own
        // directory, so a stack declared in a subdirectory of a removed
        // worktree was judged by a path nobody had removed.
        assert_eq!(
            worktree_of("/dev/gmrepo/.claude/worktrees/wt-a/docker/dev").as_deref(),
            Some("/dev/gmrepo/.claude/worktrees/wt-a")
        );
        assert_eq!(
            worktree_of("/dev/gmrepo-sib/docker").as_deref(),
            Some("/dev/gmrepo-sib")
        );
    }

    #[test]
    fn a_sibling_checkout_needs_the_separator() {
        assert_eq!(
            worktree_of("/dev/gmrepo-sib").as_deref(),
            Some("/dev/gmrepo-sib")
        );
        // `gmrepolegacy` merely starts with the repository's name.
        assert_eq!(worktree_of("/dev/gmrepolegacy"), None);
    }

    #[test]
    fn the_main_worktree_is_not_one_of_them() {
        assert_eq!(worktree_of("/dev/gmrepo"), None);
        assert_eq!(worktree_of("/dev/gmrepo/src"), None);
        assert_eq!(worktree_of("/dev/gmrepo/.claude/worktrees"), None);
    }

    #[test]
    fn a_foreign_directory_is_nobody_here() {
        assert_eq!(worktree_of("/dev/other/app"), None);
        assert_eq!(worktree_of("/elsewhere"), None);
    }
}
