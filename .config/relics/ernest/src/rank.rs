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

use std::path::Path;

use anyhow::{Context, Result};
use ignore::overrides::{Override, OverrideBuilder};

/// A predicate over the files a run measured, and the words the caller used to
/// ask for it.
pub struct Selection {
    /// Canonical, in argv order, so two snapshots can be told apart by a reader
    /// and by `ernest diff`.
    pub asked: String,
    focus: Option<Override>,
}

impl Selection {
    /// `None` when nothing narrowed the ranking, so the caller can leave the
    /// whole thing alone rather than build a predicate that admits everything.
    pub fn build(focus: &[String]) -> Result<Option<Self>> {
        if focus.is_empty() {
            return Ok(None);
        }

        // Rooted at the working directory and matched against the path the walk
        // produced, which is both what the report prints and what the caller
        // typed the pathspec against.
        let cwd = std::env::current_dir().context("resolving the working directory")?;
        let mut builder = OverrideBuilder::new(&cwd);
        for pathspec in focus {
            builder
                .add(pathspec)
                .with_context(|| format!("--focus {pathspec}"))?;
        }
        let matcher = builder.build().context("building --focus")?;

        let asked = focus
            .iter()
            .map(|pathspec| format!("focus={pathspec}"))
            .collect::<Vec<_>>()
            .join(" ");

        Ok(Some(Selection {
            asked,
            focus: Some(matcher),
        }))
    }

    /// Every narrowing flag must admit the path: they intersect, because both
    /// answer "rank less than everything" and no other reading of the two
    /// together is unsurprising.
    pub fn admits(&self, path: &Path) -> bool {
        // A non-match comes back as `Match::Ignore`, per the crate's own
        // inversion of gitignore semantics — so the question is whitelisting,
        // not the absence of an ignore.
        self.focus
            .as_ref()
            .is_none_or(|matcher| matcher.matched(path, false).is_whitelist())
    }
}
