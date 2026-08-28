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
    /// There is no repository here to have a staged set.
    #[error("this is not a git repository, so nothing is staged")]
    NoRepository,
    /// git ran and refused. Its first line only: this runs in front of a
    /// Touch ID prompt, and git answers a bad invocation with a usage screen.
    #[error("git could not report the staged set: {0}")]
    Failed(String),
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
    let output = match tool.capture(&mut command) {
        Ok(output) => output,
        Err(relic_core::tool::Error::Failed { ref stderr, .. }) => {
            // Classified by asking git a question with a machine-readable
            // answer, not by reading the complaint it just printed. Outside a
            // repository `--cached` is not even a valid flag, so the message
            // is about the flag and says nothing about the real cause. Paid
            // only on the failure path, which is why it costs nothing.
            if !inside_work_tree(&tool) {
                return Err(Error::NoRepository);
            }
            return Err(Error::Failed(first_line(stderr)));
        }
        Err(e) => return Err(Error::Failed(e.to_string())),
    };
    parse(&output.stdout)
}

fn inside_work_tree(tool: &Tool) -> bool {
    let mut command = tool.command();
    command.args(["rev-parse", "--is-inside-work-tree"]);
    tool.capture(&mut command).is_ok()
}

/// git's complaint, without the usage screen it attaches to one.
fn first_line(stderr: &str) -> String {
    stderr
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .unwrap_or("no output")
        .to_owned()
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
    fn a_complaint_loses_the_usage_screen_attached_to_it() {
        let stderr = "error: unknown option `cached'\nusage: git diff ...\n    -p, --patch\n";
        assert_eq!(first_line(stderr), "error: unknown option `cached'");
        assert_eq!(first_line("\n\n"), "no output");
    }

    #[test]
    fn an_empty_list_is_no_paths_rather_than_one_empty_path() {
        assert!(parse("").expect("parses").is_empty());
        assert!(parse("\0").expect("parses").is_empty());
    }
}
