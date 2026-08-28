//! What would be removed, decided before anything is.
//!
//! A plan rather than a loop of side effects: the same computation answers
//! `--dry-run` and the real run, so what a dry run shows is what a real run
//! does, by construction rather than by two code paths agreeing.

use camino::{Utf8Path, Utf8PathBuf};

/// One thing to remove, and which lane condemned it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Doomed {
    /// The absolute path.
    pub path: Utf8PathBuf,
    /// Whether a repository said so, or only the name did.
    pub by_repository: bool,
}

/// Everything a run would remove, and what it could not ask about.
#[derive(Debug, Default)]
pub struct Plan {
    /// What to remove, in path order.
    pub doomed: Vec<Doomed>,
    /// Repositories that could not be asked, and why. Reported, never silent:
    /// a repository this could not read is one whose cruft survives, and a run
    /// that says nothing about it looks like a clean one.
    pub unanswered: Vec<(Utf8PathBuf, String)>,
    /// How many repositories were asked.
    pub repositories: usize,
}

impl Plan {
    /// The paths, deduplicated and ordered, so a directory and something under
    /// it are never both removed.
    #[must_use]
    pub fn collapsed(&self) -> Vec<&Doomed> {
        let mut ordered: Vec<&Doomed> = self.doomed.iter().collect();
        ordered.sort();
        ordered.dedup_by(|a, b| a.path == b.path);
        let mut kept: Vec<&Doomed> = Vec::with_capacity(ordered.len());
        for item in ordered {
            if kept
                .last()
                .is_some_and(|last| item.path.starts_with(&last.path))
            {
                continue;
            }
            kept.push(item);
        }
        kept
    }

    /// The directories a removal would leave empty.
    ///
    /// Reported, never deleted: git does not track an empty directory, so one
    /// left behind is invisible to every other tool — but removing it would
    /// silently delete a placeholder somebody meant to keep.
    #[must_use]
    pub fn emptied(&self, exists: &dyn Fn(&Utf8Path) -> bool) -> Vec<Utf8PathBuf> {
        let doomed: Vec<&Utf8Path> = self.collapsed().iter().map(|d| d.path.as_path()).collect();
        let mut parents: Vec<&Utf8Path> = doomed.iter().filter_map(|p| p.parent()).collect();
        parents.sort_unstable();
        parents.dedup();
        parents
            .into_iter()
            .filter(|parent| !exists(parent))
            .map(Utf8Path::to_owned)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doomed(paths: &[&str]) -> Plan {
        Plan {
            doomed: paths
                .iter()
                .map(|p| Doomed {
                    path: Utf8PathBuf::from(p),
                    by_repository: true,
                })
                .collect(),
            ..Plan::default()
        }
    }

    #[test]
    fn a_directory_swallows_what_is_under_it() {
        let plan = doomed(&["/a/__pycache__", "/a/__pycache__/x.pyc", "/b/.DS_Store"]);
        let paths: Vec<_> = plan.collapsed().iter().map(|d| d.path.as_str()).collect();
        assert_eq!(paths, ["/a/__pycache__", "/b/.DS_Store"]);
    }

    /// One path condemned by both lanes is still one removal.
    #[test]
    fn the_same_path_from_two_lanes_is_one_entry() {
        let mut plan = doomed(&["/a/.DS_Store"]);
        plan.doomed.push(Doomed {
            path: Utf8PathBuf::from("/a/.DS_Store"),
            by_repository: false,
        });
        assert_eq!(plan.collapsed().len(), 1);
    }

    #[test]
    fn a_parent_that_still_holds_something_is_not_emptied() {
        let plan = doomed(&["/a/.DS_Store", "/b/.DS_Store"]);
        let emptied = plan.emptied(&|p: &Utf8Path| p == "/a");
        assert_eq!(emptied, [Utf8PathBuf::from("/b")]);
    }
}
