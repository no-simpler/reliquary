//! Registry ↔ `PATH` lane ↔ entrypoints: four ways they drift.
//!
//! Read-only. Two of the questions are problems and two are information, and
//! the split is not cosmetic: an informational finding that graded would be a
//! gate that cannot be cleared where it fires, which is how a gate teaches
//! people to bypass it.

use camino::Utf8PathBuf;

use crate::lane::Relic;
use crate::paths::Paths;
use crate::publish;
use crate::registry::Registry;

/// Registry entries with no backing file in the `PATH` lane.
///
/// `registry --prune` fodder: something published a name and the file went
/// away.
#[must_use]
pub fn orphans(paths: &Paths, registry: &Registry) -> Vec<(String, Option<String>)> {
    registry
        .iter()
        .filter(|entry| !paths.local_bin.join(&entry.name).exists())
        .map(|entry| (entry.name.clone(), entry.owner.clone()))
        .collect()
}

/// Names a relic declares that are not in the registry.
///
/// The inverse of an orphan: declared, never published.
#[must_use]
pub fn unpublished(relics: &[Relic], registry: &Registry) -> Vec<(String, String)> {
    relics
        .iter()
        .flat_map(|relic| {
            let slug = relic.slug().to_owned();
            relic
                .published_names()
                .into_iter()
                .filter(|name| !registry.has(name))
                .map(move |name| (slug.clone(), name))
        })
        .collect()
}

/// Relics that are not Rust and do not say why.
///
/// **Informational, never a failure.** A relic awaiting its rewrite has to keep
/// publishing, and this list is the worklist for getting through them — it
/// empties as each one is either rewritten into the workspace or given its
/// reason.
#[must_use]
pub fn runtime_stance(relics: &[Relic]) -> Vec<(String, String)> {
    relics
        .iter()
        .filter_map(|relic| {
            let manifest = relic.manifest.as_ref().ok()?;
            if manifest.runtime.is_compiled() || !manifest.runtime_exemption.is_empty() {
                return None;
            }
            Some((relic.slug().to_owned(), manifest.runtime.to_string()))
        })
        .collect()
}

/// Executable files in the `PATH` lane that the registry does not know.
///
/// Foreign or sanctioned-sidestep binaries sharing the lane. Informational: the
/// lane is shared by design.
#[must_use]
pub fn unmanaged(paths: &Paths, registry: &Registry) -> Vec<String> {
    let Ok(entries) = fs_err::read_dir(paths.local_bin.as_std_path()) else {
        return Vec::new();
    };
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| Utf8PathBuf::from_path_buf(entry.path()).ok())
        .filter(|path| publish::executable(path))
        .filter_map(|path| path.file_name().map(ToOwned::to_owned))
        .filter(|name| !name.starts_with('.'))
        .filter(|name| !registry.has(name))
        .collect();
    names.sort();
    names
}

/// Everything the report holds.
#[derive(Debug, Default)]
pub struct Report {
    /// Registered, no file.
    pub orphans: Vec<(String, Option<String>)>,
    /// Declared, not registered.
    pub unpublished: Vec<(String, String)>,
    /// Not Rust, no reason given.
    pub stance: Vec<(String, String)>,
    /// In the lane, not in the registry.
    pub unmanaged: Vec<String>,
}

impl Report {
    /// Ask all four questions.
    #[must_use]
    pub fn gather(paths: &Paths, relics: &[Relic], registry: &Registry) -> Self {
        Self {
            orphans: orphans(paths, registry),
            unpublished: unpublished(relics, registry),
            stance: runtime_stance(relics),
            unmanaged: unmanaged(paths, registry),
        }
    }

    /// How many findings grade. The two informational lists do not.
    #[must_use]
    pub fn problems(&self) -> usize {
        self.orphans.len() + self.unpublished.len()
    }
}

#[cfg(test)]
mod tests {
    use super::{Report, runtime_stance, unpublished};
    use crate::lane::{Lane, Relic};
    use crate::manifest::Manifest;
    use crate::paths::Paths;
    use crate::registry::Registry;
    use camino::Utf8PathBuf;

    fn relic(name: &str, body: &str) -> Relic {
        let dir = Utf8PathBuf::from(format!("/lane/{name}"));
        Relic {
            manifest: toml::from_str::<toml::Value>(body)
                .ok()
                .and_then(|_| {
                    #[derive(serde::Deserialize)]
                    struct Doc {
                        relic: Manifest,
                    }
                    toml::from_str::<Doc>(body).ok().map(|d| d.relic)
                })
                .ok_or_else(|| "broken".to_owned()),
            dir,
            lane: Lane::Public,
        }
    }

    fn rust(name: &str) -> Relic {
        relic(
            name,
            &format!("[relic]\nname = \"{name}\"\nruntime = \"rust\"\n"),
        )
    }

    fn exempted(name: &str) -> Relic {
        relic(
            name,
            &format!(
                "[relic]\nname = \"{name}\"\nruntime = \"bash\"\n\
                 runtime-exemption = \"the bootstrap seed\"\n"
            ),
        )
    }

    fn bare(name: &str) -> Relic {
        relic(
            name,
            &format!("[relic]\nname = \"{name}\"\nruntime = \"bash\"\n"),
        )
    }

    #[test]
    fn the_stance_list_is_the_worklist_and_an_exemption_leaves_it() {
        let relics = [rust("a"), exempted("b"), bare("c")];
        let stance = runtime_stance(&relics);
        assert_eq!(stance, [("c".to_owned(), "bash".to_owned())]);
    }

    #[test]
    fn a_broken_manifest_is_not_a_stance_finding() {
        // It has no runtime to have a stance about, and it is already reported
        // by discovery. Saying it twice in different words is not two findings.
        let relics = [relic("broken", "[relic\n")];
        assert!(runtime_stance(&relics).is_empty());
    }

    #[test]
    fn a_declared_name_that_is_not_registered_is_unpublished() {
        let relics = [rust("a"), rust("b")];
        let registry = Registry::parse("a\ta\n");
        let gaps = unpublished(&relics, &registry);
        assert_eq!(gaps, [("b".to_owned(), "b".to_owned())]);
    }

    #[test]
    fn only_the_two_drift_questions_grade() {
        let paths = Paths::under(Utf8PathBuf::from("/nowhere"));
        let relics = [bare("c")];
        let registry = Registry::parse("");
        let report = Report::gather(&paths, &relics, &registry);
        // `c` publishes nothing (no entrypoints dir on a path that does not
        // exist), so the only finding is its stance — which is informational.
        assert_eq!(report.stance.len(), 1);
        assert_eq!(report.problems(), 0);
    }
}
