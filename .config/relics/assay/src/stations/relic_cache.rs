//! How much disk the relic lanes' build trees are holding.
//!
//! Cargo never removes an artifact. Every dependency bump leaves the version it
//! replaced behind, and `target/` only grows — so a machine that has not run
//! `up` in a while, or that has `up` but not `cargo-sweep`, balloons quietly.
//! `up` sweeps it; this is what notices when nothing has.
//!
//! **Never a failure.** The whole tree is regenerable by a rebuild, so a large
//! one is a machine that is degraded and entirely reproducible — which is the
//! definition of a soft finding.
//!
//! **Both lanes, separately.** Each relic lane is its own cargo workspace with
//! its own `target/`, and a cache reported for one says nothing about the other.
//! The private lane is often absent, because it only exists once the archive has
//! been decrypted; that is a fact about the machine, not a finding.

use anyhow::Result;
use camino::Utf8Path;
use relic_core::finding::{FixHint, Outcome, StationId, Summary};
use relic_core::tool::Tool;

use crate::station::{Context, Station};

/// The lanes that carry a cargo workspace, `$HOME`-relative.
const LANES: &[&str] = &[".config/relics", ".config/attic"];

/// The build directory inside a lane.
const TARGET: &str = "target";

/// Above this, a build tree is worth a word. 4 GiB, in kibibytes.
///
/// The lane measured 3.7 GB before `[profile.dev] debug = "line-tables-only"`
/// and `up`'s sweep step existed, almost all of it full DWARF under
/// `target/debug`. The ceiling sits just above where the problem was, so it
/// reports the condition that produced it rather than one nobody has met.
const CEILING_KIB: u64 = 4 * 1024 * 1024;

/// The station.
pub struct RelicCache {
    id: StationId,
}

impl Default for RelicCache {
    fn default() -> Self {
        Self {
            id: StationId::from_static("relic-cache"),
        }
    }
}

impl Station for RelicCache {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "the relic lanes' build trees have not run away"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let built: Vec<_> = LANES
            .iter()
            .map(|lane| (*lane, cx.at(lane).join(TARGET)))
            .filter(|(_, target)| target.is_dir())
            .collect();
        if built.is_empty() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "neither relic lane has been built yet",
            )));
        }
        let Some(du) = Tool::find("du") else {
            return Ok(Outcome::Skipped(Summary::lossy(
                "du is not on PATH, so nothing can measure a build tree",
            )));
        };

        let mut findings = Vec::new();
        for (lane, target) in built {
            let Some(kib) = measure(&du, &target) else {
                findings.push(self.id.note(Summary::lossy(&format!(
                    "~/{lane}/{TARGET} could not be measured"
                ))));
                continue;
            };
            if kib > CEILING_KIB {
                findings.push(
                    self.id
                        .soft(Summary::lossy(&format!(
                            "~/{lane}/{TARGET} is {} MiB",
                            kib / 1024
                        )))
                        .fixed_by(FixHint::lossy(&format!(
                            "run `up`, or `cargo sweep --time 30` in ~/{lane}"
                        ))),
                );
            }
        }
        Ok(Outcome::Ran(findings))
    }
}

/// A directory's size in kibibytes, or nothing when `du` would not say.
///
/// `du -sk` is not human-facing output: POSIX fixes the format as a number, a
/// tab and the path, and `-k` fixes the unit so the answer does not depend on a
/// `BLOCKSIZE` the environment happens to carry. Blocks rather than apparent
/// size, because the question is what the disk is holding.
fn measure(du: &Tool, target: &Utf8Path) -> Option<u64> {
    let mut command = du.command();
    command.arg("-sk").arg(target);
    let answer = du.capture(&mut command).ok()?;
    answer
        .stdout
        .lines()
        .next()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use relic_core::finding::{Finding, Severity};

    use super::*;

    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
    }

    impl Machine {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let home =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            Self { _dir: dir, home }
        }

        fn built(&self, lane: &str, bytes: usize) -> &Self {
            let target = self.home.join(lane).join(TARGET);
            fs_err::create_dir_all(&target).expect("a build tree");
            fs_err::write(target.join("artifact"), vec![b'x'; bytes]).expect("written");
            self
        }

        fn outcome(&self) -> Outcome {
            RelicCache::default()
                .check(&Context::new(self.home.clone(), Context::ambient_path()))
                .expect("the station ran")
        }

        fn findings(&self) -> Vec<Finding> {
            match self.outcome() {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("unexpectedly skipped: {reason}"),
            }
        }
    }

    #[test]
    fn a_machine_that_has_never_built_is_skipped_not_passed() {
        let machine = Machine::new();
        let Outcome::Skipped(reason) = machine.outcome() else {
            panic!("nothing has been built, so there is nothing to measure");
        };
        assert!(reason.as_str().contains("neither relic lane"));
    }

    #[test]
    fn a_small_build_tree_has_nothing_to_say() {
        let machine = Machine::new();
        machine.built(".config/relics", 1024);
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn an_absent_private_lane_is_a_fact_and_not_a_finding() {
        let machine = Machine::new();
        machine.built(".config/relics", 1024);
        assert!(
            !machine.home.join(".config/attic").exists(),
            "the private lane only exists once the archive is decrypted"
        );
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn the_ceiling_is_where_the_problem_was_measured() {
        // 4 GiB, expressed the way the retired check expressed it, so the two
        // cannot drift apart silently.
        assert_eq!(CEILING_KIB, 4_194_304);
    }

    #[test]
    fn a_tree_over_the_ceiling_is_soft_and_never_broken() {
        // The size is what `du` reports, so this is asserted through the same
        // reading rather than by writing four gibibytes to a scratch disk.
        let du = Tool::find("du").expect("du is POSIX");
        let machine = Machine::new();
        machine.built(".config/relics", 4096);
        let measured =
            measure(&du, &machine.home.join(".config/relics").join(TARGET)).expect("du answered");
        assert!(
            measured > 0,
            "a directory holding a file is not zero blocks"
        );
        assert!(
            measured < CEILING_KIB,
            "a 4 KiB artifact is not a runaway build tree"
        );

        // And the grading, on a finding built the way the station builds one.
        let id = StationId::from_static("relic-cache");
        assert_eq!(id.soft(Summary::lossy("x")).severity, Severity::Soft);
    }

    #[test]
    fn a_directory_that_does_not_exist_cannot_be_measured() {
        let du = Tool::find("du").expect("du is POSIX");
        assert_eq!(measure(&du, Utf8Path::new("/nowhere/at/all")), None);
    }

    #[test]
    fn both_lanes_are_measured_separately() {
        let machine = Machine::new();
        machine.built(".config/relics", 512);
        machine.built(".config/attic", 512);
        // Neither is over the ceiling, so the proof that both were visited is
        // that neither is reported and the station did not skip.
        let Outcome::Ran(findings) = machine.outcome() else {
            panic!("both lanes are built, so neither is a skip");
        };
        assert!(findings.is_empty());
    }
}
