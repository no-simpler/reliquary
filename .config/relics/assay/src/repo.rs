//! The dotfiles repository, and the vocabulary every lane speaks about it.
//!
//! yadm is a git wrapper over a work tree that is `$HOME`, with a git directory
//! it will name on request. Two stations need that: one asks which paths are in
//! which lane, the other asks which files are ours to lint. Both must get the
//! same answer, so both ask the same program the same question — a second
//! implementation of "what is tracked" is a second answer waiting to disagree.

use anyhow::{Context as _, Result};
use camino::{Utf8Path, Utf8PathBuf};
use relic_core::finding::{Finding, FixHint, Location, StationId, Summary};
use relic_core::git::{self, Git};
use relic_core::tool::Tool;
use std::fmt;

use crate::station::Context;

/// Where the yadm wrapper lives. A `yadm` resolved here would print its own
/// archive-check banner into the middle of a station's answer.
pub(crate) const WRAPPER_DIR: &str = ".config/bin";

/// The pack size below which object-database shape is not worth a word.
const PACK_FLOOR_KIB: u64 = 50 * 1024;

/// Fewer unreachable objects than this is ordinary garbage, not a mistake.
const UNREACHABLE_FLOOR: u64 = 1000;

// --- Reaching the repository ------------------------------------------------

/// Why the station could not read the lanes. Every one of these means the
/// machine cannot be verified, which is a `Broken` finding rather than an
/// `Err`: the station itself is working fine, and it has something true to say.
#[derive(Debug)]
pub(crate) enum Unreachable {
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
    pub(crate) fn into_finding(self, id: &StationId) -> Finding {
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
pub(crate) struct Repo {
    pub(crate) git: Git,
    /// The git directory, as yadm reports it.
    pub(crate) dir: Utf8PathBuf,
    /// The work tree, which is the home directory.
    pub(crate) home: Utf8PathBuf,
}

impl Repo {
    /// Locate the repository by asking yadm, and prove git can be run.
    pub(crate) fn discover(cx: &Context) -> Result<Self, Unreachable> {
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
    pub(crate) fn ls_files(&self, lane: Lane, scope: Scope<'_>) -> Result<Vec<Rel>> {
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
    pub(crate) fn tracked(&self) -> Result<Vec<Rel>> {
        self.ls_files(Lane::Tracked, Scope::Everything)
    }

    /// Everything the encrypt lane would sweep in — yadm's own query.
    pub(crate) fn encrypted(&self, encrypt: &Encrypt) -> Result<Vec<Rel>> {
        self.ls_files(Lane::Untracked, Scope::Encrypt(encrypt))
    }

    /// Everything matched by the encrypt lane that is *also* tracked in the
    /// clear. yadm asks this separately for the same reason: `--others` is
    /// untracked-only, so the overlap is invisible to the first query.
    pub(crate) fn in_both_lanes(&self, encrypt: &Encrypt) -> Result<Vec<Rel>> {
        self.ls_files(Lane::Tracked, Scope::Encrypt(encrypt))
    }

    /// Whether one pattern matches anything at all, in either lane.
    pub(crate) fn pattern_is_live(&self, encrypt: &Encrypt, pattern: &str) -> Result<bool> {
        let scope = Scope::OnePattern(encrypt, pattern);
        if !self.ls_files(Lane::Untracked, scope)?.is_empty() {
            return Ok(true);
        }
        // Only reached for a pattern with no untracked matches, so the second
        // fork is paid on the rare path rather than on every pattern.
        Ok(!self.ls_files(Lane::Tracked, scope)?.is_empty())
    }

    /// The object database's shape, or nothing when git would not say.
    pub(crate) fn objects(&self) -> Option<Objects> {
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
    pub(crate) fn plumbing(&self, args: &[&str]) -> Option<String> {
        let mut command = self.git.command();
        command.arg("--git-dir").arg(&self.dir).args(args);
        self.git.capture(&mut command).ok().map(|out| out.stdout)
    }
}

/// Which half of the work tree a question is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Lane {
    /// What the index holds.
    Tracked,
    /// What it does not — yadm's own `--others`, deliberately without
    /// `--exclude-standard`, because `.gitignore` does not protect the archive.
    Untracked,
}

impl Lane {
    pub(crate) fn args(self) -> &'static [&'static str] {
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
pub(crate) enum Scope<'a> {
    /// Every path in the lane, with no pathspec at all.
    Everything,
    /// What the encrypt lane names, and nothing when it names nothing.
    Encrypt(&'a Encrypt),
    /// One of its patterns.
    OnePattern(&'a Encrypt, &'a str),
}

impl<'a> Scope<'a> {
    /// The exclusions and pathspecs, or `None` when the scope selects nothing.
    pub(crate) fn args(self) -> Option<(&'a [String], Vec<&'a str>)> {
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
pub(crate) fn real_yadm(cx: &Context) -> Option<Tool> {
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
pub(crate) struct Rel(Utf8PathBuf);

impl Rel {
    pub(crate) fn new(spelled: &str) -> Self {
        Self(Utf8PathBuf::from(spelled))
    }

    /// The path under `home`, or nothing when it is not under it.
    pub(crate) fn under(home: &Utf8Path, absolute: &Utf8Path) -> Option<Self> {
        absolute
            .strip_prefix(home)
            .ok()
            .and_then(|rest| (!rest.as_str().is_empty()).then(|| Self(rest.to_owned())))
    }

    pub(crate) fn as_str(&self) -> &str {
        self.0.as_str()
    }

    pub(crate) fn path(&self) -> &Utf8Path {
        &self.0
    }

    pub(crate) fn absolute(&self, home: &Utf8Path) -> Utf8PathBuf {
        home.join(&self.0)
    }

    /// Every directory above it, nearest first.
    pub(crate) fn ancestors(&self) -> impl Iterator<Item = &Utf8Path> {
        self.0
            .ancestors()
            .skip(1)
            .filter(|dir| !dir.as_str().is_empty())
    }

    pub(crate) fn location(&self) -> Location {
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
pub(crate) struct Encrypt {
    /// Pathspecs, in file order.
    pub(crate) includes: Vec<String>,
    /// Exclusions, already in the `--exclude=/…` form git wants.
    pub(crate) excludes: Vec<String>,
}

impl Encrypt {
    pub(crate) fn read(path: &Utf8Path) -> Result<Self> {
        let text = fs_err::read_to_string(path.as_std_path())
            .with_context(|| format!("reading {path} — the encrypt lane is undefined"))?;
        Ok(Self::parse(&text))
    }

    pub(crate) fn parse(text: &str) -> Self {
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

// --- What the object database looks like ------------------------------------

/// The shape of the object database, in the two numbers that matter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Objects {
    pub(crate) in_pack: u64,
    pub(crate) pack_kib: u64,
    pub(crate) reachable: u64,
}

impl Objects {
    /// Objects packed but reachable from nothing.
    pub(crate) fn unreachable(self) -> u64 {
        self.in_pack.saturating_sub(self.reachable)
    }

    /// Whether the database is big enough, and dead enough, to be worth saying.
    pub(crate) fn is_bloated(self) -> bool {
        let dead = self.unreachable();
        self.in_pack > 0
            && self.reachable > 0
            && self.pack_kib > PACK_FLOOR_KIB
            && dead > UNREACHABLE_FLOOR
            && dead.saturating_mul(5) > self.in_pack
    }

    /// How much of the database is dead, as whole percent.
    pub(crate) fn dead_percent(self) -> u64 {
        if self.in_pack == 0 {
            return 0;
        }
        self.unreachable().saturating_mul(100) / self.in_pack
    }
}

/// A machine built from nothing, for the stations that read one.
///
/// Every path a station reads is a file this writes, so no test answers for
/// the machine it runs on — the same reason [`Context`] takes a home and a
/// search path rather than reading them.
#[cfg(test)]
pub(crate) mod testing {
    use std::os::unix::fs::PermissionsExt as _;
    use std::process::Command;

    use camino::Utf8PathBuf;
    use relic_core::finding::{Finding, Outcome};

    use crate::station::Context;

    /// A machine built from nothing: a home, a yadm repository over it, and a
    /// `yadm` on the injected search path that answers where that repository
    /// is. Everything the station reads is a file this writes, so no test
    /// answers for the machine it runs on.
    pub(crate) struct Machine {
        _dir: tempfile::TempDir,
        pub(crate) home: Utf8PathBuf,
        pub(crate) bin: Utf8PathBuf,
        pub(crate) repo: Utf8PathBuf,
    }

    impl Machine {
        pub(crate) fn new() -> Self {
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

        pub(crate) fn write(&self, rel: &str, body: &str) -> Utf8PathBuf {
            let path = self.home.join(rel);
            if let Some(parent) = path.parent() {
                fs_err::create_dir_all(parent).expect("a parent directory");
            }
            fs_err::write(&path, body).expect("written");
            path
        }

        pub(crate) fn write_bytes(&self, rel: &str, body: &[u8]) {
            let path = self.home.join(rel);
            if let Some(parent) = path.parent() {
                fs_err::create_dir_all(parent).expect("a parent directory");
            }
            fs_err::write(&path, body).expect("written");
        }

        pub(crate) fn executable(&self, name: &str, body: &str) {
            let path = self.bin.join(name);
            fs_err::write(&path, body).expect("written");
            fs_err::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
                .expect("made executable");
        }

        /// Stages a path, which is all `ls-files` needs — the index is the
        /// tracked set, and a commit would only add a signature prompt.
        pub(crate) fn track(&self, rel: &str) {
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

        /// Run a station over this machine and take its outcome.
        pub(crate) fn outcome_of(&self, station: &dyn crate::station::Station) -> Outcome {
            station.check(&self.context()).expect("the station ran")
        }

        /// Its findings, insisting it actually ran.
        pub(crate) fn findings_of(&self, station: &dyn crate::station::Station) -> Vec<Finding> {
            match self.outcome_of(station) {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("unexpectedly skipped: {reason}"),
            }
        }

        /// The findings whose summary mentions `needle`, which is how a test
        /// names one rule without depending on the order of the rest.
        pub(crate) fn about_of(
            &self,
            station: &dyn crate::station::Station,
            needle: &str,
        ) -> Vec<Finding> {
            self.findings_of(station)
                .into_iter()
                .filter(|finding| finding.summary.as_str().contains(needle))
                .collect()
        }

        pub(crate) fn context(&self) -> Context {
            Context::new(self.home.clone(), vec![self.bin.clone()])
        }
    }

    pub(crate) fn run_git(args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .status()
            .expect("git runs");
        assert!(status.success(), "git {args:?} failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
