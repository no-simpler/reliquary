//! Where things live, and the three homes they live in.
//!
//! **The record is in `ark`.** Outcomes, timings and intervals — not
//! regenerable, and the whole point. Durable, and restic's to keep. One chain
//! per machine, because an append-only hash chain is not a mergeable structure:
//! two writers in one file is a fork, two writers in two files is a corpus.
//!
//! **The verifiers are not.** An argon2id verifier is a confirmation oracle:
//! unlimited offline guessing against an unambiguous success signal. That is an
//! accepted shape for a ninety-bit phrase and a poor one for a login password,
//! and putting one in `ark` would replicate it to two providers, into every
//! snapshot, and into a month of prior versions — where no rotation can reach
//! it. A verifier is regenerable by re-typing the secret and carries no history,
//! so it belongs where losing the local copy loses nothing.
//!
//! **The flagship marker is neither.** It says what this machine is, so it must
//! be machine-local by construction: a marker that travelled with the dotfiles
//! would declare every machine the flagship, which is exactly backwards. `rote`
//! only ever reads it.
//!
//! An engram with no verifier here is therefore a first-class state, not a
//! corruption: it is what a bare-metal restore looks like, and it is correct.
//! The history survives; the oracle does not, and `attach` is how it comes back.

use std::os::unix::fs::DirBuilderExt as _;

use anyhow::{Context as _, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};

use crate::config::Config;
use crate::corpus::chain::{Chains, file_name};
use crate::corpus::record::{Event, Record, SCHEMA, render};
use crate::machine::{Flagship, MachineId};

/// Mode for every directory `rote` creates. The directory is the control; the
/// file mode below it is belt.
const DIR_MODE: u32 = 0o700;

/// Mode for every file `rote` writes.
const FILE_MODE: u32 = 0o600;

/// The clock, as a capability.
///
/// A drill day is a **local calendar** day shifted by the rollover hour, not a
/// UTC one: a daily ritual is a calendar concept, and a UTC boundary falls in
/// the small hours here. Each record freezes the day it was credited to, so a
/// later change of timezone cannot re-date history.
#[derive(Clone, Debug)]
pub struct Clock {
    now: Timestamp,
    zone: TimeZone,
    rollover_hour: i8,
}

impl Clock {
    /// Build a clock from an instant, a zone and a rollover hour.
    pub fn new(now: Timestamp, zone: TimeZone, rollover_hour: i8) -> Self {
        Self {
            now,
            zone,
            rollover_hour,
        }
    }

    /// The instant.
    pub fn now(&self) -> Timestamp {
        self.now
    }

    /// The drill day the instant belongs to.
    pub fn today(&self) -> Date {
        let zoned = self.now.to_zoned(self.zone.clone());
        zoned
            .checked_sub(Span::new().hours(i64::from(self.rollover_hour)))
            .unwrap_or(zoned)
            .date()
    }
}

/// The ambient values `rote` is allowed to read, read once at the edge.
#[derive(Clone, Debug, Default)]
pub struct Env {
    /// The home directory.
    pub home: Option<Utf8PathBuf>,
    /// `ROTE_ROOT`, the corpus tree. A test seam, and an escape hatch.
    pub root: Option<Utf8PathBuf>,
    /// `ROTE_STATE`, the machine-local tree.
    pub state: Option<Utf8PathBuf>,
    /// `ROTE_CONFIG`, the config file.
    pub config: Option<Utf8PathBuf>,
    /// `ROTE_FLAGSHIP`, the marker. A seam rather than a derived path, because
    /// the marker is lane-wide and not `rote`'s to place under `ROTE_STATE`.
    pub flagship: Option<Utf8PathBuf>,
    /// `ROTE_MACHINE`, this machine's identity. A seam so a suite can be two
    /// machines at once; never a file, which could be copied onto a second one.
    pub machine: Option<String>,
    /// This machine's label, recorded on every line.
    pub host: String,
    /// `ROTE_NOW`, the instant the process believes it is running at.
    ///
    /// A drill day is a local calendar day shifted by the rollover hour, so a
    /// suite that derives its own fixtures from the wall clock and a binary
    /// that derives its own disagree about which day it is, for the minutes a
    /// run happens to straddle four in the morning. The clock is a parameter
    /// everywhere inside the crate already; this is the parameter at the
    /// process boundary. It moves no cost and lowers no cost: what it is worth
    /// forging, `ROTE_MACHINE` already forges more directly.
    pub now: Option<jiff::Timestamp>,
}

impl Env {
    /// Read the environment. The only place in the crate that does.
    pub fn from_process() -> Self {
        let read = |name: &str| {
            std::env::var_os(name)
                .map(std::path::PathBuf::from)
                .and_then(|p| Utf8PathBuf::from_path_buf(p).ok())
        };
        Self {
            home: relic_core::path::home(),
            root: read("ROTE_ROOT"),
            state: read("ROTE_STATE"),
            config: read("ROTE_CONFIG"),
            flagship: read("ROTE_FLAGSHIP"),
            machine: std::env::var("ROTE_MACHINE").ok(),
            host: crate::machine::hostname(),
            now: std::env::var("ROTE_NOW")
                .ok()
                .and_then(|text| text.parse().ok()),
        }
    }

    /// Where the config file is.
    ///
    /// # Errors
    ///
    /// When there is no override and no home directory.
    pub fn config_path(&self) -> Result<Utf8PathBuf> {
        if let Some(path) = &self.config {
            return Ok(path.clone());
        }
        Ok(self
            .home()?
            .join(".config")
            .join("rote")
            .join("config.toml"))
    }

    fn home(&self) -> Result<&Utf8Path> {
        self.home
            .as_deref()
            .ok_or_else(|| anyhow!("HOME is unset or not UTF-8, so there is nowhere to look"))
    }
}

/// The three homes.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The corpus tree, in `ark`.
    pub log_dir: Utf8PathBuf,
    /// The machine-local tree.
    pub state_dir: Utf8PathBuf,
    /// The flagship marker, which `rote` reads and never writes.
    pub marker: Utf8PathBuf,
}

impl Paths {
    /// Resolve every home: an override first, then the config, then the default.
    ///
    /// # Errors
    ///
    /// When a default is needed and there is no home directory.
    pub fn resolve(env: &Env, config: &Config) -> Result<Self> {
        let log_dir = match env.root.clone().or_else(|| config.root.clone()) {
            Some(path) => path,
            None => env.home()?.join("Trove").join("ark").join("rote"),
        };
        let state_dir = match env.state.clone().or_else(|| config.state.clone()) {
            Some(path) => path,
            None => env.home()?.join(".local").join("state").join("rote"),
        };
        let marker = match env.flagship.clone() {
            Some(path) => path,
            None => crate::machine::marker_path(env.home()?, None),
        };
        Ok(Self {
            log_dir,
            state_dir,
            marker,
        })
    }

    /// Where the chains live.
    pub fn chains_dir(&self) -> Utf8PathBuf {
        self.log_dir.join("chains")
    }

    /// One machine's chain.
    pub fn chain(&self, machine: &MachineId) -> Utf8PathBuf {
        self.chains_dir().join(file_name(machine))
    }

    /// The lock. Machine-local, because it guards this machine's chain and
    /// nothing else — a lock in `ark` would be both wrong-grained and litter in
    /// a tree that replicates offsite.
    pub fn lock(&self) -> Utf8PathBuf {
        self.state_dir.join("chain.lock")
    }

    /// The verifier file.
    pub fn verifiers(&self) -> Utf8PathBuf {
        self.state_dir.join("verifiers.toml")
    }
}

/// Create a directory and its missing parents, private from the start.
///
/// Modes are set at creation rather than afterwards, so there is no window in
/// which the directory is readable. A directory that already exists is left
/// alone: tightening someone else's tree is not this tool's call.
///
/// # Errors
///
/// When a component cannot be created.
pub fn ensure_dir(path: &Utf8Path) -> Result<()> {
    if path.is_dir() {
        return Ok(());
    }
    if let Some(parent) = path.parent()
        && !parent.as_str().is_empty()
    {
        ensure_dir(parent)?;
    }
    match std::fs::DirBuilder::new().mode(DIR_MODE).create(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error).with_context(|| format!("creating {path}")),
    }
}

/// A locked handle on this machine's chain, for the commands that write.
pub struct Store {
    paths: Paths,
    machine: MachineId,
    host: String,
    chains: Chains,
    _lock: relic_core::lock::Lock,
}

impl Store {
    /// Take the lock and read the corpus.
    ///
    /// **Every refusal a writer can meet lands here**, before any command has
    /// touched the verifier file — not at the append, after it has. A command
    /// that minted an oracle and then could not record it would leave the two
    /// disagreeing in the one direction that lies.
    ///
    /// # Errors
    ///
    /// When this machine is not the flagship, the tree cannot be created, the
    /// lock cannot be taken, the corpus cannot be read, or it holds a record
    /// from a newer schema.
    pub fn open(paths: Paths, machine: MachineId, host: String) -> Result<Self> {
        let flagship = Flagship::read(&paths.marker, &machine)?;
        if !flagship.writes_allowed() {
            return Err(anyhow!(
                "{}",
                flagship.refusal(&paths.marker, &machine, &host)
            ));
        }
        ensure_dir(&paths.chains_dir())?;
        ensure_dir(&paths.state_dir)?;
        let lock =
            relic_core::lock::Lock::acquire(&paths.lock(), relic_core::lock::Wait::INTERACTIVE)?;
        let chains = Chains::load(&paths.chains_dir())?;
        if chains.has_future_records() {
            return Err(anyhow!(
                "the corpus holds a record written by a newer rote, so this one will not add to it"
            ));
        }
        Ok(Self {
            paths,
            machine,
            host,
            chains,
            _lock: lock,
        })
    }

    /// The corpus as it stands.
    pub fn chains(&self) -> &Chains {
        &self.chains
    }

    /// This machine.
    pub fn machine(&self) -> &MachineId {
        &self.machine
    }

    /// Append one event to this machine's chain.
    ///
    /// The envelope is the store's to fill: the caller knows what happened, and
    /// the store knows where in which chain it goes.
    ///
    /// # Errors
    ///
    /// When the corpus holds a record this binary cannot read, or the write
    /// fails.
    pub fn append(&mut self, at: Timestamp, day: Date, event: Event) -> Result<()> {
        if self.chains.has_future_records() {
            return Err(anyhow!(
                "the corpus holds a record written by a newer rote, so this one will not add to it"
            ));
        }
        let prev = self.chains.head(&self.machine);
        let record = Record {
            v: SCHEMA,
            at: at.round(jiff::Unit::Second).unwrap_or(at),
            day,
            host: self.host.clone(),
            prev,
            event,
        };
        let raw = render(&record)?;
        let path = self.paths.chain(&self.machine);
        {
            use fs_err::os::unix::fs::OpenOptionsExt as _;
            use std::io::Write as _;
            // The mode is set at creation, so a chain is private from its
            // first byte rather than from a moment after it.
            let mut file = fs_err::OpenOptions::new()
                .create(true)
                .append(true)
                .mode(FILE_MODE)
                .open(&path)?;
            writeln!(file, "{raw}")?;
            file.flush()?;
            file.into_parts().0.sync_all()?;
        }
        self.chains.accept(&self.machine.clone(), &raw, &record);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use jiff::civil::date;
    use jiff::tz::TimeZone;

    use std::os::unix::fs::PermissionsExt as _;

    use super::{Clock, Env, Paths, Store, ensure_dir};
    use crate::config::Config;
    use crate::corpus::record::{Digest, EngramId, Enrolled, Event, SCHEMA};
    use crate::machine::MachineId;

    fn tree() -> (tempfile::TempDir, Paths, MachineId) {
        let dir = tempfile::tempdir().unwrap();
        let base = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let paths = Paths {
            log_dir: base.join("ark"),
            state_dir: base.join("state"),
            marker: base.join("flagship"),
        };
        std::fs::create_dir_all(&base).unwrap();
        std::fs::write(&paths.marker, "").unwrap();
        (dir, paths, MachineId::of("test"))
    }

    fn enroll(name: &str) -> Event {
        Event::Enroll(Enrolled {
            slug: name.parse().unwrap(),
            engram: EngramId::mint().unwrap(),
            critical: false,
        })
    }

    #[test]
    fn the_drill_day_follows_the_local_clock_and_the_rollover_hour() {
        let zone = TimeZone::get("Europe/Berlin").unwrap();
        let late = "2026-09-10T23:30:00Z".parse().unwrap();
        assert_eq!(Clock::new(late, zone.clone(), 4).today(), date(2026, 9, 10));
        let morning = "2026-09-11T06:00:00Z".parse().unwrap();
        assert_eq!(Clock::new(morning, zone, 4).today(), date(2026, 9, 11));
    }

    #[test]
    fn a_rollover_of_zero_is_the_plain_local_date() {
        let zone = TimeZone::get("Europe/Berlin").unwrap();
        let late = "2026-09-10T23:30:00Z".parse().unwrap();
        assert_eq!(Clock::new(late, zone, 0).today(), date(2026, 9, 11));
    }

    #[test]
    fn the_defaults_put_the_corpus_in_ark_and_everything_else_out_of_it() {
        let env = Env {
            home: Some(Utf8PathBuf::from("/home/x")),
            ..Env::default()
        };
        let paths = Paths::resolve(&env, &Config::default()).unwrap();
        assert_eq!(paths.log_dir, "/home/x/Trove/ark/rote");
        assert_eq!(paths.state_dir, "/home/x/.local/state/rote");
        assert_eq!(paths.marker, "/home/x/.local/state/reliquary/flagship");
        for path in [paths.verifiers(), paths.lock(), paths.marker.clone()] {
            assert!(
                !path.starts_with(&paths.log_dir),
                "{path} must never sit inside the tree that goes offsite"
            );
        }
    }

    #[test]
    fn a_chain_is_named_for_its_machine_and_nothing_else() {
        let (_dir, paths, machine) = tree();
        assert_eq!(
            paths.chain(&machine),
            paths.chains_dir().join(format!("{machine}.jsonl"))
        );
    }

    #[test]
    fn appending_chains_each_record_to_the_one_before_it() {
        let (_dir, paths, machine) = tree();
        let mut store = Store::open(paths.clone(), machine.clone(), "Mac".to_owned()).unwrap();
        let at = date(2026, 9, 13)
            .to_zoned(TimeZone::UTC)
            .unwrap()
            .timestamp();
        store.append(at, date(2026, 9, 13), enroll("a")).unwrap();
        store.append(at, date(2026, 9, 13), enroll("b")).unwrap();
        drop(store);

        let chains = crate::corpus::chain::Chains::load(&paths.chains_dir()).unwrap();
        assert!(chains.issues.is_empty(), "{:?}", chains.issues);
        let records = chains.records();
        assert_eq!(records.len(), 2);
        assert_eq!(records.first().map(|r| r.prev), Some(Digest::GENESIS));
        assert_ne!(records.last().map(|r| r.prev), Some(Digest::GENESIS));
        assert_eq!(records.first().map(|r| r.v), Some(SCHEMA));
    }

    #[test]
    fn the_chain_is_private_the_moment_it_exists() {
        let (_dir, paths, machine) = tree();
        let mut store = Store::open(paths.clone(), machine.clone(), "Mac".to_owned()).unwrap();
        let at = date(2026, 9, 13)
            .to_zoned(TimeZone::UTC)
            .unwrap()
            .timestamp();
        store.append(at, date(2026, 9, 13), enroll("a")).unwrap();
        let mode = std::fs::metadata(paths.chain(&machine))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn a_machine_that_is_not_the_flagship_cannot_open_a_store_at_all() {
        let (_dir, paths, machine) = tree();
        std::fs::remove_file(&paths.marker).unwrap();
        let error = match Store::open(paths, machine, "Mac".to_owned()) {
            Ok(_) => panic!("a satellite must not open a store"),
            Err(error) => error.to_string(),
        };
        assert!(error.contains("Reads are fine"));
    }

    #[test]
    fn a_directory_is_private_from_the_moment_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let deep = Utf8PathBuf::from_path_buf(dir.path().join("a/b/c")).unwrap();
        ensure_dir(&deep).unwrap();
        let mode = std::fs::metadata(&deep).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }
}
