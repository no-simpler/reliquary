//! The shared `PATH` registry: what is managed, and by whom.
//!
//! One file, `~/.local/bin/.reliquary-managed`, listing every managed binary as
//! `<name>[<TAB><owner>]`. The owner column is optional per-entry provenance —
//! which meta-project published it — and is best-effort, never authoritative.
//!
//! **Read-only here.** Every write goes through `install-on-path.sh`, the
//! sourced shell ABI two external repositories also call. One implementation,
//! or the lane grows two opinions about who owns what.

use camino::Utf8Path;

/// One managed binary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// The name on `PATH`.
    pub name: String,
    /// Who published it, when the row says.
    pub owner: Option<String>,
}

/// The registry, in file order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Registry {
    entries: Vec<Entry>,
}

impl Registry {
    /// Read it, or an empty registry when there is none.
    ///
    /// An absent file is not an error: it is what a machine that has published
    /// nothing looks like.
    #[must_use]
    pub fn load(path: &Utf8Path) -> Self {
        fs_err::read_to_string(path.as_std_path())
            .map(|body| Self::parse(&body))
            .unwrap_or_default()
    }

    /// Parse its text. `#` comments and blank lines are not entries.
    #[must_use]
    pub fn parse(body: &str) -> Self {
        let entries = body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty() && !line.starts_with('#'))
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let name = fields.next()?.to_owned();
                Ok::<Entry, ()>(Entry {
                    name,
                    owner: fields.next().map(ToOwned::to_owned),
                })
                .ok()
            })
            .collect();
        Self { entries }
    }

    /// Whether a name is managed.
    #[must_use]
    pub fn has(&self, name: &str) -> bool {
        self.entries.iter().any(|entry| entry.name == name)
    }

    /// Who published a name, when the registry says.
    #[must_use]
    pub fn owner(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|entry| entry.name == name)
            .and_then(|entry| entry.owner.as_deref())
    }

    /// Every name this owner published.
    pub fn owned_by<'a>(&'a self, owner: &'a str) -> impl Iterator<Item = &'a str> {
        self.entries
            .iter()
            .filter(move |entry| entry.owner.as_deref() == Some(owner))
            .map(|entry| entry.name.as_str())
    }

    /// Whether this owner published anything at all.
    #[must_use]
    pub fn knows_owner(&self, owner: &str) -> bool {
        self.owned_by(owner).next().is_some()
    }

    /// Every entry, in file order.
    pub fn iter(&self) -> impl Iterator<Item = &Entry> {
        self.entries.iter()
    }

    /// Whether it holds nothing.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// How much of what a relic declares has actually reached `PATH`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum State {
    /// Every declared name is registered.
    Published,
    /// Some are.
    Partial,
    /// None are.
    Unpublished,
    /// It declares none, so there is nothing to publish.
    NoEntrypoints,
    /// Its manifest will not parse, so what it publishes is unknowable.
    ///
    /// Distinct from [`State::NoEntrypoints`] on purpose: "declares nothing"
    /// and "cannot be read" look identical in a table and are opposite facts,
    /// and the second is the one somebody has to fix.
    Broken,
    /// Listed as external and not on this machine.
    Absent,
    /// External and present: the registry can only answer by owner column,
    /// which is provenance rather than proof.
    Unknown,
}

impl State {
    /// The tally for a set of declared names.
    #[must_use]
    pub fn of(registry: &Registry, names: &[String]) -> Self {
        if names.is_empty() {
            return Self::NoEntrypoints;
        }
        let have = names.iter().filter(|name| registry.has(name)).count();
        if have == 0 {
            Self::Unpublished
        } else if have < names.len() {
            Self::Partial
        } else {
            Self::Published
        }
    }

    /// The word `status` prints.
    #[must_use]
    pub fn plain(self) -> &'static str {
        match self {
            Self::Published => "yes",
            Self::Partial => "partial",
            Self::Unpublished => "no",
            Self::NoEntrypoints => "n/a (no entrypoints)",
            Self::Broken => "unknown (manifest unreadable)",
            Self::Absent => "not present",
            Self::Unknown => "unknown",
        }
    }

    /// The glyph and word `list` prints.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Published => "● published",
            Self::Partial => "◐ partial",
            Self::Unpublished => "○ unpublished",
            Self::NoEntrypoints => "— no entrypoints",
            Self::Broken => "✗ unreadable manifest",
            Self::Absent => "○ listed, not present",
            Self::Unknown => "? unknown",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Registry, State};

    fn registry() -> Registry {
        Registry::parse(
            "# a comment\n\ndocket\tdocket\nmidden\tmidden\ndewey\thalo\nbare\n  spaced\tbb\n",
        )
    }

    #[test]
    fn comments_and_blanks_are_not_entries() {
        assert_eq!(registry().iter().count(), 5);
    }

    #[test]
    fn an_owner_column_is_optional() {
        let registry = registry();
        assert_eq!(registry.owner("docket"), Some("docket"));
        assert_eq!(registry.owner("bare"), None);
        assert!(registry.has("bare"));
        assert!(!registry.has("nothing"));
    }

    #[test]
    fn an_owner_lists_what_it_published() {
        let registry = registry();
        let names: Vec<&str> = registry.owned_by("halo").collect();
        assert_eq!(names, ["dewey"]);
        assert!(registry.knows_owner("bb"));
        assert!(!registry.knows_owner("nobody"));
    }

    #[test]
    fn an_absent_registry_is_empty_rather_than_an_error() {
        assert!(Registry::load(camino::Utf8Path::new("/nowhere/at/all")).is_empty());
    }

    #[test]
    fn publication_is_all_some_or_none() {
        let registry = registry();
        let names =
            |list: &[&str]| -> Vec<String> { list.iter().map(|s| (*s).to_owned()).collect() };
        assert_eq!(
            State::of(&registry, &names(&["docket", "midden"])),
            State::Published
        );
        assert_eq!(
            State::of(&registry, &names(&["docket", "nothing"])),
            State::Partial
        );
        assert_eq!(
            State::of(&registry, &names(&["nothing"])),
            State::Unpublished
        );
        assert_eq!(State::of(&registry, &[]), State::NoEntrypoints);
    }
}
