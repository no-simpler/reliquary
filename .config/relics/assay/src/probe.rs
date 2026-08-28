//! Resolving a program the way a shell would, over an injected search path.
//!
//! `type -aP` is what the shell checkers used, and it answers two questions at
//! once: what runs, and what else is there. The second is the shadow scan, so
//! resolution here returns every hit rather than the winner alone.

use camino::{Utf8Path, Utf8PathBuf};

/// Every executable named `name` on `path`, in search order, without repeats.
///
/// Repeats are dropped by spelling, as `type -aP | awk '!seen[$0]++'` did. Two
/// spellings of one binary are a separate question, and [`same_file`] answers
/// it — a symlink farm reached through two directories is not two installs.
#[must_use]
pub fn resolve_all(name: &str, path: &[Utf8PathBuf]) -> Vec<Utf8PathBuf> {
    let mut hits: Vec<Utf8PathBuf> = Vec::new();
    for dir in path {
        let candidate = dir.join(name);
        if is_executable(&candidate) && !hits.contains(&candidate) {
            hits.push(candidate);
        }
    }
    hits
}

/// What would run.
#[must_use]
pub fn resolve(name: &str, path: &[Utf8PathBuf]) -> Option<Utf8PathBuf> {
    resolve_all(name, path).into_iter().next()
}

/// Whether the path names something this process could execute.
#[must_use]
pub fn is_executable(path: &Utf8Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        std::fs::metadata(path)
            .is_ok_and(|meta| meta.is_file() && meta.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

/// Whether two spellings reach the same binary.
///
/// Symlinks are followed. `OrbStack` puts `docker` in `~/.orbstack/bin` and links
/// it into `/usr/local/bin`; that is one install seen twice, and reporting it
/// as
/// a conflict is the false positive this exists to avoid.
#[must_use]
pub fn same_file(left: &Utf8Path, right: &Utf8Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        // Unresolvable is not "the same": a broken symlink beside a real binary
        // is drift worth reporting, not a duplicate worth hiding.
        _ => left == right,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn executable(dir: &Utf8Path, name: &str) -> Utf8PathBuf {
        let path = dir.join(name);
        fs_err::write(&path, "#!/bin/sh\n").expect("written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs_err::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("made executable");
        }
        path
    }

    fn scratch() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
        (dir, root)
    }

    #[test]
    fn the_first_directory_on_the_path_wins() {
        let (_keep, root) = scratch();
        let first = root.join("first");
        let second = root.join("second");
        fs_err::create_dir_all(&first).expect("created");
        fs_err::create_dir_all(&second).expect("created");
        let winner = executable(&first, "just");
        let loser = executable(&second, "just");

        let path = vec![first, second];
        assert_eq!(resolve("just", &path).as_ref(), Some(&winner));
        assert_eq!(resolve_all("just", &path), vec![winner, loser]);
    }

    #[test]
    fn a_file_without_the_bit_is_not_a_hit() {
        let (_keep, root) = scratch();
        fs_err::write(root.join("just"), "not executable").expect("written");
        assert_eq!(resolve("just", &[root]), None);
    }

    #[test]
    fn one_directory_named_twice_yields_one_hit() {
        let (_keep, root) = scratch();
        let found = executable(&root, "curl");
        assert_eq!(resolve_all("curl", &[root.clone(), root]), vec![found]);
    }

    #[test]
    fn a_symlink_farm_is_one_install_seen_twice() {
        let (_keep, root) = scratch();
        let real = root.join("real");
        let linked = root.join("linked");
        fs_err::create_dir_all(&real).expect("created");
        fs_err::create_dir_all(&linked).expect("created");
        let target = executable(&real, "docker");
        let link = linked.join("docker");
        std::os::unix::fs::symlink(&target, &link).expect("linked");

        assert!(same_file(&target, &link));
        assert!(!same_file(&target, &executable(&linked, "podman")));
    }
}
