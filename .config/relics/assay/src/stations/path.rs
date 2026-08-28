//! Whether the search path itself is sound.
//!
//! Not "does a shell start clean" — that is `shell-startup`, which asks three
//! shells what they end up with. This asks a different question about the answer:
//! is a search path of this *shape* safe to resolve a program through, and are
//! the two lanes this machine publishes into reachable at all?
//!
//! The two do not overlap. `shell-startup` owns duplicate entries, because it can
//! see all three shells' paths and this station only ever sees one. This owns the
//! shape of the one it is given — which is the path the calling process actually
//! got, and a nested non-interactive tool shell has been observed to inherit a
//! different order from the interactive one that configured it.
//!
//! **The lanes are graded hard.** `~/.config/bin` shadows Homebrew's `yadm` and
//! `gh` on purpose: the wrapper is how `yadm encrypt` records the archive's hash,
//! and the `gh` shim is how the benefactor identity stays separate. Lose the
//! ordering and both silently become the Homebrew originals — a guard disarmed,
//! which is what `Broken` means. `~/.local/bin` is where every relic publishes;
//! off the path, nothing published this year is reachable.
//!
//! **`bin/pb` is retired here, not ported.** Its inventory half — every personal
//! bin executable, coloured by whether yadm manages it — is `yadm-coverage`'s
//! question for `~/.config/bin` and the registry adapter's for `~/.local/bin`,
//! both of which already answer it. What is left of `pb` is its three warnings,
//! and they are checks: a lane that does not exist, a lane off `$PATH`, and a
//! file in one that is not executable.

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};

use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};

use crate::station::{Context, Station};

/// The lanes this machine publishes into, `$HOME`-relative, in the order they
/// must appear relative to Homebrew.
const LANES: &[&str] = &[".config/bin", ".local/bin"];

/// The lane that must win over Homebrew, and the program whose location says
/// where Homebrew's own directory is.
const SHADOWING_LANE: &str = ".config/bin";

/// Resolved to find Homebrew's bin directory rather than hardcoding a prefix:
/// `/opt/homebrew/bin` on Apple silicon, `/usr/local/bin` on Intel, and neither
/// on a machine that has no Homebrew.
const BREW: &str = "brew";

/// The station.
pub struct SearchPath {
    id: StationId,
}

impl Default for SearchPath {
    fn default() -> Self {
        Self {
            id: StationId::from_static("path"),
        }
    }
}

impl Station for SearchPath {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "the search path is safe to resolve through and reaches both publish lanes"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        if cx.path().is_empty() {
            return Ok(Outcome::Ran(vec![
                self.id
                    .broken(Summary::lossy("the search path is empty"))
                    .fixed_by(FixHint::lossy(
                        "something stripped $PATH before assay ran; check the caller",
                    )),
            ]));
        }

        let mut findings = Vec::new();
        findings.extend(self.shape(cx));
        findings.extend(self.lanes(cx));
        findings.extend(self.ordering(cx));
        Ok(Outcome::Ran(findings))
    }
}

impl SearchPath {
    /// Entries that are unsafe or dead, in the order the path lists them.
    fn shape(&self, cx: &Context) -> Vec<Finding> {
        let mut findings = Vec::new();
        let mut dead: Vec<Utf8PathBuf> = Vec::new();
        let lanes: Vec<Utf8PathBuf> = LANES.iter().map(|lane| real(&cx.at(lane))).collect();
        for entry in cx.path() {
            if entry.as_str().is_empty() {
                findings.push(
                    self.id
                        .broken(Summary::lossy(
                            "the search path holds an empty entry, which means the working directory",
                        ))
                        .detailed_with(Detail::new(
                            "A leading, trailing or doubled `:` resolves programs out of whatever \
                             directory you happen to be in.",
                        ))
                        .fixed_by(FixHint::lossy("remove the stray `:` from $PATH")),
                );
                continue;
            }
            if entry.is_relative() {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "{entry} is a relative search-path entry, so it means a different \
                             directory from every directory"
                        )))
                        .fixed_by(FixHint::lossy("spell it absolutely, or remove it")),
                );
                continue;
            }
            if !entry.is_dir() {
                // A lane is not reported here even when it is missing. `lanes`
                // owns that fact and says something useful about it; two
                // findings for one absence is how a report gets read twice and
                // acted on once.
                if !lanes.iter().any(|lane| lane == &real(entry)) {
                    dead.push(entry.clone());
                }
                continue;
            }
            if world_writable(entry) {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "{entry} is on the search path and is writable by anyone"
                        )))
                        .at(Location::file(entry.clone()))
                        .fixed_by(FixHint::lossy(&format!("chmod go-w {entry}, or drop it"))),
                );
            }
        }
        findings.extend(self.dead(&dead));
        findings
    }

    /// Search-path entries that are not directories, collapsed into one finding.
    ///
    /// **A `Note`, and one line.** Graded, it would redden every run of a normal
    /// macOS: `path_helper` contributes three `cryptexd` bootstrap directories
    /// that exist only sometimes, and a plugin harness contributes version-stamped
    /// directories that come and go. None of them is this machine's to fix, and a
    /// verdict nobody can clear where it fires is how a gate gets switched off.
    ///
    /// It is still worth saying, because one of them usually *is* ours — a stale
    /// `fish_user_paths` entry naming a formula that was renamed, say — and a
    /// dead entry costs a lookup on every resolution. So: one line with the
    /// count, and the names one level down, the way `shell-startup` reports
    /// duplicates.
    fn dead(&self, dead: &[Utf8PathBuf]) -> Vec<Finding> {
        if dead.is_empty() {
            return Vec::new();
        }
        let names: Vec<&str> = dead.iter().map(|dir| dir.as_str()).collect();
        vec![
            self.id
                .note(Summary::lossy(&format!(
                    "{} search-path entries are not directories",
                    dead.len()
                )))
                .detailed_with(Detail::new(names.join("\n")))
                .fixed_by(FixHint::lossy(
                    "drop the ones this machine owns; a dead entry costs a lookup and hides a typo",
                )),
        ]
    }

    /// Both publish lanes exist, are reachable, and hold only programs.
    fn lanes(&self, cx: &Context) -> Vec<Finding> {
        let mut findings = Vec::new();
        for lane in LANES {
            let at = cx.at(lane);
            if !at.is_dir() {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!("~/{lane} does not exist")))
                        .detailed_with(Detail::new(
                            "It is a publish target. Nothing installed into it can be, and \
                             nothing already there is reachable.",
                        ))
                        .fixed_by(FixHint::lossy(&format!("mkdir -p ~/{lane}"))),
                );
                continue;
            }
            if !on_path(cx, &at) {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "~/{lane} is not on the search path"
                        )))
                        .at(Location::file(at.clone()))
                        .fixed_by(FixHint::lossy(
                            "shell/env.d/040-env.sh adds both lanes; 999-path.sh orders them",
                        )),
                );
                continue;
            }
            findings.extend(self.inert(lane, &at));
        }
        findings
    }

    /// A plain file in a lane that cannot be run.
    ///
    /// `pb`'s one check worth keeping. A file placed here is meant to be a
    /// program, and a missing execute bit makes it silently not one — the
    /// command is simply not found, which reads as never installed.
    fn inert(&self, lane: &str, at: &Utf8Path) -> Vec<Finding> {
        let Ok(entries) = at.read_dir_utf8() else {
            return vec![
                self.id
                    .broken(Summary::lossy(&format!("~/{lane} could not be read")))
                    .at(Location::file(at.to_owned())),
            ];
        };
        let mut names: Vec<Utf8PathBuf> = entries
            .flatten()
            .map(|entry| entry.path().to_owned())
            .filter(|path| {
                // A leading dot is this machine's convention for data beside the
                // programs — `.reliquary-managed` is the registry itself. Nothing
                // resolves a dotfile as a command, so a missing execute bit on
                // one is not a broken command.
                !path.file_name().is_some_and(|name| name.starts_with('.'))
                    && path.is_file()
                    && !executable(path)
            })
            .collect();
        names.sort();
        names
            .into_iter()
            .map(|path| {
                let name = path.file_name().unwrap_or("?").to_owned();
                self.id
                    .soft(Summary::lossy(&format!(
                        "~/{lane}/{name} is not executable, so it is not a command"
                    )))
                    .at(Location::file(path))
                    .fixed_by(FixHint::lossy(&format!("chmod +x ~/{lane}/{name}")))
            })
            .collect()
    }

    /// The shadowing lane comes before Homebrew's own directory.
    ///
    /// Nothing when Homebrew is not on this path: that is a machine without
    /// Homebrew, or one whose bedrock is already the `bedrock` station's finding.
    fn ordering(&self, cx: &Context) -> Vec<Finding> {
        let lane = cx.at(SHADOWING_LANE);
        let Some(lane_at) = position(cx, &lane) else {
            return Vec::new();
        };
        let Some(brew_at) = cx.path().iter().position(|dir| dir.join(BREW).is_file()) else {
            return Vec::new();
        };
        if lane_at < brew_at {
            return Vec::new();
        }
        let brew_dir = cx
            .path()
            .get(brew_at)
            .map_or("?", |dir| dir.as_str())
            .to_owned();
        vec![
            self.id
                .broken(Summary::lossy(&format!(
                    "~/{SHADOWING_LANE} comes after {brew_dir} on the search path"
                )))
                .detailed_with(Detail::new(
                    "The lane exists to shadow Homebrew's `yadm` and `gh`. Behind it, bare \
                     `yadm` is Homebrew's — no wrapper subcommands, and `yadm encrypt` stops \
                     recording the archive's hash — and `gh` loses the benefactor profile.",
                ))
                .fixed_by(FixHint::lossy(
                    "shell/env.d/999-path.sh forces the order last, after every other prepend",
                )),
        ]
    }
}

/// Where a directory sits on the search path, comparing what the filesystem
/// resolves rather than how it is spelled.
fn position(cx: &Context, dir: &Utf8Path) -> Option<usize> {
    let wanted = real(dir);
    cx.path().iter().position(|entry| real(entry) == wanted)
}

/// Whether a directory is on the search path at all.
fn on_path(cx: &Context, dir: &Utf8Path) -> bool {
    position(cx, dir).is_some()
}

/// A path as the filesystem resolves it, or as spelled when it does not resolve.
fn real(path: &Utf8Path) -> Utf8PathBuf {
    path.canonicalize_utf8().unwrap_or_else(|_| path.to_owned())
}

/// Whether anyone at all may write into it.
#[cfg(unix)]
fn world_writable(path: &Utf8Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    fs_err::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o002 != 0)
}

/// Nothing to say where the permission bits do not exist.
#[cfg(not(unix))]
fn world_writable(_path: &Utf8Path) -> bool {
    false
}

/// Whether the owner may run it.
#[cfg(unix)]
fn executable(path: &Utf8Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    fs_err::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
}

/// Nothing to say where the permission bits do not exist.
#[cfg(not(unix))]
fn executable(_path: &Utf8Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use relic_core::finding::Severity;

    use super::*;

    /// A machine whose home, lanes and search path a test composes.
    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
        path: Vec<Utf8PathBuf>,
    }

    impl Machine {
        /// Both lanes present and on the path, in the right order. The shape
        /// every test starts from, so each one varies exactly one thing.
        fn sound() -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let home =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            let mut machine = Self {
                _dir: dir,
                home,
                path: Vec::new(),
            };
            for lane in LANES {
                let at = machine.home.join(lane);
                fs_err::create_dir_all(&at).expect("a lane");
                machine.path.push(at);
            }
            machine
        }

        fn dir(&mut self, name: &str) -> Utf8PathBuf {
            let at = self.home.join(name);
            fs_err::create_dir_all(&at).expect("a directory");
            at
        }

        /// Put a Homebrew-looking directory on the path.
        fn brew_at(&mut self, name: &str, position: usize) -> &mut Self {
            let at = self.dir(name);
            write(&at.join(BREW), 0o755);
            self.path.insert(position, at);
            self
        }

        fn entry(&mut self, entry: impl Into<Utf8PathBuf>) -> &mut Self {
            self.path.push(entry.into());
            self
        }

        fn findings(&self) -> Vec<Finding> {
            let outcome = SearchPath::default()
                .check(&Context::new(self.home.clone(), self.path.clone()))
                .expect("the station ran");
            match outcome {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("unexpectedly skipped: {reason}"),
            }
        }

        fn only(&self) -> Finding {
            let findings = self.findings();
            assert_eq!(findings.len(), 1, "{findings:?}");
            findings.into_iter().next().expect("one")
        }
    }

    /// A file with a mode.
    fn write(at: &Utf8Path, mode: u32) {
        fs_err::write(at, "#!/bin/sh\n").expect("a file");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs_err::set_permissions(at, std::fs::Permissions::from_mode(mode)).expect("a mode");
        }
    }

    #[test]
    fn a_sound_path_has_nothing_to_say() {
        assert!(Machine::sound().findings().is_empty());
    }

    #[test]
    fn a_stripped_path_is_broken_and_nothing_else_is_reported_over_it() {
        let mut machine = Machine::sound();
        machine.path.clear();
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("empty"), "{finding:?}");
    }

    #[test]
    fn an_empty_entry_means_the_working_directory_and_is_broken() {
        let mut machine = Machine::sound();
        machine.entry("");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("working directory"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_relative_entry_is_broken_for_the_same_reason() {
        let mut machine = Machine::sound();
        machine.entry("bin");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("relative"), "{finding:?}");
    }

    #[cfg(unix)]
    #[test]
    fn a_directory_anyone_may_write_into_is_broken() {
        use std::os::unix::fs::PermissionsExt as _;

        let mut machine = Machine::sound();
        let at = machine.dir("open");
        fs_err::set_permissions(&at, std::fs::Permissions::from_mode(0o777)).expect("a mode");
        machine.entry(at);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("writable by anyone"),
            "{finding:?}"
        );
    }

    #[test]
    fn dead_entries_are_one_note_naming_them_all_rather_than_a_verdict_each() {
        let mut machine = Machine::sound();
        machine.entry("/no/such/place").entry("/nor/this/one");
        let finding = machine.only();
        assert_eq!(
            finding.severity,
            Severity::Note,
            "most dead entries belong to the OS or a harness, not to this machine"
        );
        assert!(finding.summary.as_str().contains('2'), "{finding:?}");
        let detail = finding.detail.expect("the names, one level down");
        assert!(detail.as_str().contains("/no/such/place"), "{detail}");
        assert!(detail.as_str().contains("/nor/this/one"), "{detail}");
    }

    #[test]
    fn a_lane_that_does_not_exist_is_broken_because_nothing_can_publish_into_it() {
        let machine = Machine::sound();
        fs_err::remove_dir_all(machine.home.join(".local/bin")).expect("removed");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding
                .summary
                .as_str()
                .contains(".local/bin does not exist"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_lane_that_exists_and_is_off_the_path_is_broken() {
        let mut machine = Machine::sound();
        let lane = machine.home.join(".local/bin");
        machine.path.retain(|entry| entry != &lane);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("not on the search path"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_lane_reached_by_another_spelling_is_still_reached() {
        let mut machine = Machine::sound();
        let lane = machine.home.join(".local/bin");
        for entry in &mut machine.path {
            if entry == &lane {
                *entry = machine.home.join(".local/./bin");
            }
        }
        assert!(
            machine.findings().is_empty(),
            "membership is what the filesystem resolves, not how it is spelled"
        );
    }

    #[test]
    fn a_file_in_a_lane_that_cannot_be_run_is_not_a_command_and_says_so() {
        let machine = Machine::sound();
        write(&machine.home.join(".config/bin/inert"), 0o644);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Soft);
        assert!(
            finding.summary.as_str().contains("inert is not executable"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_dotfile_beside_the_programs_is_data_and_needs_no_execute_bit() {
        let machine = Machine::sound();
        write(&machine.home.join(".local/bin/.reliquary-managed"), 0o644);
        assert!(
            machine.findings().is_empty(),
            "the registry is not a command and nothing resolves it as one"
        );
    }

    #[test]
    fn the_shadowing_lane_behind_homebrew_is_broken_because_the_wrapper_stops_winning() {
        let mut machine = Machine::sound();
        machine.brew_at("brewbin", 0);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("comes after"),
            "{finding:?}"
        );
        assert!(
            finding
                .detail
                .as_ref()
                .is_some_and(|d| d.as_str().contains("yadm encrypt")),
            "the consequence is what makes it broken rather than untidy"
        );
    }

    #[test]
    fn the_shadowing_lane_ahead_of_homebrew_is_the_arrangement_and_says_nothing() {
        let mut machine = Machine::sound();
        machine.brew_at("brewbin", 2);
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn a_machine_without_homebrew_is_not_a_machine_with_homebrew_in_the_wrong_place() {
        let mut machine = Machine::sound();
        machine.entry(machine.home.join(".config/bin").clone());
        machine.path.remove(0);
        assert!(
            machine.findings().is_empty(),
            "no brew on the path is nothing to order against"
        );
    }
}
