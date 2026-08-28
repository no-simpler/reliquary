//! The set a commit is actually about.
//!
//! The retired hook read every tracked file, which is why it cost seconds: a
//! guard over what is *being committed* was doing a hundred times the work on
//! every commit. The whole-tree sweep is a standing audit, and belongs to one.
//!
//! git is invoked through [`relic_core::tool`] rather than [`relic_core::git`]:
//! that constructor strips the ambient `GIT_*` environment so a relic run from
//! a hook does not answer for the hook's repository. Here the hook's repository
//! is precisely the question, so the environment is inherited.

use camino::Utf8PathBuf;
use relic_core::tool::Tool;

/// Why the staged set could not be determined.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// git is not on PATH, so nothing can say what is staged.
    #[error("git is not on PATH, so the staged set cannot be read")]
    NoGit,
    /// git ran and refused.
    #[error("git could not report the staged set")]
    Failed(#[source] relic_core::tool::Error),
    /// git named a path this program cannot spell, which it will not guess at.
    #[error("git named a path that is not valid UTF-8")]
    NotUtf8,
}

/// The paths this commit adds, copies or modifies, relative to the work tree.
///
/// Deletions and renames-away are excluded: there is no content left to guard.
///
/// # Errors
///
/// [`Error`].
pub fn paths() -> Result<Vec<Utf8PathBuf>, Error> {
    let tool = Tool::find("git").ok_or(Error::NoGit)?;
    let mut command = tool.command();
    command.args([
        "diff",
        "--cached",
        "--name-only",
        "-z",
        "--diff-filter=ACMR",
    ]);
    let output = tool.capture(&mut command).map_err(Error::Failed)?;
    parse(&output.stdout)
}

/// git's NUL-delimited list. NUL-delimited because the retired hook split on
/// whitespace, so a tracked path with a space in it broke the guard into
/// fragments — in the hook whose job is preventing leaks.
fn parse(stdout: &str) -> Result<Vec<Utf8PathBuf>, Error> {
    Ok(stdout
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(Utf8PathBuf::from)
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_nul_delimited_list_keeps_paths_with_spaces_whole() {
        let parsed = parse("a b.txt\0c/d.md\0").expect("parses");
        assert_eq!(parsed, ["a b.txt", "c/d.md"]);
    }

    #[test]
    fn an_empty_list_is_no_paths_rather_than_one_empty_path() {
        assert!(parse("").expect("parses").is_empty());
        assert!(parse("\0").expect("parses").is_empty());
    }
}
