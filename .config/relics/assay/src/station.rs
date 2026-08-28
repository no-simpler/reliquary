//! What a station is, and what it is given.

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};

use relic_core::finding::{Outcome, StationId};

/// Where the system lists the shells that may serve as login shells.
const DEFAULT_SHELLS: &str = "/etc/shells";

/// One check.
///
/// A station reports; it never grades, never exits, and never decides what a
/// finding means for the run. That is [`relic_core::finding::Grade`]'s job, and
/// keeping it there is what deletes the mutable `FAILS`/`WARNS` counters the
/// shell checkers carried.
///
/// Returning `Err` is allowed and is not a way to report a problem with the
/// machine: it means the station itself could not complete. The runner turns it
/// into a [`relic_core::finding::Severity::Broken`] finding, so a check that
/// throws is never a quiet pass.
pub trait Station {
    /// The token that selects this station on the command line.
    fn id(&self) -> &StationId;

    /// One line for `--list`, saying what it checks.
    fn title(&self) -> &'static str;

    /// Run it.
    ///
    /// # Errors
    ///
    /// When the station itself could not complete. That is not how a problem
    /// with the machine is reported — see the note on the trait.
    fn check(&self, cx: &Context) -> Result<Outcome>;

    /// What this station's rules were copied out of, when they were copied out
    /// of something.
    ///
    /// Most stations read the machine and answer for it; nothing they know can
    /// go stale. A station that instead **transcribes a table from a
    /// third-party binary** carries an obligation the others do not: when that
    /// binary moves, the table may no longer describe it, and the station goes
    /// on reporting confidently against rules that are no longer the rules.
    ///
    /// Declaring the derivation is how the runner can say so. It is a
    /// runner-level facility on purpose — a station that checked its own
    /// freshness would be the same twenty lines written once per station, and
    /// the twentieth would be the one that forgot.
    fn derived_from(&self) -> Option<Derivation> {
        None
    }
}

/// Where a station's rules came from, and how to check they are still current.
///
/// Deliberately not a version *requirement*. A newer artefact does not make the
/// transcription wrong — most upgrades change nothing the table describes — so
/// drift is reported and never graded. What it buys is that the question gets
/// asked at all, with the recipe to answer it sitting right there.
#[derive(Clone, Copy)]
pub struct Derivation {
    /// What was read, in the words a person would use.
    pub artefact: &'static str,
    /// The version it was read against.
    pub version: &'static str,
    /// How to read it again.
    pub recipe: &'static str,
    /// What is installed now, or nothing when that cannot be determined.
    ///
    /// A function pointer rather than a closure: a station is a `dyn` trait
    /// object, and the resolution has to be testable against a fixture home
    /// without a process environment.
    pub installed: fn(&Context) -> Option<String>,
}

/// Everything a station is allowed to know about the machine.
///
/// Ambient authority is injected, never read. A station that reaches for `$HOME`
/// itself can only be tested by mutating a process environment, which no two
/// tests can safely do at once — the rule [`relic_core::ui::FormatInputs`] and
/// [`relic_core::tool::Tool`] already follow.
#[derive(Clone, Debug)]
pub struct Context {
    /// The home directory the checks are about.
    home: Utf8PathBuf,
    /// The search path, in order. Injected rather than read, so a station that
    /// resolves a program is testable against a directory a test built.
    path: Vec<Utf8PathBuf>,
    /// Where the system lists the shells that may serve as login shells.
    /// Injected for the same reason as `path`: a station that read the real
    /// `/etc/shells` would answer for the machine the test runs on.
    shells: Utf8PathBuf,
    /// Whether checks that cost the network, a passphrase or real time may run.
    /// Off by default: `assay` is detect-only, offline and side-effect-free
    /// until asked otherwise.
    deep: bool,
}

impl Context {
    /// A context over one home directory and one search path.
    ///
    /// Both are arguments rather than lookups. A station that reads `$PATH`
    /// itself can only be tested by mutating a process environment, which
    /// `unsafe_code = "forbid"` makes unsafe and no two tests can share.
    pub fn new(home: impl Into<Utf8PathBuf>, path: Vec<Utf8PathBuf>) -> Self {
        Self {
            home: home.into(),
            path,
            shells: Utf8PathBuf::from(DEFAULT_SHELLS),
            deep: false,
        }
    }

    /// The search path, in the order it is searched.
    pub fn path(&self) -> &[Utf8PathBuf] {
        &self.path
    }

    /// Where permissible login shells are listed.
    pub fn shells(&self) -> &Utf8Path {
        &self.shells
    }

    /// Point the login-shell check at another file, for a test that must not
    /// answer for the machine it runs on.
    #[must_use]
    pub fn with_shells(mut self, shells: impl Into<Utf8PathBuf>) -> Self {
        self.shells = shells.into();
        self
    }

    /// The search path this process inherited, for the binary to hand over.
    #[must_use]
    pub fn ambient_path() -> Vec<Utf8PathBuf> {
        std::env::var_os("PATH")
            .map(|value| {
                std::env::split_paths(&value)
                    .filter_map(|dir| Utf8PathBuf::from_path_buf(dir).ok())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// Permit the checks that are not free.
    #[must_use]
    pub fn deeply(mut self) -> Self {
        self.deep = true;
        self
    }

    /// The home directory the checks are about.
    pub fn home(&self) -> &Utf8Path {
        &self.home
    }

    /// A path under it.
    pub fn at(&self, relative: impl AsRef<str>) -> Utf8PathBuf {
        self.home.join(relative.as_ref())
    }

    /// Whether the expensive checks may run.
    pub fn deep(&self) -> bool {
        self.deep
    }
}
