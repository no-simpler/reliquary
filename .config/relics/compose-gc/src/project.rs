//! A Compose project name.
//!
//! Two constructors, because a name has two provenances and only one of them
//! may be rewritten. A name **observed** on a Docker label is a key the daemon
//! already agreed to: normalizing it would build a filter that matches nothing.
//! A name **derived** from a directory is this program's guess at what Compose
//! would have called it, so it is normalized the way Compose normalizes.

use camino::Utf8Path;

/// What Compose calls a stack.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct ProjectName(String);

impl ProjectName {
    /// A name read off a Docker label, taken verbatim.
    ///
    /// `None` for the empty string: a container with no project label is not a
    /// Compose container, and an empty filter value would match every project
    /// rather than none.
    #[must_use]
    pub fn observed(raw: &str) -> Option<Self> {
        if raw.is_empty() {
            return None;
        }
        Some(Self(raw.to_owned()))
    }

    /// The default name Compose derives from a project directory.
    ///
    /// Lowercase, drop everything outside `[a-z0-9_-]`, then trim leading `_`
    /// and `-`. Re-derive with the recipe in this relic's `CLAUDE.md`; read
    /// against Docker Compose v5.1.2.
    ///
    /// `None` when nothing survives, which is not a name Compose would use
    /// either.
    #[must_use]
    pub fn derived(dir: &Utf8Path) -> Option<Self> {
        let base = dir.file_name()?;
        let kept: String = base
            .to_lowercase()
            .chars()
            .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_' || *c == '-')
            .collect();
        let trimmed = kept.trim_start_matches(['_', '-']);
        if trimmed.is_empty() {
            return None;
        }
        Some(Self(trimmed.to_owned()))
    }

    /// The name, as Docker spells it.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// A volume or network name with this project's prefix removed.
    ///
    /// Compose names a project's resources `<project>_<declared>`, so stripping
    /// the prefix leaves the name as the compose file declared it — which is
    /// what makes two projects raised from the same file comparable.
    #[must_use]
    pub fn strip_prefix<'a>(&self, resource: &'a str) -> &'a str {
        resource
            .strip_prefix(&self.0)
            .and_then(|rest| rest.strip_prefix('_'))
            .unwrap_or(resource)
    }
}

impl std::fmt::Display for ProjectName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn derived(path: &str) -> Option<String> {
        ProjectName::derived(Utf8Path::new(path)).map(|name| name.as_str().to_owned())
    }

    #[test]
    fn compose_normalization_is_reproduced() {
        // Each pair was read back from `docker compose config --format json`
        // on Compose v5.1.2. See CLAUDE.md for the recipe.
        assert_eq!(derived("/x/MyRepo").as_deref(), Some("myrepo"));
        assert_eq!(
            derived("/x/repo.with.dots").as_deref(),
            Some("repowithdots")
        );
        assert_eq!(derived("/x/_leading").as_deref(), Some("leading"));
        assert_eq!(derived("/x/-lead").as_deref(), Some("lead"));
        assert_eq!(derived("/x/__--x").as_deref(), Some("x"));
        assert_eq!(derived("/x/UP-CASE_1").as_deref(), Some("up-case_1"));
        assert_eq!(derived("/x/9nine").as_deref(), Some("9nine"));
        assert_eq!(derived("/x/a b").as_deref(), Some("ab"));
        assert_eq!(derived("/x/Ünïcode").as_deref(), Some("ncode"));
    }

    #[test]
    fn a_directory_that_normalizes_to_nothing_names_no_project() {
        assert_eq!(derived("/x/---"), None);
        assert_eq!(derived("/x/。。"), None);
    }

    #[test]
    fn an_observed_name_is_never_rewritten() {
        // The daemon's spelling is the key every filter is built from. A name
        // Compose would not have derived is still the name it is stored under.
        let observed = ProjectName::observed("Not-Normalized").map(|n| n.as_str().to_owned());
        assert_eq!(observed.as_deref(), Some("Not-Normalized"));
        assert_eq!(ProjectName::observed(""), None);
    }

    #[test]
    fn the_project_prefix_comes_off_a_resource_name() {
        let project = ProjectName::observed("wt-a").unwrap();
        assert_eq!(project.strip_prefix("wt-a_data"), "data");
        // Not this project's resource: left exactly as it came.
        assert_eq!(project.strip_prefix("other_data"), "other_data");
        // The separator is part of the prefix — `wt-abandoned` is not `wt-a`'s.
        assert_eq!(project.strip_prefix("wt-abandoned"), "wt-abandoned");
    }
}
