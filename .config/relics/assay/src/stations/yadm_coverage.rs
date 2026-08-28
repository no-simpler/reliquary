//! Paths nobody has decided about.
//!
//! yadm is whitelist-based, and Reliquary runs a second, invisible lane on top
//! of it: the GPG archive. A file that is neither tracked, nor matched by an
//! encrypt pattern, nor deliberately excluded looks exactly like a file nobody
//! has ever looked at — because usually that is what it is.
//!
//! "Should this be tracked?" is a judgment call and always will be. "Has this
//! been decided?" is not: every path under a scanned root is exactly one of
//! plaintext-tracked, encrypt-matched, inside a pruned runtime directory, or
//! declared in `yadm/unmanaged` with a reason. Anything else is undecided, and
//! the judgment happens at first sighting and is then recorded, rather than
//! re-derived every time someone notices the file again.
//!
//! **Archive membership is asked of git, not reimplemented.** yadm expands the
//! encrypt lane with `git --glob-pathspecs ls-files --others --exclude=…`, and
//! so does this — the same question, of the same program, against the same
//! repository. The retired script reimplemented it in Python with `glob` plus a
//! hand-written ancestor test, which is a second matcher that can disagree with
//! the first. It is still offline, side-effect-free and free of Touch ID: the
//! lane is *expanded*, never decrypted, so this is safe in the unattended
//! `yadm doctor` dream pre-pass. Archive-versus-disk drift is a different
//! question and stays `yadm check` / `yadm verify`'s.
//!
//! **The identity rules are `warden`'s test, not a third copy of it.** The hook
//! guards the staged set; this guards two whole sets — every tracked file (the
//! standing full-tree audit the hook shed when it became `warden`) and every
//! undecided one (the same guard run backwards: a hit is positive evidence of
//! which lane the file belongs in). One definition, one matcher, three callers.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context as _, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use regex::Regex;
use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};

use crate::repo::{Encrypt, Rel, Repo};
use crate::station::{Context, Station};

/// The encrypt lane's definition, `$HOME`-relative.
const ENCRYPT: &str = ".config/yadm/encrypt";

/// The paths deliberately kept out of both lanes, with a reason each.
const UNMANAGED: &str = ".config/yadm/unmanaged";

/// Past this a file is generated or vendored, and the content rules are not a
/// scanner for those. Configuration is small.
const MAX_SCAN_BYTES: u64 = 256 * 1024;

/// Where a decision is expected of every path.
const ROOTS: &[&str] = &[".config", ".claude", ".ssh", ".github", ".local/bin"];

/// Regenerable state with no configuration value, pruned whole. Built in rather
/// than declared, because these are tool-owned churn and not decisions anyone
/// makes.
const PRUNE_DIRS: &[&str] = &[
    ".claude/projects",
    ".claude/file-history",
    ".claude/tasks",
    ".claude/plans",
    ".claude/docket",
    ".claude/midden",
    ".claude/jobs",
    ".claude/backups",
    ".claude/sessions",
    ".claude/paste-cache",
    ".claude/telemetry",
    ".claude/shell-snapshots",
    ".claude/daemon",
    ".claude/cache",
    ".claude/debug",
    ".claude/chrome",
    ".claude/plugins",
    ".claude/downloads",
    ".claude/session-env",
    ".config/gcloud",
    ".config/raycast",
    ".config/intelephense/workspace",
    ".config/vim/autoload",
    ".config/vim/plugged",
    ".config/tmux/plugins",
    ".config/zed/embeddings",
    ".config/zed/prompts",
];

/// Names pruned at any depth. The first five are the exclusion block in
/// `yadm/encrypt`; the rest are build and dependency trees, never config.
const PRUNE_NAMES: &[&str] = &[
    "__pycache__",
    ".pytest_cache",
    ".mypy_cache",
    ".ruff_cache",
    ".DS_Store",
    "node_modules",
    "target",
    ".git",
    ".venv",
    "venv",
    ".tox",
    "dist",
    "build",
];

/// Credential shapes, by the format that produced them. The **name** is what a
/// finding reports; the matched text never is, because a report that quotes the
/// secret has moved it somewhere new.
const CREDENTIAL_SHAPES: &[(&str, &str)] = &[
    ("an OpenAI-style secret key", r"sk-[A-Za-z0-9]{20,}"),
    ("a Slack token", r"xox[abcdps]-[A-Za-z0-9-]{10,}"),
    ("a GitHub token", r"gh[pousr]_[A-Za-z0-9]{20,}"),
    (
        "a GitLab personal access token",
        r"glpat-[A-Za-z0-9_-]{15,}",
    ),
    ("a Sentry user token", r"sntryu_[A-Za-z0-9]{20,}"),
    ("an AWS access key id", r"AKIA[0-9A-Z]{16}"),
    ("a Google API key", r"AIza[0-9A-Za-z_-]{30,}"),
    ("a PEM private key", r"-----BEGIN [A-Z ]*PRIVATE KEY"),
];

/// The station.
pub struct YadmCoverage {
    id: StationId,
}

impl Default for YadmCoverage {
    fn default() -> Self {
        Self {
            id: StationId::from_static("yadm-coverage"),
        }
    }
}

impl Station for YadmCoverage {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "every path is tracked, encrypted, pruned or declared — and nothing leaks"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let repo = match Repo::discover(cx) {
            Ok(repo) => repo,
            Err(reason) => return Ok(Outcome::Ran(vec![reason.into_finding(&self.id)])),
        };
        Ok(Outcome::Ran(examine(&self.id, cx, &repo)?))
    }
}

/// The paths this machine has decided to keep out of both lanes.
///
/// The file documents its own semantics — `*` does not cross `/`, `**` does —
/// and that is now what it gets. The retired script matched with `fnmatch`,
/// whose `*` crosses `/` and whose `**` means nothing in particular, so the
/// documented contract and the enforced one had drifted apart.
struct Declared {
    globs: GlobSet,
    /// Entries that are not globs, with the reason each was refused.
    bad: Vec<(String, String)>,
}

impl Declared {
    /// The declarations at `path`, or nothing when the file is absent.
    fn read(path: &Utf8Path) -> Result<Option<Self>> {
        let text = match fs_err::read_to_string(path.as_std_path()) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(anyhow!(error).context(format!("reading {path}"))),
        };
        Self::parse(&text).map(Some)
    }

    fn parse(text: &str) -> Result<Self> {
        let mut builder = GlobSetBuilder::new();
        let mut bad = Vec::new();
        for line in text.lines() {
            if line.trim_start().starts_with('#') || line.trim().is_empty() {
                continue;
            }
            // `<path-or-glob><TAB><reason>`; the reason is for the reader.
            let pattern = match line.split('\t').next() {
                Some(field) => field.trim(),
                None => line.trim(),
            };
            if pattern.is_empty() {
                continue;
            }
            match glob(pattern) {
                Ok(compiled) => {
                    builder.add(compiled);
                }
                Err(error) => bad.push((pattern.to_owned(), error.to_string())),
            }
        }
        Ok(Self {
            globs: builder.build().context("compiling yadm/unmanaged")?,
            bad,
        })
    }

    fn covers(&self, path: &Rel) -> bool {
        self.globs.is_match(path.as_str())
    }
}

/// A glob with the semantics `yadm/unmanaged` documents for itself: `*` stops
/// at a separator, `**` crosses one. The same semantics git gives the encrypt
/// lane under `--glob-pathspecs`, so both data files mean one thing by a star.
fn glob(pattern: &str) -> Result<Glob, globset::Error> {
    GlobBuilder::new(pattern).literal_separator(true).build()
}

// --- The tree ---------------------------------------------------------------

/// Every file a decision is expected about: the scanned roots, plus `$HOME`'s
/// own dotfiles, minus the pruned trees.
///
/// Sockets and fifos are left out. git cannot hold one, so a live agent socket
/// is not a decision anybody can make — it cannot be committed, cannot be
/// archived, and is gone by the next boot.
fn on_disk(home: &Utf8Path) -> Vec<Rel> {
    let mut seen = BTreeSet::new();
    if let Ok(entries) = fs_err::read_dir(home.as_std_path()) {
        for entry in entries.flatten() {
            let Ok(path) = Utf8PathBuf::from_path_buf(entry.path()) else {
                continue;
            };
            let Some(name) = path.file_name() else {
                continue;
            };
            if !name.starts_with('.') || PRUNE_NAMES.contains(&name) {
                continue;
            }
            if entry
                .file_type()
                .is_ok_and(|kind| kind.is_file() || kind.is_symlink())
                && let Some(rel) = Rel::under(home, &path)
            {
                seen.insert(rel);
            }
        }
    }

    for root in ROOTS {
        let base = home.join(root);
        if !base.is_dir() {
            continue;
        }
        let anchor = home.to_owned();
        let walk = ignore::WalkBuilder::new(&base)
            .standard_filters(false)
            .follow_links(false)
            .filter_entry(move |entry| {
                Utf8Path::from_path(entry.path())
                    .and_then(|path| Rel::under(&anchor, path))
                    .is_none_or(|rel| !is_pruned(&rel))
            })
            .build();
        for entry in walk.flatten() {
            if !entry
                .file_type()
                .is_some_and(|kind| kind.is_file() || kind.is_symlink())
            {
                continue;
            }
            if let Some(path) = Utf8Path::from_path(entry.path())
                && let Some(rel) = Rel::under(home, path)
            {
                seen.insert(rel);
            }
        }
    }
    seen.into_iter().collect()
}

/// Whether a path is inside — or is — a pruned tree.
fn is_pruned(rel: &Rel) -> bool {
    if PRUNE_DIRS.contains(&rel.as_str()) {
        return true;
    }
    rel.path()
        .file_name()
        .is_some_and(|name| PRUNE_NAMES.contains(&name))
}

/// A file's text, or nothing when it is oversized, unreadable or not text.
fn text_of(path: &Utf8Path) -> Option<String> {
    let meta = fs_err::metadata(path.as_std_path()).ok()?;
    if !meta.is_file() || meta.len() > MAX_SCAN_BYTES {
        return None;
    }
    let bytes = fs_err::read(path.as_std_path()).ok()?;
    if bytes.contains(&0) {
        return None;
    }
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

// --- The rules --------------------------------------------------------------

/// Everything the station found.
/// Which lane every path is in, once both have been read.
///
/// Assembled once and then only asked questions. The rules below take it whole
/// rather than the four or five sets each would otherwise need, which is what
/// keeps each one a paragraph about its own subject.
struct Lanes {
    /// Plaintext-tracked, as git reports it.
    tracked: BTreeSet<Rel>,
    /// Everything a decision has been made about, either way.
    managed: BTreeSet<Rel>,
    /// Directories holding at least one managed file.
    managed_dirs: BTreeSet<Utf8PathBuf>,
    /// Every path a decision is expected about.
    present: Vec<Rel>,
    /// The ones nobody has made.
    undecided: Vec<Rel>,
    /// What this machine has declared out of both lanes, when it has said.
    declared: Option<Declared>,
    /// The encrypt lane, for the rules that ask about the patterns themselves.
    encrypt: Encrypt,
}

impl Lanes {
    /// Read both lanes, the declarations and the tree.
    ///
    /// `None` when git reports nothing tracked at all, which is not a lane
    /// question — it means there is no repository to answer for.
    fn read(cx: &Context, repo: &Repo) -> Result<Option<Self>> {
        let encrypt = Encrypt::read(&cx.at(ENCRYPT))?;
        let tracked: BTreeSet<Rel> = repo.tracked()?.into_iter().collect();
        if tracked.is_empty() {
            return Ok(None);
        }
        let archived = repo.encrypted(&encrypt)?;
        let managed: BTreeSet<Rel> = tracked.iter().cloned().chain(archived).collect();
        let managed_dirs = managed
            .iter()
            .filter_map(|path| path.path().parent())
            .filter(|dir| !dir.as_str().is_empty())
            .map(Utf8Path::to_owned)
            .collect();
        let declared = Declared::read(&cx.at(UNMANAGED))?;

        let present = on_disk(cx.home());
        let undecided = present
            .iter()
            .filter(|path| {
                !managed.contains(*path)
                    && !declared
                        .as_ref()
                        .is_some_and(|declared| declared.covers(path))
            })
            .cloned()
            .collect();

        Ok(Some(Self {
            tracked,
            managed,
            managed_dirs,
            present,
            undecided,
            declared,
            encrypt,
        }))
    }

    fn is_declared(&self, path: &Rel) -> bool {
        self.declared
            .as_ref()
            .is_some_and(|declared| declared.covers(path))
    }
}

/// Everything the station found, in the order the rules are applied.
fn examine(id: &StationId, cx: &Context, repo: &Repo) -> Result<Vec<Finding>> {
    let Some(lanes) = Lanes::read(cx, repo)? else {
        return Ok(vec![
            id.broken(Summary::lossy(
                "yadm reports no tracked files — the repository is empty or absent",
            ))
            .fixed_by(FixHint::lossy("clone the dotfiles repository into $HOME")),
        ]);
    };

    let mut findings = declarations(id, &lanes);
    findings.extend(both_lanes(id, repo, &lanes)?);
    findings.extend(gone(id, cx.home(), &lanes));
    findings.extend(dead_patterns(id, repo, &lanes)?);
    findings.extend(identity(id, cx, &lanes)?);
    findings.extend(orphans(id, &lanes));
    findings.extend(credentials(id, cx.home(), &lanes));
    findings.extend(bloat(id, repo));
    Ok(findings)
}

/// What the declarations file itself says, before anything is decided with it.
fn declarations(id: &StationId, lanes: &Lanes) -> Vec<Finding> {
    let Some(declared) = &lanes.declared else {
        return vec![
            id.soft(Summary::lossy(
                "yadm/unmanaged is not there, so every undecided path will be reported",
            ))
            .at(Location::file(UNMANAGED))
            .fixed_by(FixHint::lossy(
                "write the file, one <path><TAB><reason> per line",
            )),
        ];
    };
    declared
        .bad
        .iter()
        .map(|(pattern, why)| {
            id.soft(Summary::lossy(&format!(
                "yadm/unmanaged carries a pattern that is not a glob: {pattern}"
            )))
            .detailed_with(Detail::new(why.as_str()))
            .at(Location::file(UNMANAGED))
            .fixed_by(FixHint::lossy("fix the pattern, or drop the line"))
        })
        .collect()
}

/// R1 — both lanes at once. yadm would encrypt a file it also tracks in the
/// clear, so the plaintext copy silently becomes the authoritative one.
fn both_lanes(id: &StationId, repo: &Repo, lanes: &Lanes) -> Result<Vec<Finding>> {
    Ok(repo
        .in_both_lanes(&lanes.encrypt)?
        .into_iter()
        .map(|path| {
            id.broken(Summary::lossy(&format!(
                "{path} is tracked in the clear and matched by an encrypt pattern"
            )))
            .at(path.location())
            .fixed_by(FixHint::lossy(
                "remove it from yadm/encrypt, or `yadm rm --cached` the plaintext copy",
            ))
        })
        .collect())
}

/// R2 — a managed path that is gone. A plaintext one shows as a deletion in
/// `yadm status`; an encrypt-matched one shows up nowhere at all until the next
/// `yadm encrypt` quietly drops it from the archive.
fn gone(id: &StationId, home: &Utf8Path, lanes: &Lanes) -> Vec<Finding> {
    lanes
        .managed
        .iter()
        .filter(|path| fs_err::symlink_metadata(path.absolute(home).as_std_path()).is_err())
        .map(|path| {
            id.broken(Summary::lossy(&format!(
                "{path} is managed and is not on disk"
            )))
            .at(path.location())
            .fixed_by(FixHint::lossy(
                "restore the file, or retire its tracking / encrypt pattern",
            ))
        })
        .collect()
}

/// R3 — a pattern matching nothing: either the file moved, or the pattern was
/// written for a machine this one is not.
fn dead_patterns(id: &StationId, repo: &Repo, lanes: &Lanes) -> Result<Vec<Finding>> {
    let mut findings = Vec::new();
    for pattern in &lanes.encrypt.includes {
        if !repo.pattern_is_live(&lanes.encrypt, pattern)? {
            findings.push(
                id.soft(Summary::lossy(&format!(
                    "an encrypt pattern matches nothing: {pattern}"
                )))
                .at(Location::file(ENCRYPT))
                .fixed_by(FixHint::lossy(
                    "harmless here, and it protects nothing — retire it or fix the path",
                )),
            );
        }
    }
    Ok(findings)
}

/// R5 — undecided under a directory something is managed from.
///
/// The same-directory test misses the common shape: a whole new subtree — a
/// skill, a plugin, a tool's configuration directory — where every file is new.
/// An undecided file under a directory nobody has ever tracked from is a tool's
/// private state, and is not this station's business.
fn orphans(id: &StationId, lanes: &Lanes) -> Vec<Finding> {
    lanes
        .undecided
        .iter()
        .filter(|path| path.ancestors().any(|dir| lanes.managed_dirs.contains(dir)))
        .map(|path| {
            id.soft(Summary::lossy(&format!(
                "{path} is undecided, under a directory something is managed from"
            )))
            .at(path.location())
            .fixed_by(FixHint::lossy(
                "track it, archive it, or declare it in yadm/unmanaged with a reason",
            ))
        })
        .collect()
}

/// R8 — object database bloat. yadm's whitelist means a `yadm add` on a build
/// tree stages thousands of files with nothing to stop it, and resetting the
/// stage leaves every object behind, packed and unreachable, for good.
fn bloat(id: &StationId, repo: &Repo) -> Vec<Finding> {
    let Some(objects) = repo.objects().filter(|objects| objects.is_bloated()) else {
        return Vec::new();
    };
    vec![
        id.soft(Summary::lossy(&format!(
            "the object database is {} MiB and {}% unreachable ({} of {} objects)",
            objects.pack_kib / 1024,
            objects.dead_percent(),
            objects.unreachable(),
            objects.in_pack
        )))
        .fixed_by(FixHint::lossy(
            "yadm reflog expire --expire-unreachable=now --all && yadm gc --prune=now",
        )),
    ]
}

/// The identity rules: `warden`'s test over two whole sets.
///
/// Over the **tracked** set this is the standing full-tree audit the commit
/// guard shed when it narrowed to the staged set — the hook answers for what is
/// being committed, and something has to answer for what already is. Over the
/// **undecided** set it is the same guard run backwards: a hit is not a defect
/// in the file, it is positive evidence of which lane the file belongs in.
fn identity(id: &StationId, cx: &Context, lanes: &Lanes) -> Result<Vec<Finding>> {
    let home = cx.home();
    let definition = match warden::Definition::discover(home) {
        Ok(definition) => definition,
        Err(warden::definition::Error::Absent(path)) => {
            // Home-relative, like every other location this station reports.
            // An absolute path in default output is machine-specific text in a
            // report meant to be comparable between runs and between machines.
            let at =
                Rel::under(home, &path).map_or_else(|| Location::file(path), |rel| rel.location());
            return Ok(vec![
                id.note(Summary::lossy(
                    "the identity guard is not decrypted, so the identity rules did not run",
                ))
                .at(at)
                .fixed_by(FixHint::lossy("yadm decrypt")),
            ]);
        }
        Err(
            error @ (warden::definition::Error::Unreadable(..)
            | warden::definition::Error::Malformed(..)
            | warden::definition::Error::Empty(..)
            | warden::definition::Error::Uncompilable(..)),
        ) => {
            return Ok(vec![
                id.broken(Summary::lossy(&format!(
                    "the identity guard is unusable, so nothing is testing for private content: {error}"
                )))
                .fixed_by(FixHint::lossy("repair yadm/hooks/identity-guard.toml")),
            ]);
        }
    };
    let config = warden::Config::discover(home).context("the guard's configuration")?;

    let mut findings = Vec::new();

    // The full-tree sweep. Unreadable content is refused rather than skipped,
    // exactly as the hook refuses it: the point is that what nothing can vouch
    // for does not sit in a public tree.
    for path in &lanes.tracked {
        if config.allows_binary(path.path()) {
            continue;
        }
        let Ok(bytes) = fs_err::read(path.absolute(home).as_std_path()) else {
            continue;
        };
        let hits = warden::scan::file(path.path(), &bytes, &definition);
        if hits.is_empty() {
            continue;
        }
        // The matched line and the matched term are deliberately not reported.
        // A finding that quotes what it found has moved the private content
        // somewhere new, and the path is enough to go and look.
        findings.push(
            id.broken(Summary::lossy(&format!(
                "{path} is tracked in the clear and the identity guard refuses it"
            )))
            .detailed_with(Detail::new(reasons(&hits)))
            .at(path.location())
            .fixed_by(FixHint::lossy(
                "move it into the encrypt lane, or take the content out",
            )),
        );
    }

    // R4 — private content sitting outside both lanes. Here an unreadable file
    // is a skip, not a refusal: nothing is proposing to commit it, and the
    // question is only which lane it belongs in.
    for path in &lanes.undecided {
        let Some(text) = text_of(&path.absolute(home)) else {
            continue;
        };
        if !definition.matches(&text) {
            continue;
        }
        findings.push(
            id.broken(Summary::lossy(&format!(
                "{path} is undecided and carries identifying content"
            )))
            .at(path.location())
            .fixed_by(FixHint::lossy(
                "it can never be plaintext-tracked: archive it, or declare it in yadm/unmanaged",
            )),
        );
    }

    Ok(findings)
}

/// Why the guard refused, without repeating what it found.
fn reasons(hits: &[warden::Finding]) -> String {
    let mut kinds: BTreeMap<&str, usize> = BTreeMap::new();
    for hit in hits {
        let kind = match hit {
            warden::Finding::Term { .. } => "a term from the definition",
            warden::Finding::Characters { .. } => "the definition's character class",
            warden::Finding::Unreadable { .. } => "content nothing can vouch for",
        };
        *kinds.entry(kind).or_default() += 1;
    }
    kinds
        .into_iter()
        .map(|(kind, count)| format!("{kind} ({count})"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// R6 and R7: a credential shape in a tracked file, or in an undeclared one.
fn credentials(id: &StationId, home: &Utf8Path, lanes: &Lanes) -> Vec<Finding> {
    let shapes: Vec<(&str, Regex)> = CREDENTIAL_SHAPES
        .iter()
        .filter_map(|(name, pattern)| Regex::new(pattern).ok().map(|regex| (*name, regex)))
        .collect();

    let mut findings = Vec::new();
    for path in &lanes.present {
        let is_tracked = lanes.tracked.contains(path);
        // A declared path is a decision already made, unless it is also
        // tracked — in which case the decision and the tracking disagree and
        // the content is in the repository regardless.
        if lanes.is_declared(path) && !is_tracked {
            continue;
        }
        let Some(text) = text_of(&path.absolute(home)) else {
            continue;
        };
        let found: Vec<&str> = shapes
            .iter()
            .filter(|(_, regex)| regex.is_match(&text))
            .map(|(name, _)| *name)
            .collect();
        if found.is_empty() {
            continue;
        }
        let shapes_found = found.join(", ");
        findings.push(if is_tracked {
            id.broken(Summary::lossy(&format!(
                "{path} is tracked in the clear and holds {shapes_found}"
            )))
            .at(path.location())
            .fixed_by(FixHint::lossy(
                "rotate the credential, then move the file into the encrypt lane",
            ))
        } else {
            id.soft(Summary::lossy(&format!(
                "{path} holds {shapes_found} and nothing has decided about it"
            )))
            .at(path.location())
            .fixed_by(FixHint::lossy(
                "fine to keep out of the repo — declare it in yadm/unmanaged so the decision is recorded",
            ))
        });
    }
    findings
}

#[cfg(test)]
mod tests {
    use relic_core::finding::Severity;

    use super::*;
    use crate::repo::WRAPPER_DIR;
    use crate::repo::testing::Machine;
    use std::os::unix::fs::PermissionsExt as _;

    impl Machine {
        fn encrypt(&self, body: &str) {
            self.write(ENCRYPT, body);
        }

        fn unmanaged(&self, body: &str) {
            self.write(UNMANAGED, body);
        }

        fn guard(&self, body: &str) {
            self.write(".config/yadm/hooks/identity-guard.toml", body);
        }
    }

    fn findings(machine: &Machine) -> Vec<Finding> {
        machine.findings_of(&YadmCoverage::default())
    }

    fn about(machine: &Machine, needle: &str) -> Vec<Finding> {
        machine.about_of(&YadmCoverage::default(), needle)
    }

    /// A machine with the lanes defined and nothing interesting in them.
    fn quiet() -> Machine {
        let machine = Machine::new();
        machine.encrypt("# nothing encrypted here\n");
        machine.unmanaged("# nothing declared here\n");
        machine.write(".config/kept.txt", "kept\n");
        machine.track(".config/kept.txt");
        machine
    }

    // --- The data files -----------------------------------------------------
    #[test]
    fn a_declared_star_stops_at_a_separator_and_a_double_star_does_not() {
        let declared =
            Declared::parse(".config/one/*\treason\n.config/two/**\treason\n").expect("compiled");
        assert!(declared.covers(&Rel::new(".config/one/a")));
        assert!(
            !declared.covers(&Rel::new(".config/one/a/b")),
            "the file documents that `*` does not cross `/`, so it must not"
        );
        assert!(declared.covers(&Rel::new(".config/two/a/b")));
    }

    #[test]
    fn a_declaration_is_its_first_field_and_a_comment_is_no_declaration() {
        let declared = Declared::parse("# .config/commented\n\n.config/real\treason with\ttabs\n")
            .expect("compiled");
        assert!(declared.covers(&Rel::new(".config/real")));
        assert!(!declared.covers(&Rel::new(".config/commented")));
        assert!(declared.bad.is_empty());
    }

    #[test]
    fn a_declaration_that_is_not_a_glob_is_reported_rather_than_thrown() {
        let declared = Declared::parse(".config/[unclosed\treason\n").expect("compiled");
        assert_eq!(declared.bad.len(), 1);
        assert!(!declared.covers(&Rel::new(".config/[unclosed")));
    }

    // --- Reaching the repository -------------------------------------------

    #[test]
    fn no_yadm_on_the_path_is_broken_and_never_a_silent_pass() {
        let machine = quiet();
        fs_err::remove_file(machine.bin.join("yadm")).expect("removed");
        let findings = findings(&machine);
        assert_eq!(findings.len(), 1);
        let finding = findings.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("yadm is not on PATH"));
    }

    #[test]
    fn the_wrapper_is_stepped_over_when_something_else_on_the_path_has_yadm() {
        let machine = quiet();
        // The wrapper's own directory, first on the path and answering wrongly.
        let wrapper = machine.home.join(WRAPPER_DIR);
        fs_err::create_dir_all(&wrapper).expect("the wrapper directory");
        let shim = wrapper.join("yadm");
        fs_err::write(&shim, "#!/bin/sh\necho /nowhere\n").expect("written");
        fs_err::set_permissions(&shim, std::fs::Permissions::from_mode(0o755)).expect("chmod");

        let cx = Context::new(machine.home.clone(), vec![wrapper, machine.bin.clone()]);
        let repo = Repo::discover(&cx).expect("the real yadm answered");
        assert_eq!(repo.dir, machine.repo);
    }

    // --- The rules ----------------------------------------------------------

    #[test]
    fn a_path_in_both_lanes_is_broken() {
        let machine = quiet();
        machine.write(".config/double.txt", "in both\n");
        machine.track(".config/double.txt");
        machine.encrypt(".config/double.txt\n");

        let found = about(
            &machine,
            "tracked in the clear and matched by an encrypt pattern",
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        let finding = found.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Broken);
        assert_eq!(
            finding.location.as_ref().map(|at| at.path.as_str()),
            Some(".config/double.txt")
        );
    }

    #[test]
    fn a_managed_path_that_is_gone_is_broken() {
        let machine = quiet();
        machine.write(".config/vanished.txt", "here for now\n");
        machine.track(".config/vanished.txt");
        fs_err::remove_file(machine.home.join(".config/vanished.txt")).expect("removed");

        let found = about(&machine, "is managed and is not on disk");
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(
            found.first().map(|finding| finding.severity),
            Some(Severity::Broken)
        );
    }

    #[test]
    fn an_encrypt_pattern_matching_nothing_is_soft() {
        let machine = quiet();
        machine.write(".config/present.txt", "here\n");
        machine.encrypt(".config/present.txt\n.config/absent.txt\n");

        let found = about(&machine, "an encrypt pattern matches nothing");
        assert_eq!(found.len(), 1, "{found:#?}");
        let finding = found.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Soft);
        assert!(finding.summary.as_str().ends_with(".config/absent.txt"));
    }

    #[test]
    fn an_encrypt_lane_that_names_nothing_sweeps_in_nothing() {
        let machine = quiet();
        machine.write(".config/stray.txt", "untracked\n");
        machine.encrypt("# every line here is a comment\n");

        // git reads an empty pathspec list as *everything*, so the failure this
        // pins is silent and total: every untracked path reported as archived,
        // every tracked path reported as in both lanes, and a clean bill.
        assert!(about(&machine, "matched by an encrypt pattern").is_empty());
        assert_eq!(
            about(&machine, ".config/stray.txt is undecided").len(),
            1,
            "a lane that names nothing archives nothing, so this is still undecided"
        );
    }

    #[test]
    fn a_pattern_matching_only_a_tracked_file_is_live_not_dead() {
        let machine = quiet();
        machine.write(".config/both.txt", "here\n");
        machine.track(".config/both.txt");
        machine.encrypt(".config/both.txt\n");

        assert!(
            about(&machine, "an encrypt pattern matches nothing").is_empty(),
            "`--others` is untracked-only, so a tracked match must be asked for separately"
        );
    }

    #[test]
    fn an_undecided_file_under_a_managed_directory_is_soft() {
        let machine = quiet();
        machine.write(".config/stray.txt", "nobody decided\n");

        let found = about(&machine, ".config/stray.txt is undecided");
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(
            found.first().map(|finding| finding.severity),
            Some(Severity::Soft)
        );
    }

    #[test]
    fn a_whole_new_subtree_is_caught_and_not_only_a_stray_beside_a_tracked_file() {
        let machine = quiet();
        machine.write(".config/newskill/deep/inside.md", "all new\n");

        assert_eq!(
            about(&machine, ".config/newskill/deep/inside.md is undecided").len(),
            1,
            "an ancestor holding a managed file is what makes this a decision half-made"
        );
    }

    #[test]
    fn a_declared_path_is_a_decision_already_made() {
        let machine = quiet();
        machine.write(".config/stray.txt", "declared\n");
        machine.unmanaged(".config/stray.txt\tregenerated by its own tool\n");

        assert!(about(&machine, ".config/stray.txt is undecided").is_empty());
    }

    #[test]
    fn a_pruned_tree_is_never_a_decision() {
        let machine = quiet();
        machine.write(".config/relics/target/debug/huge.bin", "build output\n");
        machine.write(".config/gcloud/credentials.db", "regenerable\n");

        let findings = findings(&machine);
        assert!(
            !findings
                .iter()
                .any(|finding| finding.summary.as_str().contains("target/debug")),
            "{findings:#?}"
        );
        assert!(
            !findings
                .iter()
                .any(|finding| finding.summary.as_str().contains("gcloud")),
            "{findings:#?}"
        );
    }

    #[test]
    fn a_missing_unmanaged_file_says_so_rather_than_reporting_everything_silently() {
        let machine = Machine::new();
        machine.encrypt("# nothing\n");
        machine.write(".config/kept.txt", "kept\n");
        machine.track(".config/kept.txt");

        let found = about(&machine, "yadm/unmanaged is not there");
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(
            found.first().map(|finding| finding.severity),
            Some(Severity::Soft)
        );
    }

    #[test]
    fn an_undefined_encrypt_lane_stops_the_station_rather_than_reporting_a_clean_machine() {
        let machine = Machine::new();
        machine.write(".config/kept.txt", "kept\n");
        machine.track(".config/kept.txt");

        let station = YadmCoverage::default();
        let error = station
            .check(&machine.context())
            .expect_err("a lane that cannot be read is not a clean machine");
        assert!(
            error.to_string().contains("the encrypt lane is undefined"),
            "{error:#}"
        );
    }

    // --- The content rules --------------------------------------------------

    #[test]
    fn a_credential_in_a_tracked_file_is_broken_and_the_secret_is_never_quoted() {
        let machine = quiet();
        // Assembled rather than written, so this file does not itself carry
        // the shape it is testing for. A scanner whose fixtures trip it teaches
        // the reader that its own findings are noise — and this one caught the
        // first draft of this very test.
        let secret = format!("{}{}", "ghp_", "0123456789abcdefghijklmnopqrstuvwx");
        machine.write(".config/leaky.toml", &format!("token = \"{secret}\"\n"));
        machine.track(".config/leaky.toml");

        let found = about(&machine, "holds a GitHub token");
        assert_eq!(found.len(), 1, "{found:#?}");
        let finding = found.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Broken);
        let rendered = format!("{finding:?}");
        assert!(
            !rendered.contains(&secret),
            "a report that quotes the secret has moved it somewhere new"
        );
    }

    #[test]
    fn a_credential_in_an_undeclared_file_is_soft() {
        let machine = quiet();
        machine.write(
            ".config/loose.toml",
            &format!("token = \"{}{}\"\n", "glpat-", "0123456789abcdefghi"),
        );

        let found = about(&machine, "nothing has decided about it");
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(
            found.first().map(|finding| finding.severity),
            Some(Severity::Soft)
        );
    }

    #[test]
    fn a_declared_credential_file_is_left_alone_unless_it_is_also_tracked() {
        let machine = quiet();
        machine.write(
            ".config/creds",
            &format!("token = \"{}{}\"\n", "glpat-", "0123456789abcdefghi"),
        );
        machine.unmanaged(".config/creds\tlive token, re-issued on demand\n");
        assert!(about(&machine, "GitLab").is_empty());

        machine.track(".config/creds");
        let found = about(&machine, "a GitLab personal access token");
        assert_eq!(
            found.len(),
            1,
            "declaring it out of the repo cannot help once it is in the repo"
        );
    }

    #[test]
    fn a_file_too_large_to_be_configuration_is_not_scanned() {
        let machine = quiet();
        let padding = "x".repeat(usize::try_from(MAX_SCAN_BYTES).expect("fits") + 1);
        machine.write(
            ".config/huge.log",
            &format!(
                "{padding}\n{}{}\n",
                "ghp_", "0123456789abcdefghijklmnopqrstuvwx"
            ),
        );
        assert!(about(&machine, "GitHub token").is_empty());
    }

    // --- The identity rules -------------------------------------------------

    #[test]
    fn an_undecrypted_identity_guard_is_a_note_and_never_a_pass() {
        let machine = quiet();
        let found = about(&machine, "identity guard is not decrypted");
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(
            found.first().map(|finding| finding.severity),
            Some(Severity::Note),
            "a check nobody can clear where it fires must not gate"
        );
    }

    #[test]
    fn a_guard_that_is_there_and_unusable_is_broken_not_a_note() {
        let machine = quiet();
        machine.guard("[guard]\nkeywords = []\ncharacter-class = \"\"\n");

        let found = about(&machine, "identity guard is unusable");
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(
            found.first().map(|finding| finding.severity),
            Some(Severity::Broken),
            "an empty definition would pass every file while reporting that it checked"
        );
    }

    #[test]
    fn a_tracked_file_carrying_an_identifying_term_is_broken() {
        let machine = quiet();
        machine.guard("[guard]\nkeywords = [\"widgetcorp\"]\n");
        machine.write(".config/leaks.md", "belongs to WidgetCorp\n");
        machine.track(".config/leaks.md");

        let found = about(&machine, ".config/leaks.md is tracked in the clear");
        assert_eq!(found.len(), 1, "{found:#?}");
        let finding = found.first().expect("a finding");
        assert_eq!(finding.severity, Severity::Broken);
        let detail = finding.detail.as_ref().expect("the evidence");
        assert!(detail.as_str().contains("a term from the definition"));
        assert!(
            !detail.as_str().contains("widgetcorp"),
            "the term is what the definition protects; a finding that repeats it publishes it"
        );
    }

    #[test]
    fn an_undecided_file_carrying_identifying_content_is_broken() {
        let machine = quiet();
        machine.guard("[guard]\nkeywords = [\"widgetcorp\"]\n");
        machine.write(".config/loose.md", "belongs to WidgetCorp\n");

        let found = about(
            &machine,
            ".config/loose.md is undecided and carries identifying content",
        );
        assert_eq!(found.len(), 1, "{found:#?}");
        assert_eq!(
            found.first().map(|finding| finding.severity),
            Some(Severity::Broken)
        );
    }

    #[test]
    fn a_keyword_is_a_word_and_never_an_expression() {
        let machine = quiet();
        machine.guard("[guard]\nkeywords = [\"a.c\"]\n");
        machine.write(".config/near.md", "abc\n");
        machine.track(".config/near.md");

        assert!(
            about(&machine, ".config/near.md is tracked in the clear").is_empty(),
            "a term with a dot is a term, not a pattern matching any character"
        );
    }

    #[test]
    fn a_tracked_binary_is_refused_unless_the_machine_has_allowed_it() {
        let machine = quiet();
        machine.guard("[guard]\nkeywords = [\"widgetcorp\"]\n");
        // Genuinely not text. A NUL byte would not do it: NUL is valid UTF-8,
        // and `warden` asks whether the bytes decode, not whether they are
        // printable.
        machine.write_bytes(".config/blob.bin", &[0xff, 0xfe, 0x00, 0x80]);
        machine.track(".config/blob.bin");

        assert_eq!(
            about(&machine, ".config/blob.bin is tracked in the clear").len(),
            1,
            "nothing can vouch for content the guard cannot read"
        );

        machine.write(
            ".config/warden/config.toml",
            "[warden]\nbinary-allowed = [\".config/blob.bin\"]\n",
        );
        assert!(
            about(&machine, ".config/blob.bin is tracked in the clear").is_empty(),
            "an allowed binary is a decision this machine recorded"
        );
    }
}
