//! Whether the two non-brew package lanes still match what the repo declares.
//!
//! `cargo/crates.txt` and `npm/globals.txt` are committed manifests restored at
//! bootstrap and refreshed by `up`. Neither the restore nor the refresh ever
//! *compares* them to the machine: bootstrap installs what is missing and runs
//! once, `up` upgrades what is already there. So drift is silent in both
//! directions, and only surfaces on the next machine — which is the worst place
//! to find it.
//!
//! **Both directions matter, and they are different failures.**
//!
//! - **Declared and not installed**: this machine does not have a tool the repo
//!   says it has. A manifest entry added while a machine is running is *not*
//!   installed by `up`; bootstrap is the only thing that installs from a
//!   manifest, and bootstrap runs once. Degraded and entirely reproducible —
//!   which is `Soft`.
//! - **Installed and not declared**: the machine has a tool the next machine
//!   will not. The same drift `brew-health` reports for request-installed
//!   packages, and the same grade.
//!
//! **Cargo is asked for its data, not for its output.** `cargo install --list`
//! is a human-facing listing with no `--json`; `~/.cargo/.crates.toml` is the
//! ledger that listing reads, structured and locale-free.
//!
//! It is the **v1** ledger deliberately. Cargo keeps two, and they can disagree:
//! measured 2026-08-29 on this machine, `.crates2.json` was missing an entry
//! `.crates.toml` carried, and `cargo install --list` agreed with the older
//! file. An oracle that under-reports what is installed manufactures
//! "declared and not installed" findings out of nothing, so the ledger to read
//! is the one the tooling itself believes.
//!
//! Both are cargo's internal formats, so the obligation that comes with reading
//! one applies: a ledger this station cannot parse is a **`Broken` finding
//! naming the file**, never a machine that looks clean. npm needs no such
//! judgement — `npm ls -g --json` is a published interface.

use std::collections::BTreeSet;

use anyhow::Result;
use serde::Deserialize;

use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};
use relic_core::tool::Tool;

use crate::station::{Context, Station};

/// One lane: a committed manifest, and what is actually installed.
struct Lane {
    /// What the lane is called in a finding.
    name: &'static str,
    /// The committed manifest, `$HOME`-relative.
    manifest: &'static str,
    /// What to do about a name the manifest declares and the machine lacks.
    install: &'static str,
}

/// The cargo lane.
const CARGO: Lane = Lane {
    name: "cargo",
    manifest: ".config/cargo/crates.txt",
    install: "cargo binstall <name>, or cargo install <name>",
};

/// The npm lane.
const NPM: Lane = Lane {
    name: "npm",
    manifest: ".config/npm/globals.txt",
    install: "npm install -g <name>",
};

/// Cargo's own install ledger, `$HOME`-relative. The v1 one — see the module
/// note on why, and on what the other one was measured doing.
const CRATES_LEDGER: &str = ".cargo/.crates.toml";

/// Names that are installed in a lane and never declared in it, because
/// declaring them is impossible or circular rather than an oversight.
///
/// Two, and both are properties of the tool rather than decisions about this
/// machine — which is why they are here and not in a file beside the manifest.
/// A judgement call about a *package* would belong in the manifest, the way
/// `brew/undeclared` carries brew's.
const STRUCTURAL: &[(&str, &str)] = &[
    // Bootstrap installs it in order to install everything else from the
    // manifest. A manifest that declared its own installer would be circular.
    ("cargo", "cargo-binstall"),
    // `npm ls -g` always lists npm. It is not a global package anyone chose.
    ("npm", "npm"),
];

/// How long npm has to enumerate its own global packages.
const BUDGET: std::time::Duration = std::time::Duration::from_secs(30);

/// The shape `~/.cargo/.crates.toml` carries.
///
/// Not `deny_unknown_fields`: it is cargo's schema, not ours, and cargo adds
/// fields. What is asserted instead is that the one table this station needs is
/// there — see the parse failure path.
#[derive(Debug, Deserialize)]
struct CratesLedger {
    /// Keyed by `"<name> <version> (<source>)"`, valued by the binaries the
    /// crate installed.
    v1: std::collections::BTreeMap<String, Vec<String>>,
}

/// The shape `npm ls -g --json` carries.
///
/// Only the keys are wanted, so the values stay untyped — the one place the
/// typed-domain standard allows it, at the boundary and one function wide.
#[derive(Debug, Deserialize)]
struct NpmGlobals {
    /// Keyed by package name.
    #[serde(default)]
    dependencies: std::collections::BTreeMap<String, serde_json::Value>,
}

/// The station.
pub struct ManifestDrift {
    id: StationId,
}

impl Default for ManifestDrift {
    fn default() -> Self {
        Self {
            id: StationId::from_static("manifest-drift"),
        }
    }
}

impl Station for ManifestDrift {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "the cargo and npm lanes hold what their manifests declare, and nothing else"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let mut findings = Vec::new();
        let mut ran = 0_usize;

        for (lane, installed) in [
            (&CARGO, self.cargo_installed(cx)),
            (&NPM, self.npm_installed(cx)),
        ] {
            match installed {
                Ok(Some(installed)) => {
                    ran += 1;
                    findings.extend(self.compare(cx, lane, &installed));
                }
                Ok(None) => {}
                Err(finding) => {
                    ran += 1;
                    findings.push(*finding);
                }
            }
        }

        if ran == 0 {
            return Ok(Outcome::Skipped(Summary::lossy(
                "neither cargo nor npm is on this machine, so neither lane can be read",
            )));
        }
        Ok(Outcome::Ran(findings))
    }
}

impl ManifestDrift {
    /// Both directions of drift for one lane.
    fn compare(&self, cx: &Context, lane: &Lane, installed: &BTreeSet<String>) -> Vec<Finding> {
        let manifest = cx.at(lane.manifest);
        let Ok(text) = fs_err::read_to_string(&manifest) else {
            return vec![
                self.id
                    .broken(Summary::lossy(&format!(
                        "the {} manifest could not be read, so nothing can be compared to it",
                        lane.name
                    )))
                    .at(Location::file(manifest)),
            ];
        };
        let declared = declared(&text);
        if declared.is_empty() {
            return vec![
                self.id
                    .broken(Summary::lossy(&format!(
                        "the {} manifest declares nothing",
                        lane.name
                    )))
                    .at(Location::file(manifest))
                    .detailed_with(Detail::new(
                        "A manifest that names nothing would report every installed package as \
                         undeclared, which is a different fact from an empty lane.",
                    )),
            ];
        }

        let exempt: BTreeSet<&str> = STRUCTURAL
            .iter()
            .filter(|(which, _)| *which == lane.name)
            .map(|(_, name)| *name)
            .collect();

        let mut findings = Vec::new();
        for name in declared.difference(installed) {
            findings.push(
                self.id
                    .soft(Summary::lossy(&format!(
                        "{name} is declared in the {} lane and is not installed",
                        lane.name
                    )))
                    .at(Location::file(manifest.clone()))
                    .detailed_with(Detail::new(
                        "Only bootstrap installs from a manifest, and bootstrap runs once. `up` \
                         upgrades what is already here, so an entry added since is absent until \
                         it is installed by hand.",
                    ))
                    .fixed_by(FixHint::lossy(lane.install)),
            );
        }
        for name in installed.difference(&declared) {
            if exempt.contains(name.as_str()) {
                continue;
            }
            findings.push(
                self.id
                    .soft(Summary::lossy(&format!(
                        "{name} is installed in the {} lane and declared nowhere",
                        lane.name
                    )))
                    .at(Location::file(manifest.clone()))
                    .detailed_with(Detail::new(
                        "It is on this machine and will not be on the next one. The drift only \
                         surfaces on a restore, because the tool is here until then.",
                    ))
                    .fixed_by(FixHint::lossy(&format!(
                        "declare it in ~/{}, with the caller that invokes it — or uninstall it",
                        lane.manifest
                    ))),
            );
        }
        findings
    }

    /// What cargo has installed, from cargo's own ledger.
    ///
    /// `Ok(None)` when cargo has never installed anything here — no ledger and
    /// no cargo is a machine without the lane, not a machine with an empty one.
    fn cargo_installed(&self, cx: &Context) -> Result<Option<BTreeSet<String>>, Box<Finding>> {
        let ledger = cx.at(CRATES_LEDGER);
        if !ledger.is_file() {
            return Ok(None);
        }
        let text = fs_err::read_to_string(&ledger).map_err(|error| {
            Box::new(
                self.id
                    .broken(Summary::lossy(&format!(
                        "cargo's install ledger could not be read: {error}"
                    )))
                    .at(Location::file(ledger.clone())),
            )
        })?;
        let parsed: CratesLedger =
            toml::from_str(&text).map_err(|error| {
                Box::new(self.id
                .broken(Summary::lossy(&format!(
                    "cargo's install ledger is not in a shape this station knows: {error}"
                )))
                .at(Location::file(ledger))
                .detailed_with(Detail::new(
                    "It is cargo's internal format, so it can move. A ledger that cannot be \
                     read is said out loud rather than reported as an empty lane.",
                )))
            })?;
        // Each key is "<name> <version> (<source>)". The name is what a manifest
        // declares, so it is the first field and nothing else.
        Ok(Some(
            parsed
                .v1
                .keys()
                .filter_map(|key| key.split(' ').next())
                .map(ToOwned::to_owned)
                .collect(),
        ))
    }

    /// What npm has installed globally, from npm's own JSON.
    fn npm_installed(&self, cx: &Context) -> Result<Option<BTreeSet<String>>, Box<Finding>> {
        let Some(npm) = cx
            .path()
            .iter()
            .map(|dir| dir.join("npm"))
            .find(|candidate| candidate.is_file())
            .map(|candidate| Tool::at_path("npm", candidate.into_std_path_buf()))
        else {
            return Ok(None);
        };

        let mut command = npm.command();
        command.args(["ls", "--global", "--depth=0", "--json"]);
        let exit = npm.run_within(&mut command, BUDGET).map_err(|error| {
            Box::new(self.id.broken(Summary::lossy(&format!(
                "npm could not list its global packages: {error}"
            ))))
        })?;
        // The status is not the answer: `npm ls` exits non-zero on an unmet peer
        // dependency while still printing the tree it was asked for.
        let parsed: NpmGlobals = serde_json::from_str(&exit.stdout).map_err(|error| {
            Box::new(self.id.broken(Summary::lossy(&format!(
                "npm's global listing is not in a shape this station knows: {error}"
            ))))
        })?;
        Ok(Some(parsed.dependencies.into_keys().collect()))
    }
}

/// The names a manifest declares.
///
/// One per line, `#` starting a comment anywhere, blanks ignored — the shape
/// both files document in their own headers and `13-cargo-bins.sh` already
/// implements.
fn declared(text: &str) -> BTreeSet<String> {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or("").trim())
        .filter(|name| !name.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use camino::{Utf8Path, Utf8PathBuf};
    use relic_core::finding::Severity;

    use super::*;

    /// A machine with two manifests, a cargo ledger, and a shimmed `npm`.
    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
        bin: Utf8PathBuf,
    }

    impl Machine {
        /// Both lanes declaring exactly what is installed.
        fn sound() -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let home =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            let bin = home.join("bin");
            fs_err::create_dir_all(&bin).expect("a bin dir");
            let machine = Self {
                _dir: dir,
                home,
                bin,
            };
            machine
                .manifest(&CARGO, "# a comment\ncargo-one   # its caller\ncargo-two\n")
                .ledger(&["cargo-one", "cargo-two"])
                .manifest(&NPM, "one\ntwo\n")
                .npm(r#"{"dependencies":{"one":{},"two":{},"npm":{}}}"#);
            machine
        }

        fn write(at: &Utf8Path, body: &str, mode: u32) {
            if let Some(parent) = at.parent() {
                fs_err::create_dir_all(parent).expect("a parent");
            }
            fs_err::write(at, body).expect("written");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs_err::set_permissions(at, std::fs::Permissions::from_mode(mode)).expect("a mode");
            }
        }

        fn manifest(&self, lane: &Lane, body: &str) -> &Self {
            Self::write(&self.home.join(lane.manifest), body, 0o644);
            self
        }

        /// Cargo's v1 ledger, keyed the way cargo keys it.
        fn ledger(&self, names: &[&str]) -> &Self {
            let body = std::iter::once("[v1]".to_owned())
                .chain(names.iter().map(|name| {
                    format!(
                        "\"{name} 1.0.0 (registry+https://github.com/rust-lang/crates.io-index)\" \
                         = [\"{name}\"]"
                    )
                }))
                .collect::<Vec<_>>()
                .join("\n");
            Self::write(&self.home.join(CRATES_LEDGER), &format!("{body}\n"), 0o644);
            self
        }

        fn raw_ledger(&self, body: &str) -> &Self {
            Self::write(&self.home.join(CRATES_LEDGER), body, 0o644);
            self
        }

        /// An `npm` that prints a canned `ls --json`.
        fn npm(&self, json: &str) -> &Self {
            Self::write(
                &self.bin.join("npm"),
                &format!("#!/bin/sh\ncat <<'JSON'\n{json}\nJSON\n"),
                0o755,
            );
            self
        }

        fn context(&self) -> Context {
            Context::new(self.home.clone(), vec![self.bin.clone()])
        }

        fn outcome(&self) -> Outcome {
            ManifestDrift::default()
                .check(&self.context())
                .expect("the station ran")
        }

        fn findings(&self) -> Vec<Finding> {
            match self.outcome() {
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

    #[test]
    fn two_lanes_in_step_have_nothing_to_say() {
        assert!(Machine::sound().findings().is_empty());
    }

    #[test]
    fn a_declared_crate_that_is_not_installed_is_soft_and_says_why_up_did_not_fix_it() {
        let machine = Machine::sound();
        machine.ledger(&["cargo-one"]);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Soft);
        assert!(
            finding.summary.as_str().contains("cargo-two is declared"),
            "{finding:?}"
        );
        assert!(
            finding
                .detail
                .as_ref()
                .is_some_and(|d| d.as_str().contains("bootstrap runs once")),
            "the reason it is still missing is the whole of the finding's value"
        );
    }

    #[test]
    fn an_installed_crate_declared_nowhere_is_the_drift_a_restore_would_find() {
        let machine = Machine::sound();
        machine.ledger(&["cargo-one", "cargo-two", "cargo-stray"]);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Soft);
        assert!(
            finding
                .summary
                .as_str()
                .contains("cargo-stray is installed"),
            "{finding:?}"
        );
    }

    #[test]
    fn the_installer_bootstrap_uses_is_never_undeclared_because_declaring_it_is_circular() {
        let machine = Machine::sound();
        machine.ledger(&["cargo-one", "cargo-two", "cargo-binstall"]);
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn npm_listing_itself_is_not_a_global_package_anyone_chose() {
        let machine = Machine::sound();
        machine.npm(r#"{"dependencies":{"one":{},"two":{},"npm":{}}}"#);
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn an_exemption_is_per_lane_and_does_not_leak_across() {
        let machine = Machine::sound();
        // `npm` is exempt in the npm lane; a crate of that name is not.
        machine.ledger(&["cargo-one", "cargo-two", "npm"]);
        let finding = machine.only();
        assert!(
            finding.summary.as_str().contains("npm is installed"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_missing_global_package_is_reported_the_same_way_a_crate_is() {
        let machine = Machine::sound();
        machine.npm(r#"{"dependencies":{"one":{},"npm":{}}}"#);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Soft);
        assert!(
            finding.summary.as_str().contains("two is declared"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_ledger_that_cannot_be_parsed_is_said_out_loud_and_never_read_as_an_empty_lane() {
        let machine = Machine::sound();
        machine.raw_ledger("[v1\nthis is not toml\n");
        let findings = machine.findings();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].severity, Severity::Broken);
        assert!(
            findings[0].summary.as_str().contains("not in a shape"),
            "an unreadable oracle must not manufacture two declared-and-absent findings"
        );
    }

    #[test]
    fn a_machine_that_has_never_cargo_installed_has_no_cargo_lane_to_compare() {
        let machine = Machine::sound();
        fs_err::remove_file(machine.home.join(CRATES_LEDGER)).expect("removed");
        assert!(
            machine.findings().is_empty(),
            "no ledger is a machine without the lane, not one with an empty lane"
        );
    }

    #[test]
    fn neither_lane_present_is_skipped_rather_than_graded_clean() {
        let machine = Machine::sound();
        fs_err::remove_file(machine.home.join(CRATES_LEDGER)).expect("removed");
        fs_err::remove_file(machine.bin.join("npm")).expect("removed");
        let Outcome::Skipped(reason) = machine.outcome() else {
            panic!("a station with nothing to read must say so");
        };
        assert!(reason.as_str().contains("neither"), "{reason}");
    }

    #[test]
    fn a_manifest_declaring_nothing_is_refused_rather_than_read_as_a_full_lane() {
        let machine = Machine::sound();
        machine.manifest(&CARGO, "# only comments here\n\n");
        let findings = machine.findings();
        assert_eq!(findings.len(), 1, "{findings:?}");
        assert_eq!(findings[0].severity, Severity::Broken);
        assert!(
            findings[0].summary.as_str().contains("declares nothing"),
            "an empty manifest would otherwise report every installed crate as undeclared"
        );
    }

    #[test]
    fn a_manifest_that_is_not_here_is_broken_and_not_an_empty_declaration() {
        let machine = Machine::sound();
        fs_err::remove_file(machine.home.join(NPM.manifest)).expect("removed");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("could not be read"));
    }

    #[test]
    fn npm_exiting_non_zero_is_not_an_answer_about_the_tree_it_printed() {
        let machine = Machine::sound();
        Machine::write(
            &machine.bin.join("npm"),
            "#!/bin/sh\ncat <<'JSON'\n{\"dependencies\":{\"one\":{},\"two\":{},\"npm\":{}}}\nJSON\nexit 1\n",
            0o755,
        );
        assert!(
            machine.findings().is_empty(),
            "npm ls exits non-zero on an unmet peer dependency and still prints the tree"
        );
    }

    #[test]
    fn a_comment_ends_a_declaration_wherever_it_starts() {
        assert_eq!(
            declared("a  # why\n# whole line\n\n  b\n"),
            ["a".to_owned(), "b".to_owned()].into_iter().collect()
        );
    }

    #[test]
    fn a_ledger_key_names_the_crate_first_and_the_rest_is_not_its_name() {
        let key = "cargo-nextest 0.9.143 (registry+https://github.com/rust-lang/crates.io-index)";
        assert_eq!(key.split(' ').next(), Some("cargo-nextest"));
    }

    #[test]
    fn the_committed_manifests_are_ones_this_station_can_read() {
        let root = Utf8PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
        for lane in [&CARGO, &NPM] {
            let at = root.join(lane.manifest.trim_start_matches(".config/"));
            let text = fs_err::read_to_string(&at).expect("a committed manifest");
            assert!(
                !declared(&text).is_empty(),
                "{} declares nothing this station can see",
                lane.name
            );
        }
    }
}
