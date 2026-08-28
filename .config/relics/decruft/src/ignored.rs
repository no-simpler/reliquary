//! What one repository says is ignored and untracked.
//!
//! The repository is the oracle, so a per-repository unignore is respected
//! without this program knowing the rule. A `!.DS_Store` re-include is not
//! ignored, so it is never listed, so it is never removed.
//!
//! The question is asked through `ls-files`, not `clean -Xdn`. `clean` reports
//! in prose — `Would remove <path>` — which a translated git does not say, and
//! the retired shell script parsed that literal: under any other locale it
//! matched nothing, removed nothing, and reported success. `ls-files -z` is the
//! machine-readable answer to the same question.
//!
//! git runs through [`relic_core::tool`] rather than [`relic_core::git`]: the
//! repository being asked about is named explicitly with `-C`, and stripping
//! the ambient environment would be answering a question nobody asked.

use camino::{Utf8Path, Utf8PathBuf};
use relic_core::tool::Tool;

/// Why a repository could not be asked.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// git is not on PATH, so no repository can answer.
    #[error("git is not on PATH, so no repository can say what it ignores")]
    NoGit,
    /// git ran and refused.
    #[error("{0}: git could not list ignored files: {1}")]
    Failed(Utf8PathBuf, String),
}

/// Every ignored, untracked path in `repo`, relative to it, collapsed so that
/// no entry sits beneath another.
///
/// # Errors
///
/// [`Error`].
pub fn paths(tool: &Tool, repo: &Utf8Path) -> Result<Vec<Utf8PathBuf>, Error> {
    let mut command = tool.command();
    command.arg("-C").arg(repo).args([
        "ls-files",
        "--others",
        "--ignored",
        "--exclude-standard",
        "--directory",
        "-z",
    ]);
    let output = match tool.capture(&mut command) {
        Ok(output) => output,
        Err(relic_core::tool::Error::Failed { ref stderr, .. }) => {
            let first = stderr
                .lines()
                .map(str::trim)
                .find(|line| !line.is_empty())
                .unwrap_or("no output");
            return Err(Error::Failed(repo.to_owned(), first.to_owned()));
        }
        Err(e) => return Err(Error::Failed(repo.to_owned(), e.to_string())),
    };
    Ok(collapse(parse(&output.stdout)))
}

fn parse(stdout: &str) -> Vec<Utf8PathBuf> {
    stdout
        .split('\0')
        .filter(|entry| !entry.is_empty())
        .map(|entry| Utf8PathBuf::from(entry.trim_end_matches('/')))
        .collect()
}

/// Drops every entry that sits beneath another.
///
/// `--directory` reports a wholly-ignored directory *and* keeps descending into
/// it, so the raw list double-counts: a removed directory's children are listed
/// after the directory that already took them. Collapsing here makes the result
/// independent of git's descent policy rather than dependent on a version of
/// it.
fn collapse(mut paths: Vec<Utf8PathBuf>) -> Vec<Utf8PathBuf> {
    paths.sort();
    let mut kept: Vec<Utf8PathBuf> = Vec::with_capacity(paths.len());
    for path in paths {
        // Sorted, so an ancestor is always the most recent kept entry that is
        // a prefix — and only that one has to be checked.
        if kept.last().is_some_and(|last| path.starts_with(last)) {
            continue;
        }
        kept.push(path);
    }
    kept
}

/// git, found once.
///
/// # Errors
///
/// [`Error::NoGit`].
pub fn tool() -> Result<Tool, Error> {
    Tool::find("git").ok_or(Error::NoGit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collapsed(entries: &[&str]) -> Vec<String> {
        collapse(entries.iter().map(Utf8PathBuf::from).collect())
            .iter()
            .map(ToString::to_string)
            .collect()
    }

    #[test]
    fn a_trailing_separator_is_not_part_of_the_name() {
        assert_eq!(parse("a/\0b\0"), ["a", "b"]);
    }

    #[test]
    fn an_empty_answer_is_no_paths() {
        assert!(parse("").is_empty());
        assert!(parse("\0").is_empty());
    }

    /// The defect collapsing exists for: git lists the directory it would
    /// remove and then lists what is inside it too.
    #[test]
    fn a_directory_swallows_what_is_under_it() {
        assert_eq!(
            collapsed(&[".idea", ".idea/icon.svg", ".idea/codeStyles", "other"]),
            [".idea", "other"]
        );
    }

    /// A prefix of a *name* is not a prefix of a *path*.
    #[test]
    fn a_shared_name_prefix_is_not_an_ancestor() {
        assert_eq!(collapsed(&["build", "buildings"]), ["build", "buildings"]);
    }

    #[test]
    fn siblings_all_survive() {
        assert_eq!(collapsed(&["b", "a", "c"]), ["a", "b", "c"]);
    }
}
