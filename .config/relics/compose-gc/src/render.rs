//! What the sweep says, written to a sink so a test can read it.

use std::io::Write;

use anyhow::Result;
use relic_core::fmt::plural;

use crate::plan::{Plan, Reaping, Reason};

const RESET: &str = "\x1b[0m";
const BOLD: &str = "\x1b[1m";
const DIM: &str = "\x1b[2m";
const RED: &str = "\x1b[31m";
const GREEN: &str = "\x1b[32m";

/// How the run is written out.
#[derive(Clone, Copy, Debug)]
pub struct Style {
    /// Whether to spend on colour.
    pub color: bool,
    /// Whether this run removes anything.
    pub dry_run: bool,
}

/// One resource that would not go, and what the daemon said about it.
#[derive(Clone, Debug)]
pub struct Refusal {
    /// What it was, in the words the line will use — `volume wt-a_data`.
    pub what: String,
    /// The daemon's own message.
    pub why: String,
}

fn paint(text: &str, code: &str, color: bool) -> String {
    if color {
        format!("{code}{text}{RESET}")
    } else {
        text.to_owned()
    }
}

/// A line that reports something seen and not acted on.
///
/// # Errors
///
/// When the sink refuses the write.
pub fn note(out: &mut impl Write, text: &str, style: Style) -> Result<()> {
    writeln!(out, "      {}", paint(text, DIM, style.color))?;
    Ok(())
}

/// One project's outcome.
///
/// # Errors
///
/// When the sink refuses the write.
pub fn reaping(
    out: &mut impl Write,
    reaping: &Reaping,
    refusals: &[Refusal],
    style: Style,
) -> Result<()> {
    let mark = if refusals.is_empty() {
        paint("✓", GREEN, style.color)
    } else {
        paint("✗", RED, style.color)
    };
    let because = match reaping.reason {
        Reason::Abandoned => reaping.worktree.as_ref().map_or_else(
            || "worktree gone".to_owned(),
            |path| format!("worktree gone: {path}"),
        ),
        Reason::Stranded => "no containers left, main stack's volume shape".to_owned(),
    };
    writeln!(out, "  {mark} {} — {because}", reaping.project)?;
    writeln!(
        out,
        "      {}",
        paint(&tally(reaping, style.dry_run), DIM, style.color)
    )?;
    for refusal in refusals {
        writeln!(
            out,
            "      {}",
            paint(
                &format!("{} survived — {}", refusal.what, refusal.why),
                RED,
                style.color
            )
        )?;
    }
    Ok(())
}

fn tally(reaping: &Reaping, dry_run: bool) -> String {
    let verb = if dry_run { "would remove" } else { "removed" };
    format!(
        "{verb} {}, {}, {}",
        plural(reaping.containers.len(), "container", "containers"),
        plural(reaping.volumes.len(), "volume", "volumes"),
        plural(reaping.networks.len(), "network", "networks"),
    )
}

/// The closing line.
///
/// # Errors
///
/// When the sink refuses the write.
pub fn summary(out: &mut impl Write, plan: &Plan, repo: &str, style: Style) -> Result<()> {
    let verb = if style.dry_run {
        "would sweep"
    } else {
        "swept"
    };
    let text = format!(
        "compose-gc: {verb} {} from {repo}",
        plural(plan.reapings.len(), "orphaned project", "orphaned projects"),
    );
    writeln!(
        out,
        "{} {}",
        paint("==>", BOLD, style.color),
        paint(&text, BOLD, style.color)
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectName;
    use camino::Utf8PathBuf;

    fn sample() -> Reaping {
        Reaping {
            project: ProjectName::observed("wt-a").unwrap(),
            reason: Reason::Abandoned,
            worktree: Some(Utf8PathBuf::from("/dev/gmrepo/.claude/worktrees/wt-a")),
            containers: vec!["a1".to_owned(), "a2".to_owned()],
            volumes: vec!["wt-a_data".to_owned()],
            networks: Vec::new(),
        }
    }

    fn rendered(refusals: &[Refusal], style: Style) -> String {
        let mut out = Vec::new();
        reaping(&mut out, &sample(), refusals, style).unwrap();
        String::from_utf8(out).unwrap()
    }

    const PLAIN: Style = Style {
        color: false,
        dry_run: false,
    };

    #[test]
    fn a_clean_reaping_names_the_project_once() {
        let text = rendered(&[], PLAIN);
        assert_eq!(text.matches("wt-a —").count(), 1);
        assert!(
            text.contains("removed 2 containers, 1 volume, 0 networks"),
            "{text}"
        );
    }

    #[test]
    fn a_dry_run_says_would() {
        let text = rendered(
            &[],
            Style {
                dry_run: true,
                ..PLAIN
            },
        );
        assert!(text.contains("would remove 2 containers"), "{text}");
    }

    #[test]
    fn a_refusal_carries_the_daemons_own_words() {
        // The retired script sent every removal's stderr to /dev/null, so
        // "could not be removed" was the whole diagnosis.
        let text = rendered(
            &[Refusal {
                what: "volume wt-a_data".to_owned(),
                why: "volume is in use".to_owned(),
            }],
            PLAIN,
        );
        assert!(text.contains('✗'), "{text}");
        assert!(
            text.contains("volume wt-a_data survived — volume is in use"),
            "{text}"
        );
    }

    #[test]
    fn colour_is_a_choice_and_never_leaks_into_a_plain_render() {
        assert!(!rendered(&[], PLAIN).contains('\x1b'));
        assert!(
            rendered(
                &[],
                Style {
                    color: true,
                    ..PLAIN
                }
            )
            .contains('\x1b')
        );
    }
}
