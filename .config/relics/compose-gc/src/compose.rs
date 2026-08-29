//! Whether a directory designates a compose file at all.
//!
//! The distinction the retired script drew by matching Docker's English — *no
//! configuration file* against every other refusal — is drawn here from the
//! filesystem instead. It decides one thing only: whether a failed teardown is
//! a directory that never had a stack (nothing to do) or a compose file that
//! would not load (a failure worth reporting).

use camino::Utf8Path;

/// The file names Compose looks for in a project directory, in its own order.
///
/// Transcribed, so it carries the obligation `~/.config/reliquary/HARDENING.md`
/// puts on a transcribed table. Re-derive with the recipe in this relic's
/// `CLAUDE.md`; read against Docker Compose v5.1.2.
pub const CANDIDATES: [&str; 4] = [
    "compose.yaml",
    "compose.yml",
    "docker-compose.yaml",
    "docker-compose.yml",
];

/// The variable that names compose files outright, wherever they are.
pub const COMPOSE_FILE: &str = "COMPOSE_FILE";

/// Whether a stack is designated for `dir`.
///
/// `designated` is the caller's reading of [`COMPOSE_FILE`], passed in rather
/// than read: an environment a test cannot set without racing every other test
/// is not an input a decision may reach for.
#[must_use]
pub fn designates_a_stack(dir: &Utf8Path, designated: bool) -> bool {
    designated
        || CANDIDATES
            .iter()
            .any(|name| dir.join(name).as_std_path().is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_candidate_is_found() {
        // The four names were verified one at a time against Compose v5.1.2:
        // a directory holding only that file answers `config -q` with zero.
        for name in CANDIDATES {
            let dir = tempfile::tempdir().unwrap();
            let dir = Utf8Path::from_path(dir.path()).unwrap();
            std::fs::write(dir.join(name), "services:\n  x:\n    image: alpine\n").unwrap();
            assert!(designates_a_stack(dir, false), "{name}");
        }
    }

    #[test]
    fn an_empty_directory_designates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        assert!(!designates_a_stack(dir, false));
        // …unless the caller was told where the files are.
        assert!(designates_a_stack(dir, true));
    }

    #[test]
    fn a_directory_named_like_a_compose_file_is_not_one() {
        let dir = tempfile::tempdir().unwrap();
        let dir = Utf8Path::from_path(dir.path()).unwrap();
        std::fs::create_dir(dir.join("compose.yaml")).unwrap();
        assert!(!designates_a_stack(dir, false));
    }
}
