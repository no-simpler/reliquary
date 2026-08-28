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
use std::fmt;

use anyhow::{Context as _, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use globset::{Glob, GlobBuilder, GlobSet, GlobSetBuilder};
use regex::Regex;
use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};
use relic_core::git::{self, Git};
use relic_core::tool::Tool;

use crate::station::{Context, Station};

/// The encrypt lane's definition, `$HOME`-relative.
const ENCRYPT: &str = ".config/yadm/encrypt";

/// The paths deliberately kept out of both lanes, with a reason each.
const UNMANAGED: &str = ".config/yadm/unmanaged";

/// Where the yadm wrapper lives. A `yadm` resolved here would print its own
/// archive-check banner into the middle of this station's answer.
const WRAPPER_DIR: &str = ".config/bin";

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

/// Past this a file is generated or vendored, and the content rules are not a
/// scanner for those. Configuration is small.
const MAX_SCAN_BYTES: u64 = 256 * 1024;

/// The pack size below which object-database shape is not worth a word.
const PACK_FLOOR_KIB: u64 = 50 * 1024;

/// Fewer unreachable objects than this is ordinary garbage, not a mistake.
const UNREACHABLE_FLOOR: u64 = 1000;

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

// --- Reaching the repository ------------------------------------------------

/// Why the station could not read the lanes. Every one of these means the
/// machine cannot be verified, which is a `Broken` finding rather than an
/// `Err`: the station itself is working fine, and it has something true to say.
#[derive(Debug)]
enum Unreachable {
    /// git is not on `PATH`.
    NoGit,
    /// yadm is not on `PATH` outside the wrapper's directory.
    NoYadm,
    /// yadm would not say where its repository is.
    NoRepo(String),
    /// The repository yadm named is not there.
    Missing(Utf8PathBuf),
}

impl Unreachable {
    fn into_finding(self, id: &StationId) -> Finding {
        let (summary, fix) = match self {
            Self::NoGit => (
                "git is not on PATH, so neither lane can be read".to_owned(),
                "install git — it is bedrock",
            ),
            Self::NoYadm => (
                "yadm is not on PATH outside the wrapper, so the repository cannot be located"
                    .to_owned(),
                "install yadm, or check that ~/.config/bin is not the only entry that has it",
            ),
            Self::NoRepo(why) => (
                format!("yadm could not say where its repository is: {why}"),
                "run `yadm status` and see what it reports",
            ),
            Self::Missing(dir) => (
                format!("yadm names {dir} as its repository, and it is not there"),
                "clone the dotfiles repository, or repair ~/.config/yadm",
            ),
        };
        id.broken(Summary::lossy(&summary))
            .fixed_by(FixHint::lossy(fix))
    }
}

/// The dotfiles repository, and the questions this station asks git about it.
///
/// yadm's own `parse_encrypt` runs `git --glob-pathspecs ls-files --others
/// --exclude=…` against a named git directory over `$HOME`. So does this, which
/// is why the two can never disagree about what the archive holds.
struct Repo {
    git: Git,
    /// The git directory, as yadm reports it.
    dir: Utf8PathBuf,
    /// The work tree, which is the home directory.
    home: Utf8PathBuf,
}

impl Repo {
    /// Locate the repository by asking yadm, and prove git can be run.
    fn discover(cx: &Context) -> Result<Self, Unreachable> {
        let git = git::detect().ok_or(Unreachable::NoGit)?;
        let yadm = real_yadm(cx).ok_or(Unreachable::NoYadm)?;
        let mut command = yadm.command();
        command.args(["introspect", "repo"]);
        let answer = yadm
            .capture(&mut command)
            .map_err(|error| Unreachable::NoRepo(error.to_string()))?;
        let dir = Utf8PathBuf::from(answer.line());
        if dir.as_str().is_empty() {
            return Err(Unreachable::NoRepo("it named nothing".to_owned()));
        }
        if !dir.is_dir() {
            return Err(Unreachable::Missing(dir));
        }
        Ok(Self {
            git,
            dir,
            home: cx.home().to_owned(),
        })
    }

    /// One `ls-files` question, answered as home-relative paths.
    ///
    /// `-z` because a path may hold a newline: the retired script split on
    /// them, which is the same class of defect as the unquoted expansion the
    /// commit guard carried.
    fn ls_files(&self, lane: Lane, scope: Scope<'_>) -> Result<Vec<Rel>> {
        let Some((excludes, patterns)) = scope.args() else {
            return Ok(Vec::new());
        };
        let mut command = self.git.command();
        command
            .arg("-C")
            .arg(&self.home)
            .arg(format!("--git-dir={}", self.dir))
            .arg(format!("--work-tree={}", self.home))
            .arg("--glob-pathspecs")
            .arg("ls-files")
            .arg("-z")
            .args(lane.args())
            .args(excludes);
        if !patterns.is_empty() {
            command.arg("--").args(&patterns);
        }
        let answer = self
            .git
            .capture(&mut command)
            .with_context(|| format!("asking git about {}", self.dir))?;
        Ok(answer
            .stdout
            .split('\0')
            .filter(|entry| !entry.is_empty())
            // yadm trims the same trailing separator: a pathspec can name a
            // directory, and a directory is not a decision on its own.
            .map(|entry| Rel::new(entry.trim_end_matches('/')))
            .collect())
    }

    /// Everything tracked in the clear.
    fn tracked(&self) -> Result<Vec<Rel>> {
        self.ls_files(Lane::Tracked, Scope::Everything)
    }

    /// Everything the encrypt lane would sweep in — yadm's own query.
    fn encrypted(&self, encrypt: &Encrypt) -> Result<Vec<Rel>> {
        self.ls_files(Lane::Untracked, Scope::Encrypt(encrypt))
    }

    /// Everything matched by the encrypt lane that is *also* tracked in the
    /// clear. yadm asks this separately for the same reason: `--others` is
    /// untracked-only, so the overlap is invisible to the first query.
    fn in_both_lanes(&self, encrypt: &Encrypt) -> Result<Vec<Rel>> {
        self.ls_files(Lane::Tracked, Scope::Encrypt(encrypt))
    }

    /// Whether one pattern matches anything at all, in either lane.
    fn pattern_is_live(&self, encrypt: &Encrypt, pattern: &str) -> Result<bool> {
        let scope = Scope::OnePattern(encrypt, pattern);
        if !self.ls_files(Lane::Untracked, scope)?.is_empty() {
            return Ok(true);
        }
        // Only reached for a pattern with no untracked matches, so the second
        // fork is paid on the rare path rather than on every pattern.
        Ok(!self.ls_files(Lane::Tracked, scope)?.is_empty())
    }

    /// The object database's shape, or nothing when git would not say.
    fn objects(&self) -> Option<Objects> {
        let counted = self.plumbing(&["count-objects", "-v"])?;
        let mut in_pack = None;
        let mut pack_kib = None;
        for line in counted.lines() {
            if let Some(rest) = line.strip_prefix("in-pack: ") {
                in_pack = rest.trim().parse::<u64>().ok();
            } else if let Some(rest) = line.strip_prefix("size-pack: ") {
                pack_kib = rest.trim().parse::<u64>().ok();
            }
        }
        let listed = self.plumbing(&["rev-list", "--objects", "--all"])?;
        Some(Objects {
            in_pack: in_pack?,
            pack_kib: pack_kib?,
            reachable: u64::try_from(listed.lines().count()).ok()?,
        })
    }

    /// A plumbing question whose failure is a fact, not a finding: a repository
    /// with no commits cannot answer `rev-list`, and that is not a defect.
    fn plumbing(&self, args: &[&str]) -> Option<String> {
        let mut command = self.git.command();
        command.arg("--git-dir").arg(&self.dir).args(args);
        self.git.capture(&mut command).ok().map(|out| out.stdout)
    }
}

/// Which half of the work tree a question is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Lane {
    /// What the index holds.
    Tracked,
    /// What it does not — yadm's own `--others`, deliberately without
    /// `--exclude-standard`, because `.gitignore` does not protect the archive.
    Untracked,
}

impl Lane {
    fn args(self) -> &'static [&'static str] {
        match self {
            Self::Tracked => &[],
            Self::Untracked => &["--others"],
        }
    }
}

/// Which paths a question is about.
///
/// An enum rather than a pathspec list, because **git reads an empty pathspec
/// list as "everything"** — so an encrypt lane that names nothing would sweep in
/// the whole work tree and report every path as managed. That is the same shape
/// as the empty regex that would have matched every line in the identity guard:
/// a definition selecting nothing must select nothing. Here it cannot be spelled
/// wrongly, because a caller names the scope rather than assembling the list.
#[derive(Clone, Copy)]
enum Scope<'a> {
    /// Every path in the lane, with no pathspec at all.
    Everything,
    /// What the encrypt lane names, and nothing when it names nothing.
    Encrypt(&'a Encrypt),
    /// One of its patterns.
    OnePattern(&'a Encrypt, &'a str),
}

impl<'a> Scope<'a> {
    /// The exclusions and pathspecs, or `None` when the scope selects nothing.
    fn args(self) -> Option<(&'a [String], Vec<&'a str>)> {
        match self {
            Self::Everything => Some((&[], Vec::new())),
            Self::Encrypt(encrypt) => (!encrypt.includes.is_empty()).then(|| {
                (
                    encrypt.excludes.as_slice(),
                    encrypt.includes.iter().map(String::as_str).collect(),
                )
            }),
            Self::OnePattern(encrypt, pattern) => {
                Some((encrypt.excludes.as_slice(), vec![pattern]))
            }
        }
    }
}

/// yadm, resolved past the wrapper.
///
/// `~/.config/bin/yadm` is a symlink to `yadm-wrapper`, which sits ahead of
/// Homebrew on `PATH` and prints an archive check after every command it passes
/// through. The retired script scanned `PATH` the same way and for the same
/// reason.
fn real_yadm(cx: &Context) -> Option<Tool> {
    let wrapper = cx.at(WRAPPER_DIR);
    crate::probe::resolve_all("yadm", cx.path())
        .into_iter()
        .find(|hit| hit.parent() != Some(wrapper.as_path()))
        .map(|program| Tool::at_path("yadm", program.into_std_path_buf()))
}

// --- A path, as every lane spells it ---------------------------------------

/// A path relative to the home directory.
///
/// The only form any lane speaks: git answers relative to the work tree, both
/// data files are written that way, and a finding names one. Absolute paths
/// exist for exactly as long as it takes to read a file.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
struct Rel(Utf8PathBuf);

impl Rel {
    fn new(spelled: &str) -> Self {
        Self(Utf8PathBuf::from(spelled))
    }

    /// The path under `home`, or nothing when it is not under it.
    fn under(home: &Utf8Path, absolute: &Utf8Path) -> Option<Self> {
        absolute
            .strip_prefix(home)
            .ok()
            .and_then(|rest| (!rest.as_str().is_empty()).then(|| Self(rest.to_owned())))
    }

    fn as_str(&self) -> &str {
        self.0.as_str()
    }

    fn absolute(&self, home: &Utf8Path) -> Utf8PathBuf {
        home.join(&self.0)
    }

    /// Every directory above it, nearest first.
    fn ancestors(&self) -> impl Iterator<Item = &Utf8Path> {
        self.0
            .ancestors()
            .skip(1)
            .filter(|dir| !dir.as_str().is_empty())
    }

    fn location(&self) -> Location {
        Location::file(self.0.clone())
    }
}

impl fmt::Display for Rel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.as_str())
    }
}

// --- The two data files -----------------------------------------------------

/// The encrypt lane, parsed exactly as yadm parses it.
///
/// Deliberately without trimming: yadm reads raw lines, so a pattern's trailing
/// space is part of the pattern, and a checker that trims would report a lane
/// yadm does not have.
#[derive(Debug, Default, PartialEq, Eq)]
struct Encrypt {
    /// Pathspecs, in file order.
    includes: Vec<String>,
    /// Exclusions, already in the `--exclude=/…` form git wants.
    excludes: Vec<String>,
}

impl Encrypt {
    fn read(path: &Utf8Path) -> Result<Self> {
        let text = fs_err::read_to_string(path.as_std_path())
            .with_context(|| format!("reading {path} — the encrypt lane is undefined"))?;
        Ok(Self::parse(&text))
    }

    fn parse(text: &str) -> Self {
        let mut policy = Self::default();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix('!') {
                policy.excludes.push(format!("--exclude=/{rest}"));
            } else if !line.trim_start().is_empty() && !line.trim_start().starts_with('#') {
                policy.includes.push(line.to_owned());
            }
        }
        policy
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

// --- What the object database looks like ------------------------------------

/// The shape of the object database, in the two numbers that matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Objects {
    in_pack: u64,
    pack_kib: u64,
    reachable: u64,
}

impl Objects {
    /// Objects packed but reachable from nothing.
    fn unreachable(self) -> u64 {
        self.in_pack.saturating_sub(self.reachable)
    }

    /// Whether the database is big enough, and dead enough, to be worth saying.
    fn is_bloated(self) -> bool {
        let dead = self.unreachable();
        self.in_pack > 0
            && self.reachable > 0
            && self.pack_kib > PACK_FLOOR_KIB
            && dead > UNREACHABLE_FLOOR
            && dead.saturating_mul(5) > self.in_pack
    }

    /// How much of the database is dead, as whole percent.
    fn dead_percent(self) -> u64 {
        if self.in_pack == 0 {
            return 0;
        }
        self.unreachable().saturating_mul(100) / self.in_pack
    }
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
    rel.0
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
            .filter_map(|path| path.0.parent())
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
        if config.allows_binary(&path.0) {
            continue;
        }
        let Ok(bytes) = fs_err::read(path.absolute(home).as_std_path()) else {
            continue;
        };
        let hits = warden::scan::file(&path.0, &bytes, &definition);
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
    use std::os::unix::fs::PermissionsExt as _;
    use std::process::Command;

    use relic_core::finding::Severity;

    use super::*;

    /// A machine built from nothing: a home, a yadm repository over it, and a
    /// `yadm` on the injected search path that answers where that repository
    /// is. Everything the station reads is a file this writes, so no test
    /// answers for the machine it runs on.
    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
        bin: Utf8PathBuf,
        repo: Utf8PathBuf,
    }

    impl Machine {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let root =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            let home = root.join("home");
            let bin = root.join("bin");
            let repo = root.join("repo.git");
            fs_err::create_dir_all(&home).expect("home");
            fs_err::create_dir_all(&bin).expect("bin");

            run_git(&["init", "--bare", "--quiet", repo.as_str()]);
            run_git(&["--git-dir", repo.as_str(), "config", "core.bare", "false"]);

            let machine = Self {
                _dir: dir,
                home,
                bin,
                repo,
            };
            machine.executable(
                "yadm",
                &format!(
                    "#!/bin/sh\nif [ \"$1\" = introspect ] && [ \"$2\" = repo ]; then\n  echo {}\nfi\n",
                    machine.repo
                ),
            );
            machine
        }

        fn write(&self, rel: &str, body: &str) -> Utf8PathBuf {
            let path = self.home.join(rel);
            if let Some(parent) = path.parent() {
                fs_err::create_dir_all(parent).expect("a parent directory");
            }
            fs_err::write(&path, body).expect("written");
            path
        }

        fn write_bytes(&self, rel: &str, body: &[u8]) {
            let path = self.home.join(rel);
            if let Some(parent) = path.parent() {
                fs_err::create_dir_all(parent).expect("a parent directory");
            }
            fs_err::write(&path, body).expect("written");
        }

        fn executable(&self, name: &str, body: &str) {
            let path = self.bin.join(name);
            fs_err::write(&path, body).expect("written");
            fs_err::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("made executable");
        }

        /// Stages a path, which is all `ls-files` needs — the index is the
        /// tracked set, and a commit would only add a signature prompt.
        fn track(&self, rel: &str) {
            run_git(&[
                "-C",
                self.home.as_str(),
                "--git-dir",
                self.repo.as_str(),
                "--work-tree",
                self.home.as_str(),
                "add",
                "--force",
                rel,
            ]);
        }

        fn encrypt(&self, body: &str) {
            self.write(ENCRYPT, body);
        }

        fn unmanaged(&self, body: &str) {
            self.write(UNMANAGED, body);
        }

        fn guard(&self, body: &str) {
            self.write(".config/yadm/hooks/identity-guard.toml", body);
        }

        fn context(&self) -> Context {
            Context::new(self.home.clone(), vec![self.bin.clone()])
        }

        fn findings(&self) -> Vec<Finding> {
            let station = YadmCoverage::default();
            match station.check(&self.context()).expect("the station ran") {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("unexpectedly skipped: {reason}"),
            }
        }

        /// The findings whose summary mentions `needle`, which is how a test
        /// names one rule without depending on the order of the rest.
        fn about(&self, needle: &str) -> Vec<Finding> {
            self.findings()
                .into_iter()
                .filter(|finding| finding.summary.as_str().contains(needle))
                .collect()
        }
    }

    fn run_git(args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
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
    fn the_encrypt_lane_is_read_exactly_as_yadm_reads_it() {
        let policy = Encrypt::parse(
            "# a comment\n\n   \n!**/target\n.config/attic/**\n .config/leading\n.trailing \n",
        );
        assert_eq!(
            policy.includes,
            vec![
                ".config/attic/**".to_owned(),
                " .config/leading".to_owned(),
                ".trailing ".to_owned(),
            ],
            "yadm reads raw lines, so leading and trailing space are part of the pattern"
        );
        assert_eq!(policy.excludes, vec!["--exclude=/**/target".to_owned()]);
    }

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

    // --- The object database ------------------------------------------------

    #[test]
    fn a_database_is_bloated_only_when_it_is_both_big_and_dead() {
        let big_and_dead = Objects {
            in_pack: 10_000,
            pack_kib: PACK_FLOOR_KIB + 1,
            reachable: 2_000,
        };
        assert!(big_and_dead.is_bloated());
        assert_eq!(big_and_dead.unreachable(), 8_000);
        assert_eq!(big_and_dead.dead_percent(), 80);

        assert!(
            !Objects {
                pack_kib: PACK_FLOOR_KIB,
                ..big_and_dead
            }
            .is_bloated(),
            "a small database is not worth a word however dead it is"
        );
        assert!(
            !Objects {
                reachable: 9_500,
                ..big_and_dead
            }
            .is_bloated(),
            "ordinary garbage is not a mistake"
        );
        assert!(
            !Objects {
                in_pack: 0,
                reachable: 0,
                ..big_and_dead
            }
            .is_bloated(),
            "an empty database divides by nothing"
        );
    }

    // --- Reaching the repository -------------------------------------------

    #[test]
    fn no_yadm_on_the_path_is_broken_and_never_a_silent_pass() {
        let machine = quiet();
        fs_err::remove_file(machine.bin.join("yadm")).expect("removed");
        let findings = machine.findings();
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

        let found = machine.about("tracked in the clear and matched by an encrypt pattern");
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

        let found = machine.about("is managed and is not on disk");
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

        let found = machine.about("an encrypt pattern matches nothing");
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
        assert!(machine.about("matched by an encrypt pattern").is_empty());
        assert_eq!(
            machine.about(".config/stray.txt is undecided").len(),
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
            machine
                .about("an encrypt pattern matches nothing")
                .is_empty(),
            "`--others` is untracked-only, so a tracked match must be asked for separately"
        );
    }

    #[test]
    fn an_undecided_file_under_a_managed_directory_is_soft() {
        let machine = quiet();
        machine.write(".config/stray.txt", "nobody decided\n");

        let found = machine.about(".config/stray.txt is undecided");
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
            machine
                .about(".config/newskill/deep/inside.md is undecided")
                .len(),
            1,
            "an ancestor holding a managed file is what makes this a decision half-made"
        );
    }

    #[test]
    fn a_declared_path_is_a_decision_already_made() {
        let machine = quiet();
        machine.write(".config/stray.txt", "declared\n");
        machine.unmanaged(".config/stray.txt\tregenerated by its own tool\n");

        assert!(machine.about(".config/stray.txt is undecided").is_empty());
    }

    #[test]
    fn a_pruned_tree_is_never_a_decision() {
        let machine = quiet();
        machine.write(".config/relics/target/debug/huge.bin", "build output\n");
        machine.write(".config/gcloud/credentials.db", "regenerable\n");

        let findings = machine.findings();
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

        let found = machine.about("yadm/unmanaged is not there");
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

        let found = machine.about("holds a GitHub token");
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

        let found = machine.about("nothing has decided about it");
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
        assert!(machine.about("GitLab").is_empty());

        machine.track(".config/creds");
        let found = machine.about("a GitLab personal access token");
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
        assert!(machine.about("GitHub token").is_empty());
    }

    // --- The identity rules -------------------------------------------------

    #[test]
    fn an_undecrypted_identity_guard_is_a_note_and_never_a_pass() {
        let machine = quiet();
        let found = machine.about("identity guard is not decrypted");
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

        let found = machine.about("identity guard is unusable");
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

        let found = machine.about(".config/leaks.md is tracked in the clear");
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

        let found = machine.about(".config/loose.md is undecided and carries identifying content");
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
            machine
                .about(".config/near.md is tracked in the clear")
                .is_empty(),
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
            machine
                .about(".config/blob.bin is tracked in the clear")
                .len(),
            1,
            "nothing can vouch for content the guard cannot read"
        );

        machine.write(
            ".config/warden/config.toml",
            "[warden]\nbinary-allowed = [\".config/blob.bin\"]\n",
        );
        assert!(
            machine
                .about(".config/blob.bin is tracked in the clear")
                .is_empty(),
            "an allowed binary is a decision this machine recorded"
        );
    }
}
