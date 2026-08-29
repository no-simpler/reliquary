//! Discovery: which directories are relics, and in which lane.
//!
//! Two lanes exist because one is encrypted and the other is not, and the split
//! is what makes discovery **attic-safe**: a relic is surfaced only when its
//! manifest is *readable*, so an undecrypted private lane reveals nothing — not
//! a name, not a count.
//!
//! A manifest that is readable but broken is a different thing, and is reported
//! rather than skipped. Silence there is how a relic disappears.

use camino::{Utf8Path, Utf8PathBuf};

use crate::manifest::{self, Manifest};
use crate::paths::Paths;

/// Which lane a relic lives in.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Lane {
    /// `~/.config/relics` — tracked in plaintext.
    Public,
    /// `~/.config/attic` — swept whole into the encrypted archive.
    Private,
}

impl Lane {
    /// Both, in the order the tables read them.
    #[must_use]
    pub fn both() -> [Self; 2] {
        [Self::Public, Self::Private]
    }

    /// Where it is.
    #[must_use]
    pub fn root(self, paths: &Paths) -> Utf8PathBuf {
        match self {
            Self::Public => paths.public.clone(),
            Self::Private => paths.private.clone(),
        }
    }

    /// What the section heading calls it.
    #[must_use]
    pub fn heading(self) -> &'static str {
        match self {
            Self::Public => "In-house relics",
            Self::Private => "Private relics",
        }
    }
}

/// One discovered relic.
#[derive(Clone, Debug)]
pub struct Relic {
    /// Its directory name, which is also its manifest's `name` in practice.
    pub dir: Utf8PathBuf,
    /// Which lane it is in.
    pub lane: Lane,
    /// Its manifest, or why it could not be read.
    pub manifest: Result<Manifest, String>,
}

impl Relic {
    /// The directory's own name.
    #[must_use]
    pub fn slug(&self) -> &str {
        self.dir.file_name().unwrap_or_default()
    }

    /// The names it publishes, or none when its manifest will not parse.
    #[must_use]
    pub fn published_names(&self) -> Vec<String> {
        self.manifest
            .as_ref()
            .map(|m| m.published_names(&self.dir))
            .unwrap_or_default()
    }
}

/// Every relic in both lanes, public first, each lane in name order.
///
/// Ordered here rather than inherited from directory iteration, which no
/// platform promises anything about.
#[must_use]
pub fn all(paths: &Paths) -> Vec<Relic> {
    let mut out = Vec::new();
    for lane in Lane::both() {
        let mut found = in_lane(paths, lane);
        found.sort_by(|a, b| a.dir.cmp(&b.dir));
        out.extend(found);
    }
    out
}

/// Every relic in one lane, unordered.
#[must_use]
pub fn in_lane(paths: &Paths, lane: Lane) -> Vec<Relic> {
    let root = lane.root(paths);
    let Ok(entries) = fs_err::read_dir(root.as_std_path()) else {
        return Vec::new();
    };
    entries
        .filter_map(Result::ok)
        .filter_map(|entry| Utf8PathBuf::from_path_buf(entry.path()).ok())
        .filter(|dir| manifest::present(dir))
        .map(|dir| Relic {
            manifest: Manifest::load(&dir).map_err(|error| error.to_string()),
            dir,
            lane,
        })
        .collect()
}

/// Find one relic by name, in either lane.
#[must_use]
pub fn find(paths: &Paths, name: &str) -> Option<Relic> {
    Lane::both().into_iter().find_map(|lane| {
        let dir = lane.root(paths).join(name);
        manifest::present(&dir).then(|| Relic {
            manifest: Manifest::load(&dir).map_err(|error| error.to_string()),
            dir,
            lane,
        })
    })
}

/// The relic a directory is inside, if any.
///
/// Only the lane's *immediate* children are relics, so a path deep inside one
/// folds to the relic it belongs to and a path in the lane root belongs to no
/// relic at all.
#[must_use]
pub fn containing(paths: &Paths, cwd: &Utf8Path) -> Option<Relic> {
    for lane in Lane::both() {
        let root = lane.root(paths);
        let Ok(rest) = cwd.strip_prefix(&root) else {
            continue;
        };
        let Some(name) = rest.components().next() else {
            continue;
        };
        if let Some(relic) = find(paths, name.as_str()) {
            return Some(relic);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::{Lane, all, containing, find};
    use crate::paths::Paths;
    use camino::Utf8PathBuf;

    fn scratch() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let home = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf8 scratch path");
        let paths = Paths::under(home);
        for lane in Lane::both() {
            fs_err::create_dir_all(lane.root(&paths).as_std_path()).expect("a lane");
        }
        (dir, paths)
    }

    fn relic(paths: &Paths, lane: Lane, name: &str, body: &str) {
        let dir = lane.root(paths).join(name);
        fs_err::create_dir_all(dir.as_std_path()).expect("a relic dir");
        fs_err::write(dir.join("relic.toml").as_std_path(), body).expect("a manifest");
    }

    fn ok(name: &str) -> String {
        format!("[relic]\nname = \"{name}\"\nruntime = \"rust\"\n")
    }

    #[test]
    fn the_public_lane_comes_first_and_each_lane_is_in_name_order() {
        let (_guard, paths) = scratch();
        relic(&paths, Lane::Public, "zeta", &ok("zeta"));
        relic(&paths, Lane::Public, "alpha", &ok("alpha"));
        relic(&paths, Lane::Private, "beta", &ok("beta"));
        let found = all(&paths);
        let slugs: Vec<&str> = found.iter().map(super::Relic::slug).collect();
        assert_eq!(slugs, ["alpha", "zeta", "beta"]);
    }

    #[test]
    fn a_directory_with_no_manifest_is_not_a_relic() {
        let (_guard, paths) = scratch();
        fs_err::create_dir_all(paths.public.join("not-a-relic").as_std_path()).expect("a dir");
        assert!(all(&paths).is_empty());
    }

    #[test]
    fn a_broken_manifest_is_reported_rather_than_skipped() {
        let (_guard, paths) = scratch();
        relic(&paths, Lane::Public, "broken", "[relic\n");
        let found = all(&paths);
        assert_eq!(found.len(), 1);
        assert!(found.first().is_some_and(|r| r.manifest.is_err()));
        // And it publishes nothing, rather than guessing a name.
        assert!(
            found
                .first()
                .is_some_and(|r| r.published_names().is_empty())
        );
    }

    #[test]
    fn an_absent_lane_is_no_relics_rather_than_an_error() {
        let (_guard, paths) = scratch();
        fs_err::remove_dir_all(paths.private.as_std_path()).expect("lane gone");
        relic(&paths, Lane::Public, "alpha", &ok("alpha"));
        assert_eq!(all(&paths).len(), 1);
    }

    #[test]
    fn a_relic_is_found_in_either_lane_and_nowhere_else() {
        let (_guard, paths) = scratch();
        relic(&paths, Lane::Private, "hidden", &ok("hidden"));
        assert!(find(&paths, "hidden").is_some());
        assert!(find(&paths, "nothing").is_none());
    }

    #[test]
    fn a_path_inside_a_relic_folds_to_it_and_the_lane_root_does_not() {
        let (_guard, paths) = scratch();
        relic(&paths, Lane::Public, "alpha", &ok("alpha"));
        let deep = paths.public.join("alpha/src/inner");
        assert_eq!(
            containing(&paths, &deep).as_ref().map(super::Relic::slug),
            Some("alpha")
        );
        assert_eq!(
            containing(&paths, &paths.public.join("alpha"))
                .as_ref()
                .map(super::Relic::slug),
            Some("alpha")
        );
        assert!(containing(&paths, &paths.public).is_none());
        assert!(containing(&paths, &paths.home).is_none());
    }
}
