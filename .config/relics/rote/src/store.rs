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

use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};

use anyhow::{Context as _, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use serde::{Deserialize, Serialize};

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

    /// The reminder cache.
    pub fn cache(&self) -> Utf8PathBuf {
        self.state_dir.join("cache.json")
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

fn tighten(path: &Utf8Path) -> Result<()> {
    fs_err::set_permissions(path, std::fs::Permissions::from_mode(FILE_MODE))
        .with_context(|| format!("tightening {path}"))
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
        let (prev, seq) = self.chains.head(&self.machine);
        let record = Record {
            v: SCHEMA,
            at: at.round(jiff::Unit::Second).unwrap_or(at),
            day,
            machine: self.machine.clone(),
            host: self.host.clone(),
            seq,
            prev,
            event,
        };
        let raw = render(&record)?;
        let path = self.paths.chain(&self.machine);
        let existed = path.exists();
        {
            use std::io::Write as _;
            let mut file = fs_err::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)?;
            writeln!(file, "{raw}")?;
            file.flush()?;
            file.into_parts().0.sync_all()?;
        }
        if !existed {
            tighten(&path)?;
        }
        self.chains.accept(&self.machine.clone(), &raw, &record);
        Ok(())
    }
}

/// What the reminder reads. Derived and disposable: the schedule projected down
/// to the two questions a nag asks, so the reminder never opens the corpus and
/// never resolves a machine identity.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cache {
    /// Schema, so a stale cache from an older binary is ignored rather than
    /// misread.
    pub v: u32,
    /// The day each drillable lineage next falls due.
    pub due: Vec<Date>,
    /// How many active lineages have no verifier on this machine. They cannot
    /// be drilled, so they are not counted as due — they are counted here.
    #[serde(default)]
    pub dormant: usize,
    /// What the corpus and the verifiers looked like when this was derived.
    ///
    /// A cache with no witness cannot say whether it is still true, only that
    /// it exists — and the one state a reminder most has to announce is the
    /// one that also destroys it. A restored machine brings the corpus back and
    /// leaves the verifiers and this file behind, so every lineage is dormant
    /// and nothing says so. See [`Witness`].
    #[serde(default)]
    pub witness: Witness,
    /// The drill day this was derived for.
    ///
    /// A day's worth of staleness is the most the witness can miss, because
    /// what falls due is a function of the date and of nothing on disk.
    #[serde(default)]
    pub built: Option<Date>,
}

/// A cheap reading of what the cache was derived from.
///
/// Sizes and modification times, never content: the reminder runs before every
/// shell prompt, so it may `stat` and may not parse. Chains are append-only, so
/// a length is a perfect witness of one; a verifier file is small and rewritten
/// whole, so a length and a time are enough. Anything that moves and is not
/// caught here is caught the next day by [`Cache::built`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Witness {
    /// Chain files present.
    pub chains: usize,
    /// Bytes across all of them.
    pub records: u64,
    /// Bytes of the verifier file, and zero when there is none.
    pub verifiers: u64,
    /// When the verifier file was last written, in whole seconds.
    pub held_at: i64,
}

impl Witness {
    /// Read the witness, without reading anything it witnesses.
    #[must_use]
    pub fn of(paths: &Paths) -> Self {
        let mut seen = Self::default();
        if let Ok(entries) = fs_err::read_dir(paths.chains_dir()) {
            for entry in entries.flatten() {
                let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                    continue;
                };
                if crate::corpus::chain::machine_of(&name).is_none() {
                    continue;
                }
                seen.chains = seen.chains.saturating_add(1);
                if let Ok(data) = entry.metadata() {
                    seen.records = seen.records.saturating_add(data.len());
                }
            }
        }
        if let Ok(data) = fs_err::metadata(paths.verifiers()) {
            seen.verifiers = data.len();
            seen.held_at = data
                .modified()
                .ok()
                .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                .and_then(|since| i64::try_from(since.as_secs()).ok())
                .unwrap_or_default();
        }
        seen
    }
}

impl Cache {
    /// Read the cache. Anything wrong with it reads as absent — the reminder is
    /// decoration, and decoration fails silent.
    pub fn load(path: &Utf8Path) -> Option<Self> {
        let text = fs_err::read_to_string(path).ok()?;
        let cache: Self = serde_json::from_str(&text).ok()?;
        (cache.v == SCHEMA).then_some(cache)
    }

    /// Whether this still answers for what is on disk.
    ///
    /// A reminder must not depend on state destroyed by the event it exists to
    /// announce. Absence used to read as silence, so a restore — corpus back,
    /// verifiers and cache gone — left every lineage dormant and nothing saying
    /// it. The same hole opens on any change `rote` did not make, such as a
    /// verifier deleted by hand.
    #[must_use]
    pub fn still_true(&self, paths: &Paths, today: Date) -> bool {
        self.built == Some(today) && self.witness == Witness::of(paths)
    }

    /// How many lineages are due on a given day.
    pub fn due_by(&self, today: Date) -> usize {
        self.due.iter().filter(|day| **day <= today).count()
    }

    /// Write the cache.
    ///
    /// # Errors
    ///
    /// When the tree cannot be created or the file cannot be written.
    pub fn save(&self, path: &Utf8Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            ensure_dir(parent)?;
        }
        relic_core::fs::write_atomic_private(path, &serde_json::to_string(self)?)
            .with_context(|| format!("writing {path}"))
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use jiff::civil::date;
    use jiff::tz::TimeZone;

    use std::os::unix::fs::PermissionsExt as _;

    use super::{Cache, Clock, Env, Paths, Store, Witness, ensure_dir};
    use crate::config::Config;
    use crate::corpus::record::{EngramId, Enrolled, Event, SCHEMA};
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
        for path in [
            paths.verifiers(),
            paths.cache(),
            paths.lock(),
            paths.marker.clone(),
        ] {
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
        assert_eq!(records.first().map(|r| r.seq), Some(0));
        assert_eq!(records.last().map(|r| r.seq), Some(1));
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
    fn a_cache_from_another_schema_reads_as_absent() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("cache.json")).unwrap();
        Cache {
            v: SCHEMA.saturating_add(1),
            due: vec![date(2026, 9, 13)],
            dormant: 0,
            ..Cache::default()
        }
        .save(&path)
        .unwrap();
        assert!(Cache::load(&path).is_none());
    }

    #[test]
    fn the_cache_counts_what_is_due_by_a_day() {
        let cache = Cache {
            v: SCHEMA,
            due: vec![date(2026, 9, 12), date(2026, 9, 13), date(2026, 9, 20)],
            dormant: 2,
            ..Cache::default()
        };
        assert_eq!(cache.due_by(date(2026, 9, 13)), 2);
        assert_eq!(cache.dormant, 2);
    }

    #[test]
    fn a_directory_is_private_from_the_moment_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let deep = Utf8PathBuf::from_path_buf(dir.path().join("a/b/c")).unwrap();
        ensure_dir(&deep).unwrap();
        let mode = std::fs::metadata(&deep).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o700);
    }

    #[test]
    fn a_cache_stops_answering_once_what_it_was_derived_from_moves() {
        let dir = tempfile::tempdir().unwrap();
        let root = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let paths = Paths {
            log_dir: root.join("ark"),
            state_dir: root.join("state"),
            marker: root.join("flagship"),
        };
        ensure_dir(&paths.chains_dir()).unwrap();
        ensure_dir(&paths.state_dir).unwrap();
        let today = date(2026, 9, 13);
        let mut cache = Cache {
            v: SCHEMA,
            due: Vec::new(),
            dormant: 0,
            witness: Witness::of(&paths),
            built: Some(today),
        };
        assert!(cache.still_true(&paths, today));

        // Tomorrow is a different question, whatever is on disk.
        assert!(!cache.still_true(&paths, date(2026, 9, 14)));

        // A verifier taken away by hand is the restore case in miniature: the
        // cache still asserts yesterday's answer, and must stop.
        fs_err::write(paths.verifiers(), "held = {}\n").unwrap();
        assert!(!cache.still_true(&paths, today));
        cache.witness = Witness::of(&paths);
        assert!(cache.still_true(&paths, today));

        // And a chain appended to since.
        fs_err::write(paths.chains_dir().join("lusab-babad-gutih.jsonl"), "{}\n").unwrap();
        assert!(!cache.still_true(&paths, today));
    }
}
