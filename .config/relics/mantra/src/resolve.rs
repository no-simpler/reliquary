//! Where a `+token` finds its mode.
//!
//! Two bases — the home tree and the project's — and inside each, the tree
//! itself and then every skills-dir plugin's. A directory under
//! `~/.claude/skills/` is adopted by Claude Code as a local plugin, so that is
//! where a mode belonging to a plugin lives; the private lane at
//! `~/.claude/skills/attic/` is exactly this case, which is what keeps a private
//! mode covered by the one encrypt pattern that already sweeps it.
//!
//! Two rules are worth stating because they are decisions rather than
//! consequences:
//!
//! - **Marketplace plugins are not searched.** `~/.claude/plugins/` is
//!   third-party code, and a `+token` must not be able to pull directives out of
//!   it.
//! - **A project cannot shadow a home mode.** The home tree is searched first
//!   and the first hit wins, so cloning a repository cannot redefine what
//!   `+terse` means on this machine.
//!
//! Lookup is a single `join` per root rather than a recursive glob. The expander
//! this replaced globbed, which made the cost of resolving one token a function
//! of the size of the whole tree; nothing nests a mode in a subdirectory, and the
//! bound is worth more than the possibility.

use camino::{Utf8Path, Utf8PathBuf};

/// The directory name that holds modes, under every base and every plugin.
const MODES: &str = "modes";

/// Every place a mode may live, in the order they are consulted.
///
/// `project` is the session's own directory, when it differs from home.
pub fn roots(home: &Utf8Path, project: Option<&Utf8Path>) -> Vec<Utf8PathBuf> {
    let mut roots = Vec::new();
    let mut seen = Vec::new();
    let bases = [
        Some(home.join(".claude")),
        project.map(|p| p.join(".claude")),
    ];
    for base in bases.into_iter().flatten() {
        push_unique(&mut roots, &mut seen, base.join(MODES));
        for plugin in plugins(&base) {
            push_unique(&mut roots, &mut seen, plugin.join(MODES));
        }
    }
    roots
}

/// Adds a root unless its resolved path is already in the list. The session's
/// working directory is sometimes `$HOME` itself, and a root consulted twice
/// would report every mode in it twice.
fn push_unique(roots: &mut Vec<Utf8PathBuf>, seen: &mut Vec<Utf8PathBuf>, root: Utf8PathBuf) {
    let key = relic_core::path::resolve_lenient(&root).unwrap_or_else(|_| root.clone());
    if seen.contains(&key) {
        return;
    }
    seen.push(key);
    roots.push(root);
}

/// The skills-dir plugins under one base, by name, so the order two plugins
/// defining the same mode are consulted in does not depend on the filesystem.
fn plugins(base: &Utf8Path) -> Vec<Utf8PathBuf> {
    let Ok(entries) = fs_err::read_dir(base.join("skills")) else {
        return Vec::new();
    };
    let mut found: Vec<Utf8PathBuf> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir() || t.is_symlink()))
        .filter_map(|e| relic_core::path::utf8(e.path()).ok())
        .filter(|p| !p.file_name().is_some_and(|n| n.starts_with('.')))
        .collect();
    found.sort();
    found
}

/// The file one name resolves to, or nothing.
pub fn find(name: &str, roots: &[Utf8PathBuf]) -> Option<Utf8PathBuf> {
    roots
        .iter()
        .map(|root| root.join(format!("{name}.md")))
        .find(|path| path.is_file())
}

/// Every mode reachable from `roots`, by name, first hit winning — the same rule
/// a single lookup follows, so a listing never shows a file a `+token` would not
/// reach.
pub fn all(roots: &[Utf8PathBuf]) -> Vec<(String, Utf8PathBuf)> {
    let mut found: Vec<(String, Utf8PathBuf)> = Vec::new();
    for root in roots {
        let Ok(entries) = fs_err::read_dir(root) else {
            continue;
        };
        let mut here: Vec<(String, Utf8PathBuf)> = entries
            .flatten()
            .filter_map(|e| relic_core::path::utf8(e.path()).ok())
            .filter(|p| p.extension() == Some("md") && p.is_file())
            .filter_map(|p| p.file_stem().map(|stem| (stem.to_owned(), p.clone())))
            .collect();
        here.sort();
        for (name, path) in here {
            if !found.iter().any(|(seen, _)| *seen == name) {
                found.push((name, path));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs_err as fs;

    struct Tree(tempfile::TempDir);

    impl Tree {
        fn new() -> Self {
            Self(tempfile::tempdir().expect("a temporary directory"))
        }

        fn root(&self) -> Utf8PathBuf {
            // On macOS the temporary root is itself a symlink, and the dedup
            // compares resolved paths.
            let real = fs::canonicalize(self.0.path()).expect("canonical");
            relic_core::path::utf8(real).expect("utf-8")
        }

        fn write(&self, rel: &str, text: &str) -> Utf8PathBuf {
            let path = self.root().join(rel);
            fs::create_dir_all(path.parent().expect("a parent")).expect("mkdir");
            fs::write(&path, text).expect("write");
            path
        }
    }

    #[test]
    fn the_home_tree_comes_first_then_its_plugins() {
        let tree = Tree::new();
        let home = tree.root().join("home");
        fs::create_dir_all(home.join(".claude/skills/zeta")).expect("mkdir");
        fs::create_dir_all(home.join(".claude/skills/attic")).expect("mkdir");
        let found = roots(&home, None);
        assert_eq!(
            found,
            [
                home.join(".claude/modes"),
                home.join(".claude/skills/attic/modes"),
                home.join(".claude/skills/zeta/modes"),
            ]
        );
    }

    #[test]
    fn the_project_tree_comes_after_the_home_tree() {
        let tree = Tree::new();
        let home = tree.root().join("home");
        let project = tree.root().join("project");
        fs::create_dir_all(&home).expect("mkdir");
        fs::create_dir_all(&project).expect("mkdir");
        let found = roots(&home, Some(&project));
        assert_eq!(
            found,
            [home.join(".claude/modes"), project.join(".claude/modes")]
        );
    }

    #[test]
    fn a_project_that_is_home_is_not_searched_twice() {
        let tree = Tree::new();
        let home = tree.root().join("home");
        fs::create_dir_all(&home).expect("mkdir");
        assert_eq!(roots(&home, Some(&home)), [home.join(".claude/modes")]);
    }

    #[test]
    fn a_dot_directory_is_not_a_plugin() {
        let tree = Tree::new();
        let home = tree.root().join("home");
        fs::create_dir_all(home.join(".claude/skills/.cache")).expect("mkdir");
        assert_eq!(roots(&home, None), [home.join(".claude/modes")]);
    }

    #[test]
    fn a_project_cannot_shadow_a_home_mode() {
        let tree = Tree::new();
        let home = tree.root().join("home");
        let project = tree.root().join("project");
        let wanted = tree.write("home/.claude/modes/terse.md", "home\n");
        tree.write("project/.claude/modes/terse.md", "project\n");
        let roots = roots(&home, Some(&project));
        assert_eq!(find("terse", &roots), Some(wanted));
    }

    #[test]
    fn a_plugin_mode_resolves_by_its_bare_name() {
        let tree = Tree::new();
        let home = tree.root().join("home");
        let wanted = tree.write("home/.claude/skills/attic/modes/mr.md", "private\n");
        assert_eq!(find("mr", &roots(&home, None)), Some(wanted));
    }

    #[test]
    fn a_missing_name_resolves_to_nothing() {
        let tree = Tree::new();
        let home = tree.root().join("home");
        fs::create_dir_all(home.join(".claude/modes")).expect("mkdir");
        assert_eq!(find("absent", &roots(&home, None)), None);
    }

    #[test]
    fn a_nested_file_is_not_reachable() {
        // The expander globbed and would have found this. The bound is the
        // point: resolving a token must not cost a tree walk.
        let tree = Tree::new();
        let home = tree.root().join("home");
        tree.write("home/.claude/modes/nested/deep.md", "deep\n");
        assert_eq!(find("deep", &roots(&home, None)), None);
    }

    #[test]
    fn listing_follows_the_same_first_hit_rule() {
        let tree = Tree::new();
        let home = tree.root().join("home");
        let project = tree.root().join("project");
        let terse = tree.write("home/.claude/modes/terse.md", "home\n");
        let mr = tree.write("home/.claude/skills/attic/modes/mr.md", "private\n");
        tree.write("project/.claude/modes/terse.md", "shadowed\n");
        let only = tree.write("project/.claude/modes/local.md", "project\n");
        let found = all(&roots(&home, Some(&project)));
        assert_eq!(
            found,
            [
                ("terse".to_owned(), terse),
                ("mr".to_owned(), mr),
                ("local".to_owned(), only),
            ]
        );
    }
}
