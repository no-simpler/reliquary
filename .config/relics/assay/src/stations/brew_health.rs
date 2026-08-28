//! Homebrew packages and Brewfile declarations that have rotted upstream.
//!
//! Homebrew announces this class of decay only as unattributed noise during
//! `brew update` — "Some installed kegs have no formulae!" — which scrolls past
//! inside `up` and says nothing about what to do. This names the package, the
//! reason, the deadline and the replacement, and extends the same scrutiny to
//! the Brewfiles, so a declaration that no longer resolves is caught here rather
//! than on the next machine's bootstrap.
//!
//! Verification only: it never installs, uninstalls, taps or upgrades. It reads
//! install receipts and Homebrew's local API cache, so it is offline on any
//! machine that has run `brew update` once.

use std::collections::BTreeMap;
use std::fmt::Write as _;

use anyhow::{Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};
use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};
use relic_core::tool::Tool;
use serde::Deserialize;

use crate::probe;
use crate::station::{Context, Station};

/// Where the Brewfiles live, under the home directory.
const BREW_DIR: &str = ".config/brew";

/// Where a deliberate exception to check five is recorded — the same role
/// `yadm/unmanaged` plays for a path nobody manages.
const UNDECLARED: &str = ".config/brew/undeclared";

/// Where yadm lists the paths that ride the encrypted archive.
const ENCRYPT: &str = ".config/yadm/encrypt";

/// The station.
pub struct BrewHealth {
    id: StationId,
}

impl Default for BrewHealth {
    fn default() -> Self {
        Self {
            id: StationId::from_static("brew-health"),
        }
    }
}

impl Station for BrewHealth {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "installed packages and Brewfile declarations still exist upstream"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let Some(brew) = probe::resolve("brew", cx.path()).map(Brew::new) else {
            // Not a failure: Homebrew is macOS-only here, and a Linux machine
            // has nothing to check.
            return Ok(Outcome::Skipped(Summary::lossy(
                "Homebrew is not installed on this machine",
            )));
        };

        let mut findings = Vec::new();
        let installed = brew.installed();
        match &installed {
            Ok(installed) => {
                findings.extend(rotting(&self.id, installed, &brew));
                findings.extend(orphaned(&self.id, installed, &brew));
            }
            Err(error) => findings.push(self.id.soft(Summary::lossy(&format!(
                "the installed-package data could not be read: {error}"
            )))),
        }
        findings.extend(declarations(&self.id, cx, &brew));
        Ok(Outcome::Ran(findings))
    }
}

// --- Homebrew, as a capability ---------------------------------------------

/// Homebrew, proven present.
struct Brew {
    tool: Tool,
}

impl Brew {
    fn new(program: Utf8PathBuf) -> Self {
        Self {
            tool: Tool::at_path("brew", program.into_std_path_buf()),
        }
    }

    /// A `brew` invocation that will not update itself.
    ///
    /// The check has to stay fast and side-effect-free even when Homebrew
    /// thinks it is due for a refresh.
    fn command(&self) -> std::process::Command {
        let mut command = self.tool.command();
        command.env("HOMEBREW_NO_AUTO_UPDATE", "1");
        command
    }

    fn ask(&self, args: &[&str]) -> Option<String> {
        let mut command = self.command();
        command.args(args);
        self.tool
            .capture(&mut command)
            .ok()
            .map(|output| output.line().to_owned())
    }

    fn accepts(&self, args: &[&str]) -> bool {
        let mut command = self.command();
        command.args(args);
        self.tool.capture(&mut command).is_ok()
    }

    /// The lines of an answer, without the blanks.
    fn lines(&self, args: &[&str]) -> Vec<String> {
        self.ask(args)
            .into_iter()
            .flat_map(|text| {
                text.lines()
                    .map(str::trim)
                    .filter(|line| !line.is_empty())
                    .map(str::to_owned)
                    .collect::<Vec<_>>()
            })
            .collect()
    }

    /// Install receipts merged with the cached API data, which is where the
    /// deprecation metadata lives.
    fn installed(&self) -> Result<Installed> {
        let mut command = self.command();
        command.args(["info", "--json=v2", "--installed"]);
        let output = self
            .tool
            .capture(&mut command)
            .context("asking brew what is installed")?;
        serde_json::from_str(&output.stdout).context("reading brew's installed-package JSON")
    }

    /// The names a core tap currently ships, or `None` when the roster has never
    /// been cached on this machine.
    fn roster(&self, file: &str) -> Option<Vec<String>> {
        let cache = self.ask(&["--cache"])?;
        let path = Utf8Path::new(&cache).join("api").join(file);
        let text = fs_err::read_to_string(path).ok()?;
        Some(
            text.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_owned)
                .collect(),
        )
    }

    /// Where third-party taps are checked out.
    fn taps_root(&self) -> Option<Utf8PathBuf> {
        self.ask(&["--repository"])
            .map(|repo| Utf8Path::new(&repo).join("Library").join("Taps"))
    }
}

// --- Homebrew's schema, at the boundary ------------------------------------
//
// Third-party and gaining fields on every upgrade, so no `deny_unknown_fields`:
// the carve-out for a schema we do not own. Everything past these structs is
// ours.
//
// The two lanes are separate types rather than one with both identifier fields,
// because they genuinely disagree: a formula's `name` is a string and a **cask's
// `name` is a list** of its human-readable titles, with `token` carrying the
// identifier. The shell checker never noticed, having only ever read `token` for
// a cask; typing the schema is what surfaced it.

#[derive(Deserialize, Debug, Default)]
struct Installed {
    #[serde(default)]
    formulae: Vec<Formula>,
    #[serde(default)]
    casks: Vec<Cask>,
}

#[derive(Deserialize, Debug, Default)]
struct Formula {
    #[serde(default)]
    name: String,
    #[serde(flatten)]
    common: Common,
}

#[derive(Deserialize, Debug, Default)]
struct Cask {
    #[serde(default)]
    token: String,
    #[serde(flatten)]
    common: Common,
}

/// What both lanes carry, and all this station reads.
#[derive(Deserialize, Debug, Default)]
struct Common {
    #[serde(default)]
    tap: Option<String>,
    #[serde(default)]
    deprecated: bool,
    #[serde(default)]
    deprecation_reason: Option<String>,
    #[serde(default)]
    deprecation_date: Option<String>,
    #[serde(default)]
    deprecation_replacement_formula: Option<String>,
    #[serde(default)]
    deprecation_replacement_cask: Option<String>,
    #[serde(default)]
    disabled: bool,
    #[serde(default)]
    disable_reason: Option<String>,
    #[serde(default)]
    disable_date: Option<String>,
    #[serde(default)]
    disable_replacement_formula: Option<String>,
    #[serde(default)]
    disable_replacement_cask: Option<String>,
}

/// Which lane a package is in. The two differ in more than a label: only a
/// formula has a dependency lane, and only a cask is named by a token.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Kind {
    Formula,
    Cask,
}

impl Kind {
    fn as_str(self) -> &'static str {
        match self {
            Self::Formula => "formula",
            Self::Cask => "cask",
        }
    }

    /// The core tap this kind lives in.
    fn core_tap(self) -> &'static str {
        match self {
            Self::Formula => "homebrew/core",
            Self::Cask => "homebrew/cask",
        }
    }

    /// The cached roster file listing what the core tap ships.
    fn roster_file(self) -> &'static str {
        match self {
            Self::Formula => "formula_names.txt",
            Self::Cask => "cask_names.txt",
        }
    }

    /// Where a tap keeps this kind's definitions.
    fn tap_subdirectory(self) -> &'static str {
        match self {
            Self::Formula => "Formula",
            Self::Cask => "Casks",
        }
    }

    /// The flag that narrows a `brew info` to this lane.
    fn flag(self) -> &'static str {
        match self {
            Self::Formula => "--formula",
            Self::Cask => "--cask",
        }
    }
}

impl std::fmt::Display for Kind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Common {
    /// Homebrew's own pointer to what supersedes this package.
    fn replacement(&self) -> Option<String> {
        let candidates = [
            (&self.deprecation_replacement_formula, Kind::Formula),
            (&self.deprecation_replacement_cask, Kind::Cask),
            (&self.disable_replacement_formula, Kind::Formula),
            (&self.disable_replacement_cask, Kind::Cask),
        ];
        candidates.into_iter().find_map(|(value, kind)| {
            value
                .as_deref()
                .filter(|text| !text.is_empty())
                .map(|text| format!("use the {kind} {text} instead"))
        })
    }
}

/// Every installed package as the three things this station reads: what it is
/// called, which lane it is in, and what upstream says about it.
fn every(installed: &Installed) -> impl Iterator<Item = (&str, Kind, &Common)> {
    installed
        .formulae
        .iter()
        .map(|formula| (named(&formula.name), Kind::Formula, &formula.common))
        .chain(
            installed
                .casks
                .iter()
                .map(|cask| (named(&cask.token), Kind::Cask, &cask.common)),
        )
}

/// A package that named itself, or the placeholder for one that did not.
fn named(name: &str) -> &str {
    if name.is_empty() { "?" } else { name }
}

// --- Check 1: deprecated or disabled ---------------------------------------

/// Something upstream has put on a clock. Soft: it still works today.
fn rotting(station: &StationId, installed: &Installed, brew: &Brew) -> Vec<Finding> {
    let core_casks = brew.roster(Kind::Cask.roster_file());
    let mut findings = Vec::new();

    for (name, kind, package) in every(installed) {
        let (state, reason, date, becomes) = if package.disabled {
            (
                "disabled",
                package.disable_reason.as_deref(),
                package.disable_date.as_deref(),
                "removed",
            )
        } else if package.deprecated {
            (
                "deprecated",
                package.deprecation_reason.as_deref(),
                package.deprecation_date.as_deref(),
                "disabled",
            )
        } else {
            continue;
        };

        let mut summary = format!("{kind} {name} — {state} upstream");
        if let Some(reason) = reason.filter(|text| !text.is_empty()) {
            let _ = write!(summary, " ({reason})");
        }
        if let Some(date) = date.filter(|text| !text.is_empty()) {
            let _ = write!(summary, "; {becomes} on {date}");
        }

        let mut finding = station.soft(Summary::lossy(&summary));
        if let Some(replacement) = package.replacement() {
            finding = finding.fixed_by(FixHint::lossy(&replacement));
        } else if kind == Kind::Formula
            && core_casks
                .as_ref()
                .is_some_and(|casks| casks.iter().any(|cask| cask == name))
        {
            finding = finding.fixed_by(FixHint::lossy(&format!(
                "a cask named {name} exists — declare it as a cask instead"
            )));
        }
        findings.push(finding);
    }
    findings
}

// --- Checks 2 and 3: orphaned kegs -----------------------------------------

/// Whether a tapped repository still carries a definition.
///
/// Taps lay files out flat, under `Formula/` or `Casks/`, sharded by first
/// letter as homebrew-core does, or under `HomebrewFormula/` — so every
/// sanctioned location is tried before calling something orphaned. `None` means
/// the tap is not on this machine, so there is nothing to judge against.
fn in_tap(taps_root: &Utf8Path, tap: &str, name: &str, kind: Kind) -> Option<bool> {
    let (user, repo) = tap.split_once('/')?;
    let root = taps_root.join(user).join(format!("homebrew-{repo}"));
    if !root.is_dir() {
        return None;
    }
    let sub = kind.tap_subdirectory();
    let file = format!("{name}.rb");
    let shard = name.chars().next()?.to_lowercase().to_string();
    Some(
        [
            root.join(sub).join(&file),
            root.join(sub).join(shard).join(&file),
            root.join(&file),
            root.join("HomebrewFormula").join(&file),
        ]
        .iter()
        .any(|candidate| candidate.is_file()),
    )
}

/// Installed, and gone from the tap it came from. Broken: the machine can no
/// longer be reproduced.
fn orphaned(station: &StationId, installed: &Installed, brew: &Brew) -> Vec<Finding> {
    let taps_root = brew.taps_root();
    let core_formulae = brew.roster(Kind::Formula.roster_file());
    let core_casks = brew.roster(Kind::Cask.roster_file());
    let mut findings = Vec::new();
    let mut notes: Vec<String> = Vec::new();

    for (name, kind, package) in every(installed) {
        let Some(tap) = package.tap.as_deref().filter(|tap| !tap.is_empty()) else {
            notes.push(format!(
                "{kind} {name} was installed outside any tap, so it cannot be judged"
            ));
            continue;
        };

        let present = if tap == kind.core_tap() {
            let roster = match kind {
                Kind::Formula => &core_formulae,
                Kind::Cask => &core_casks,
            };
            let Some(roster) = roster else {
                notes.push(format!(
                    "the {kind} roster is not cached ({}), so run `brew update`",
                    kind.roster_file()
                ));
                continue;
            };
            roster.iter().any(|entry| entry == name)
        } else {
            let Some(present) = taps_root
                .as_deref()
                .and_then(|root| in_tap(root, tap, name, kind))
            else {
                notes.push(format!(
                    "tap {tap} is not present on this machine, so {kind} {name} is unjudged"
                ));
                continue;
            };
            present
        };

        if !present {
            let mut finding = station.broken(Summary::lossy(&format!(
                "{kind} {name} is installed and no longer exists in {tap}"
            )));
            if kind == Kind::Formula
                && core_casks
                    .as_ref()
                    .is_some_and(|casks| casks.iter().any(|cask| cask == name))
            {
                finding = finding.fixed_by(FixHint::lossy(&format!(
                    "a cask named {name} exists — reinstall it as a cask and update the Brewfile"
                )));
            }
            findings.push(finding);
        }
    }

    notes.sort();
    notes.dedup();
    findings.extend(notes.iter().map(|note| station.note(Summary::lossy(note))));
    findings
}

// --- Checks 4 and 5: the Brewfiles -----------------------------------------

/// One Brewfile, parsed into the names it declares.
#[derive(Default, Debug)]
struct Declared {
    formulae: Vec<String>,
    casks: Vec<String>,
    taps: Vec<String>,
}

impl Declared {
    fn absorb(&mut self, text: &str) {
        for line in text.lines() {
            let line = line.trim();
            let Some((keyword, rest)) = line.split_once(char::is_whitespace) else {
                continue;
            };
            let Some(name) = quoted(rest) else { continue };
            match keyword {
                "brew" => self.formulae.push(name),
                "cask" => self.casks.push(name),
                "tap" => self.taps.push(name),
                _ => {}
            }
        }
    }

    fn tidy(&mut self) {
        for list in [&mut self.formulae, &mut self.casks, &mut self.taps] {
            list.sort();
            list.dedup();
        }
    }

    /// Every declared package name, tap prefixes dropped so `user/tap/name` and
    /// `name` compare equal — a Brewfile may spell either.
    fn bare(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .formulae
            .iter()
            .chain(&self.casks)
            .map(|name| bare(name))
            .collect();
        names.sort();
        names.dedup();
        names
    }
}

/// The first double-quoted field of a line.
fn quoted(text: &str) -> Option<String> {
    let after = text.split_once('"')?.1;
    let (inside, _) = after.split_once('"')?;
    (!inside.is_empty()).then(|| inside.to_owned())
}

/// A name without its tap prefix.
fn bare(name: &str) -> String {
    name.rsplit('/').next().unwrap_or(name).trim().to_owned()
}

/// Brewfile scopes the encrypt list names which are not on disk.
///
/// Some scopes ride the encrypted archive, and before `yadm decrypt` they are
/// simply absent. What they declare is then unknown — so "installed on request
/// and declared in no Brewfile" would be an accusation drawn from files this
/// machine cannot read. Nine packages were reported that way on a fresh
/// account, every one of them declared in a scope that was not there.
///
/// Membership is derived by expanding the pattern list, never by decrypting —
/// the same route the `yadm-coverage` station takes. A pattern carrying a glob is
/// skipped: it cannot prove a particular file absent.
fn absent_scopes(cx: &Context) -> Vec<String> {
    let Ok(text) = fs_err::read_to_string(cx.at(ENCRYPT)) else {
        return Vec::new();
    };

    let prefix = format!("{BREW_DIR}/Brewfile");
    let mut absent: Vec<String> = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#') && !line.starts_with('!'))
        .filter(|line| line.starts_with(&prefix))
        .filter(|line| !line.contains(['*', '?', '[']))
        .filter(|line| !cx.at(line).exists())
        .map(str::to_owned)
        .collect();
    absent.sort();
    absent.dedup();
    absent
}

/// What a tap holds, and whether brew may load from it.
#[derive(Deserialize)]
struct TapInfo {
    installed: bool,
    trusted: bool,
    formula_names: Vec<String>,
    cask_tokens: Vec<String>,
}

/// The bare names an installed-but-untrusted tap provides, each mapped to its
/// tap.
///
/// brew refuses to *load* a formula from an untrusted tap, and at the call site
/// that refusal is indistinguishable from the name not existing — which is how
/// `clc` was graded "no longer resolves to any formula" on a machine that had
/// simply never run `brew trust`. The tap's own listing answers what brew
/// declined to: `formula_names` and `cask_tokens` are directory metadata rather
/// than formula evaluation, so they are readable either way.
fn untrusted_offerings(brew: &Brew, taps: &[String]) -> BTreeMap<String, String> {
    let mut offered = BTreeMap::new();
    for tap in taps {
        let Some(raw) = brew.ask(&["tap-info", "--json", tap]) else {
            continue;
        };
        let Ok(infos) = serde_json::from_str::<Vec<TapInfo>>(&raw) else {
            continue;
        };
        for info in infos {
            if !info.installed || info.trusted {
                continue;
            }
            for name in info.formula_names.iter().chain(&info.cask_tokens) {
                offered.insert(bare(name), tap.clone());
            }
        }
    }
    offered
}

/// The Brewfiles present on disk.
///
/// Only what is there is read: the encrypted scopes may not be decrypted on this
/// machine, and their absence is not a problem to report.
fn brewfiles(dir: &Utf8Path) -> Vec<Utf8PathBuf> {
    let Ok(entries) = fs_err::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<Utf8PathBuf> = entries
        .flatten()
        .filter_map(|entry| Utf8PathBuf::from_path_buf(entry.path()).ok())
        .filter(|path| path.is_file())
        .filter(|path| {
            let name = path.file_name().unwrap_or_default();
            (name == "Brewfile" || name.starts_with("Brewfile@")) && !name.ends_with(".lock.json")
        })
        .collect();
    found.sort();
    found
}

/// Checks four and five: what the Brewfiles claim, and what they leave out.
fn declarations(station: &StationId, cx: &Context, brew: &Brew) -> Vec<Finding> {
    let dir = cx.at(BREW_DIR);
    let files = brewfiles(&dir);
    if files.is_empty() {
        return vec![
            station
                .soft(Summary::lossy("no Brewfile is present"))
                .at(Location::file(&dir)),
        ];
    }

    let mut declared = Declared::default();
    for file in &files {
        if let Ok(text) = fs_err::read_to_string(file) {
            declared.absorb(&text);
        }
    }
    declared.tidy();

    let mut findings = Vec::new();

    // A Brewfile-declared tap that is not installed here makes an unresolvable
    // name ambiguous, so record that first and soften the verdict accordingly.
    let taps_root = brew.taps_root();
    let mut untapped = false;
    for tap in &declared.taps {
        let installed = taps_root.as_ref().is_some_and(|root| {
            tap.split_once('/').is_some_and(|(user, repo)| {
                root.join(user).join(format!("homebrew-{repo}")).is_dir()
            })
        });
        if !installed {
            untapped = true;
            findings.push(station.note(Summary::lossy(&format!(
                "tap {tap} is declared and not tapped on this machine"
            ))));
        }
    }

    // A name an installed tap holds but brew may not load resolves fine; the
    // refusal is about trust, not existence.
    let untrusted = untrusted_offerings(brew, &declared.taps);

    findings.extend(unresolvable(
        station,
        brew,
        Kind::Formula,
        &declared.formulae,
        untapped,
        &untrusted,
    ));
    findings.extend(unresolvable(
        station,
        brew,
        Kind::Cask,
        &declared.casks,
        untapped,
        &untrusted,
    ));

    // Check five compares what is installed against what the Brewfiles declare,
    // so it is only answerable once every Brewfile is readable.
    let absent = absent_scopes(cx);
    if absent.is_empty() {
        findings.extend(undeclared(station, cx, brew, &declared));
    } else {
        let mut note = station.note(Summary::lossy(&format!(
            "{} encrypted Brewfile scope(s) are not on disk, so what they declare cannot be \
             read — the undeclared-package check is skipped rather than guessed",
            absent.len()
        )));
        if let Some(detail) = Detail::new(absent.join("\n")) {
            note = note.detailed(detail);
        }
        findings.push(note);
    }

    findings
}

/// Names a Brewfile declares that brew can no longer resolve.
///
/// A batch `brew info` settles the healthy case in one call; it aborts on the
/// first unknown name without naming it, so the slow per-name pass runs only
/// once something is already wrong.
fn unresolvable(
    station: &StationId,
    brew: &Brew,
    kind: Kind,
    names: &[String],
    untapped: bool,
    untrusted: &BTreeMap<String, String>,
) -> Vec<Finding> {
    if names.is_empty() {
        return Vec::new();
    }
    let flag = kind.flag();
    let mut batch: Vec<&str> = vec!["info", flag, "--json=v2"];
    batch.extend(names.iter().map(String::as_str));
    if brew.accepts(&batch) {
        return Vec::new();
    }

    let core_casks = brew.roster(Kind::Cask.roster_file());
    names
        .iter()
        .filter(|name| !brew.accepts(&["info", flag, "--json=v2", name]))
        .map(|name| {
            if let Some(tap) = untrusted.get(&bare(name)) {
                // The tap holds it; brew merely will not load it unmodified by
                // a trust decision. That is a thing to do, not a rotted
                // declaration.
                return station
                    .note(Summary::lossy(&format!(
                        "the Brewfiles declare {kind} \"{name}\", which tap {tap} provides but \
                         brew may not load until the tap is trusted"
                    )))
                    .fixed_by(FixHint::lossy(&format!("brew trust --{kind} {tap}/{name}")));
            }
            if untapped {
                // A tap the Brewfiles declare is missing here, so the name may
                // be perfectly valid on a machine that has run bootstrap.
                return station.soft(Summary::lossy(&format!(
                    "the Brewfiles declare {kind} \"{name}\", which does not resolve — and a declared tap is missing here, so this may be a false alarm"
                )));
            }
            let mut finding = station.broken(Summary::lossy(&format!(
                "the Brewfiles declare {kind} \"{name}\", which no longer resolves to any {kind}"
            )));
            if kind == Kind::Formula
                && core_casks
                    .as_ref()
                    .is_some_and(|casks| casks.iter().any(|cask| cask == name))
            {
                finding = finding.fixed_by(FixHint::lossy(&format!(
                    "a cask named {name} exists — change the line to cask \"{name}\""
                )));
            }
            finding
        })
        .collect()
}

/// Installed on request and declared nowhere — the inverse drift, and the one
/// that only shows up on a restore.
///
/// `--installed-on-request` is the whole trick: it excludes packages pulled in
/// as dependencies, which are reproduced by declaring their dependents and must
/// not be declared themselves. Casks have no dependency lane, so all of them
/// count.
///
/// Soft, never broken. A scope that is not decrypted here is not read, so the
/// verdict can be incomplete; and a machine is allowed to hold a package on
/// purpose, which `brew/undeclared` is where to say.
fn undeclared(station: &StationId, cx: &Context, brew: &Brew, declared: &Declared) -> Vec<Finding> {
    let exceptions = cx.at(UNDECLARED);
    let excused: Vec<String> = fs_err::read_to_string(&exceptions)
        .unwrap_or_default()
        .lines()
        .map(|line| line.split('#').next().unwrap_or_default().trim())
        .filter(|line| !line.is_empty())
        .filter_map(|line| line.split_whitespace().next())
        .map(bare)
        .collect();

    let known = declared.bare();
    let mut wanted = brew.lines(&["leaves", "--installed-on-request"]);
    wanted.extend(brew.lines(&["list", "--cask"]));

    let mut missing: Vec<String> = wanted
        .into_iter()
        .map(|name| bare(&name))
        .filter(|name| !known.contains(name) && !excused.contains(name))
        .collect();
    missing.sort();
    missing.dedup();

    let where_to_say = FixHint::lossy(&format!(
        "declare it, uninstall it, or record the decision in ~/{UNDECLARED}"
    ));
    missing
        .into_iter()
        .map(|name| {
            station
                .soft(Summary::lossy(&format!(
                    "{name} is installed on request and declared in no Brewfile, so it will not survive a restore"
                )))
                .fixed_by(where_to_say.clone())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use relic_core::finding::Severity;

    use super::*;

    /// A Homebrew stand-in: a shell script answering the six invocations this
    /// station makes, from files a test writes. Fixtures rather than the real
    /// brew, because the interesting states — an orphaned keg, a Brewfile entry
    /// that stopped resolving — cannot be produced on a healthy machine.
    struct Machine {
        _keep: tempfile::TempDir,
        home: Utf8PathBuf,
        bin: Utf8PathBuf,
    }

    impl Machine {
        fn new() -> Self {
            use std::os::unix::fs::PermissionsExt as _;

            let keep = tempfile::tempdir().expect("a scratch dir");
            let root = Utf8PathBuf::from_path_buf(keep.path().to_path_buf()).expect("utf-8");
            let home = root.join("home");
            let bin = root.join("bin");
            let data = root.join("data");
            for dir in [&home, &bin, &data] {
                fs_err::create_dir_all(dir).expect("created");
            }
            fs_err::create_dir_all(data.join("cache").join("api")).expect("created");
            fs_err::create_dir_all(data.join("repo").join("Library").join("Taps"))
                .expect("created");

            let machine = Self {
                _keep: keep,
                home,
                bin: bin.clone(),
            };
            machine.file("installed.json", r#"{"formulae":[],"casks":[]}"#);
            machine.file("leaves", "");
            machine.file("casklist", "");
            machine.file("resolvable", "");
            machine.roster("formula_names.txt", "");
            machine.roster("cask_names.txt", "");

            let script = format!(
                r#"#!/bin/sh
case "$1" in
  --cache) echo "{data}/cache"; exit 0;;
  --repository) echo "{data}/repo"; exit 0;;
  leaves) cat "{data}/leaves"; exit 0;;
  list) cat "{data}/casklist"; exit 0;;
  info)
    shift
    case "$1" in
      --json=v2) cat "{data}/installed.json"; exit 0;;
      --formula|--cask)
        shift 2
        for n in "$@"; do grep -qxF -- "$n" "{data}/resolvable" || exit 1; done
        exit 0;;
    esac;;
  tap-info)
    shift 2
    f=$(echo "$1" | tr / -)
    if [ -f "{data}/tapinfo-$f" ]; then cat "{data}/tapinfo-$f"; exit 0; fi
    exit 1;;
esac
exit 1
"#
            );
            let brew = bin.join("brew");
            fs_err::write(&brew, script).expect("written");
            fs_err::set_permissions(&brew, std::fs::Permissions::from_mode(0o755))
                .expect("made executable");
            machine
        }

        fn data(&self) -> Utf8PathBuf {
            self.bin.parent().expect("a parent").join("data")
        }

        fn file(&self, name: &str, text: &str) -> &Self {
            fs_err::write(self.data().join(name), text).expect("written");
            self
        }

        fn roster(&self, name: &str, text: &str) -> &Self {
            fs_err::write(self.data().join("cache").join("api").join(name), text).expect("written");
            self
        }

        fn tap(&self, tap: &str) -> &Self {
            let (user, repo) = tap.split_once('/').expect("a tap name");
            fs_err::create_dir_all(
                self.data()
                    .join("repo")
                    .join("Library")
                    .join("Taps")
                    .join(user)
                    .join(format!("homebrew-{repo}")),
            )
            .expect("created");
            self
        }

        /// What `brew tap-info --json` answers for one tap.
        fn tap_info(&self, tap: &str, trusted: bool, formulae: &[&str]) -> &Self {
            let names = formulae
                .iter()
                .map(|name| format!("\"{tap}/{name}\""))
                .collect::<Vec<_>>()
                .join(",");
            let json = format!(
                r#"[{{"installed":true,"trusted":{trusted},"formula_names":[{names}],"cask_tokens":[]}}]"#
            );
            fs_err::write(
                self.data()
                    .join(format!("tapinfo-{}", tap.replace('/', "-"))),
                json,
            )
            .expect("written");
            self
        }

        /// The encrypt list this machine answers for.
        fn encrypt(&self, text: &str) -> &Self {
            let path = self.home.join(ENCRYPT);
            fs_err::create_dir_all(path.parent().expect("a parent")).expect("created");
            fs_err::write(path, text).expect("written");
            self
        }

        fn brewfile(&self, name: &str, text: &str) -> &Self {
            let dir = self.home.join(BREW_DIR);
            fs_err::create_dir_all(&dir).expect("created");
            fs_err::write(dir.join(name), text).expect("written");
            self
        }

        fn excuse(&self, text: &str) -> &Self {
            let path = self.home.join(UNDECLARED);
            fs_err::create_dir_all(path.parent().expect("a parent")).expect("created");
            fs_err::write(path, text).expect("written");
            self
        }

        fn cx(&self) -> Context {
            Context::new(self.home.clone(), vec![self.bin.clone()])
        }

        fn outcome(&self) -> Outcome {
            BrewHealth::default()
                .check(&self.cx())
                .expect("the station ran")
        }

        fn findings(&self) -> Vec<Finding> {
            match self.outcome() {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("skipped: {reason}"),
            }
        }

        fn summaries(&self) -> Vec<String> {
            self.findings()
                .iter()
                .map(|finding| finding.summary.to_string())
                .collect()
        }
    }

    /// A minimal Brewfile that declares nothing, so a test can isolate one check
    /// without the "no Brewfile" finding.
    const EMPTY_BREWFILE: &str = "# nothing declared\n";

    fn counts(findings: &[Finding]) -> (usize, usize, usize) {
        let of = |wanted: Severity| findings.iter().filter(|f| f.severity == wanted).count();
        (of(Severity::Broken), of(Severity::Soft), of(Severity::Note))
    }

    #[test]
    fn a_machine_without_homebrew_is_skipped_not_failed() {
        let machine = Machine::new();
        fs_err::remove_file(machine.bin.join("brew")).expect("removed");
        assert!(matches!(machine.outcome(), Outcome::Skipped(_)));
    }

    /// brew refuses to *load* a formula from an untrusted tap, and at the call
    /// site that refusal looks exactly like the name not existing. `clc` was
    /// graded a rotted declaration on a machine that had simply never trusted
    /// the tap.
    #[test]
    fn a_name_an_untrusted_tap_provides_is_a_note_not_a_rotted_declaration() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", "tap \"no-simpler/tap\"\nbrew \"clc\"\n");
        machine.tap("no-simpler/tap");
        machine.tap_info("no-simpler/tap", false, &["clc"]);
        let findings = machine.findings();
        assert_eq!(counts(&findings), (0, 0, 1));
        assert!(
            findings
                .first()
                .is_some_and(|f| f.summary.as_str().contains("trusted")),
            "{findings:?}"
        );
    }

    /// The escape hatch is narrow: a trusted tap that genuinely does not hold
    /// the name is still a rotted declaration.
    #[test]
    fn a_trusted_tap_still_fails_a_name_it_does_not_hold() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", "tap \"no-simpler/tap\"\nbrew \"gone\"\n");
        machine.tap("no-simpler/tap");
        machine.tap_info("no-simpler/tap", true, &["clc"]);
        let findings = machine.findings();
        assert_eq!(counts(&findings), (1, 0, 0));
    }

    /// Check five compares what is installed against what the Brewfiles
    /// declare, so a scope that is still encrypted makes it unanswerable. Nine
    /// packages were accused on a fresh account, every one declared in a scope
    /// that was not on disk.
    #[test]
    fn an_absent_encrypted_scope_skips_the_undeclared_check() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        machine.encrypt(".config/brew/Brewfile@private\n");
        machine.file("leaves", "stray\n");
        let findings = machine.findings();
        assert_eq!(counts(&findings), (0, 0, 1));
        assert!(
            !machine
                .summaries()
                .iter()
                .any(|s| s.contains("installed on request")),
            "{:?}",
            machine.summaries()
        );
    }

    /// The skip is about absence, not about the lane existing. Every named
    /// scope present means check five can answer as before.
    #[test]
    fn a_present_encrypted_scope_does_not_skip_the_undeclared_check() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        machine.brewfile("Brewfile@private", EMPTY_BREWFILE);
        machine.encrypt(".config/brew/Brewfile@private\n");
        machine.file("leaves", "stray\n");
        let findings = machine.findings();
        assert_eq!(counts(&findings), (0, 1, 0));
        assert!(
            machine
                .summaries()
                .iter()
                .any(|s| s.contains("stray is installed on request")),
            "{:?}",
            machine.summaries()
        );
    }

    #[test]
    fn a_healthy_machine_has_nothing_to_say() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        assert_eq!(machine.summaries(), Vec::<String>::new());
    }

    #[test]
    fn a_deprecated_package_is_soft_and_carries_its_deadline() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        machine.file(
            "installed.json",
            r#"{"formulae":[{"name":"sentry-cli","tap":"homebrew/core",
                 "deprecated":true,"deprecation_reason":"relicensed",
                 "deprecation_date":"2026-01-01",
                 "deprecation_replacement_cask":"sentry-cli"}],"casks":[]}"#,
        );
        machine.roster("formula_names.txt", "sentry-cli\n");
        let findings = machine.findings();
        assert_eq!(counts(&findings), (0, 1, 0));
        let finding = findings.first().expect("a finding");
        let summary = finding.summary.as_str();
        assert!(summary.contains("deprecated upstream"), "{summary}");
        assert!(summary.contains("relicensed"), "{summary}");
        assert!(summary.contains("disabled on 2026-01-01"), "{summary}");
        assert!(
            finding
                .fix
                .as_ref()
                .is_some_and(|fix| fix.as_str().contains("cask sentry-cli")),
            "{finding:?}"
        );
    }

    #[test]
    fn a_disabled_package_says_when_it_is_removed_not_disabled() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        machine.file(
            "installed.json",
            r#"{"formulae":[],"casks":[{"token":"gone","name":["Long Title","Other"],
                 "tap":"homebrew/cask","disabled":true,"disable_date":"2026-02-02"}]}"#,
        );
        machine.roster("cask_names.txt", "gone\n");
        let summaries = machine.summaries();
        assert!(
            summaries
                .first()
                .is_some_and(|s| s.contains("cask gone") && s.contains("removed on 2026-02-02")),
            "{summaries:?}"
        );
    }

    /// The regression for the schema mismatch the port surfaced: a cask's `name`
    /// is a list, and a struct that expected a string there refused the whole
    /// document — taking every other check down with it.
    #[test]
    fn a_cask_whose_name_is_a_list_still_parses() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        machine.file(
            "installed.json",
            r#"{"formulae":[],"casks":[{"token":"ok","name":["A","B"],"tap":"homebrew/cask"}]}"#,
        );
        machine.roster("cask_names.txt", "ok\n");
        assert_eq!(machine.summaries(), Vec::<String>::new());
    }

    #[test]
    fn a_keg_its_tap_no_longer_carries_is_broken() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        machine.file(
            "installed.json",
            r#"{"formulae":[{"name":"vanished","tap":"homebrew/core"}],"casks":[]}"#,
        );
        machine.roster("formula_names.txt", "something-else\n");
        machine.roster("cask_names.txt", "vanished\n");
        let findings = machine.findings();
        assert_eq!(counts(&findings), (1, 0, 0));
        let finding = findings.first().expect("a finding");
        assert!(
            finding
                .summary
                .as_str()
                .contains("no longer exists in homebrew/core"),
            "{finding:?}"
        );
        assert!(
            finding
                .fix
                .as_ref()
                .is_some_and(|fix| fix.as_str().contains("reinstall it as a cask")),
            "{finding:?}"
        );
    }

    #[test]
    fn what_cannot_be_judged_is_a_note_and_never_a_verdict() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        machine.file(
            "installed.json",
            r#"{"formulae":[{"name":"loose"},
                            {"name":"tapped","tap":"someone/theirs"}],
                "casks":[]}"#,
        );
        let findings = machine.findings();
        assert_eq!(counts(&findings), (0, 0, 2));
        assert_eq!(
            relic_core::finding::Grade::of(&findings),
            relic_core::finding::Grade::Ok
        );
        let summaries = machine.summaries();
        assert!(
            summaries.iter().any(|s| s.contains("outside any tap")),
            "{summaries:?}"
        );
        assert!(
            summaries.iter().any(|s| s.contains("someone/theirs")),
            "{summaries:?}"
        );
    }

    #[test]
    fn a_present_tap_is_judged_against_its_own_layout() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        machine.tap("someone/theirs");
        machine.file(
            "installed.json",
            r#"{"formulae":[{"name":"theirs","tap":"someone/theirs"}],"casks":[]}"#,
        );
        // Nothing on disk under the tap, so the formula is genuinely orphaned.
        assert_eq!(counts(&machine.findings()), (1, 0, 0));

        // The same formula, laid out under the sharded path homebrew-core uses.
        let sharded = machine
            .data()
            .join("repo/Library/Taps/someone/homebrew-theirs/Formula/t");
        fs_err::create_dir_all(&sharded).expect("created");
        fs_err::write(sharded.join("theirs.rb"), "class Theirs").expect("written");
        assert_eq!(counts(&machine.findings()), (0, 0, 0));
    }

    #[test]
    fn an_uncached_roster_is_a_note_asking_for_brew_update() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", EMPTY_BREWFILE);
        fs_err::remove_file(machine.data().join("cache/api/formula_names.txt")).expect("removed");
        machine.file(
            "installed.json",
            r#"{"formulae":[{"name":"anything","tap":"homebrew/core"}],"casks":[]}"#,
        );
        let findings = machine.findings();
        assert_eq!(counts(&findings), (0, 0, 1));
        assert!(
            machine
                .summaries()
                .iter()
                .any(|s| s.contains("brew update")),
            "{:?}",
            machine.summaries()
        );
    }

    #[test]
    fn a_brewfile_entry_that_stopped_resolving_is_broken() {
        let machine = Machine::new();
        machine.brewfile(
            "Brewfile",
            "brew \"here\"\nbrew \"gone\"\ncask \"present\"\n",
        );
        machine.file("resolvable", "here\npresent\n");
        machine.roster("cask_names.txt", "gone\n");
        let findings = machine.findings();
        assert_eq!(counts(&findings), (1, 0, 0));
        let finding = findings.first().expect("a finding");
        assert!(finding.summary.as_str().contains("\"gone\""), "{finding:?}");
        assert!(
            finding
                .fix
                .as_ref()
                .is_some_and(|fix| fix.as_str().contains("change the line to cask")),
            "{finding:?}"
        );
    }

    #[test]
    fn a_missing_declared_tap_softens_the_verdict_rather_than_condemning() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", "tap \"someone/theirs\"\nbrew \"gone\"\n");
        machine.file("resolvable", "");
        let findings = machine.findings();
        // One note for the untapped tap, one soft for the ambiguous name.
        assert_eq!(counts(&findings), (0, 1, 1));
        assert!(
            machine
                .summaries()
                .iter()
                .any(|s| s.contains("may be a false alarm")),
            "{:?}",
            machine.summaries()
        );
    }

    #[test]
    fn a_package_installed_on_request_and_declared_nowhere_is_soft() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", "brew \"declared\"\n");
        machine.file("resolvable", "declared\n");
        machine.file("leaves", "declared\nstray\n");
        let findings = machine.findings();
        assert_eq!(counts(&findings), (0, 1, 0));
        assert!(
            machine
                .summaries()
                .iter()
                .any(|s| s.contains("stray is installed on request")),
            "{:?}",
            machine.summaries()
        );
    }

    #[test]
    fn a_recorded_exception_stops_it_resurfacing() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", "brew \"declared\"\n");
        machine.file("resolvable", "declared\n");
        machine.file("leaves", "declared\nstray\n");
        machine.excuse("stray  # benefactor-private, deliberately local\n");
        assert_eq!(machine.summaries(), Vec::<String>::new());
    }

    #[test]
    fn a_tap_prefix_does_not_make_a_name_undeclared() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", "brew \"someone/theirs/tool\"\n");
        machine.file("resolvable", "someone/theirs/tool\n");
        machine.file("leaves", "tool\n");
        assert_eq!(machine.summaries(), Vec::<String>::new());
    }

    #[test]
    fn a_machine_with_no_brewfile_says_so() {
        let machine = Machine::new();
        let findings = machine.findings();
        assert_eq!(counts(&findings), (0, 1, 0));
        assert!(
            machine
                .summaries()
                .first()
                .is_some_and(|s| s.contains("no Brewfile")),
            "{:?}",
            machine.summaries()
        );
    }

    #[test]
    fn a_lock_file_is_not_a_brewfile() {
        let machine = Machine::new();
        machine.brewfile("Brewfile.lock.json", "{}");
        assert!(
            machine
                .summaries()
                .first()
                .is_some_and(|s| s.contains("no Brewfile")),
            "{:?}",
            machine.summaries()
        );
    }

    #[test]
    fn every_scope_is_read_not_only_the_base() {
        let machine = Machine::new();
        machine.brewfile("Brewfile", "brew \"base\"\n");
        machine.brewfile("Brewfile@work", "brew \"scoped\"\n");
        machine.file("resolvable", "base\nscoped\n");
        machine.file("leaves", "base\nscoped\n");
        assert_eq!(machine.summaries(), Vec::<String>::new());
    }
}
