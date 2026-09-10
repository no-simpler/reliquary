//! Where things live, and the two homes they live in.
//!
//! **The log is in `ark`.** Outcomes, timings and intervals — not regenerable,
//! and the whole point. Durable, and restic's to keep.
//!
//! **The verifiers are not.** An argon2id verifier is a confirmation oracle:
//! unlimited offline guessing against an unambiguous success signal. That is an
//! accepted shape for a ninety-bit phrase and a poor one for a login password,
//! and putting one in `ark` would replicate it to two providers, into every
//! snapshot, and into a month of prior versions — where no rotation can reach
//! it. A verifier is regenerable by re-enrollment and carries no history, so it
//! belongs where losing the local copy loses nothing.
//!
//! A slug with no verifier is therefore a first-class state, not a corruption:
//! it is what a bare-metal restore looks like, and it is correct. The history
//! survives; the oracle does not.

use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::{DirBuilderExt as _, PermissionsExt as _};

use anyhow::{Context as _, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use jiff::civil::Date;
use jiff::tz::TimeZone;
use jiff::{Span, Timestamp};
use serde::{Deserialize, Serialize};

use crate::config::Config;
use crate::log::{Digest, Line, Record, SCHEMA};
use crate::slug::Slug;
use crate::verifier::Verifier;

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
    /// `ROTE_ROOT`, the log tree. A test seam, and an escape hatch.
    pub root: Option<Utf8PathBuf>,
    /// `ROTE_STATE`, the machine-local tree.
    pub state: Option<Utf8PathBuf>,
    /// `ROTE_CONFIG`, the config file.
    pub config: Option<Utf8PathBuf>,
    /// This machine's name, recorded on every line.
    pub host: String,
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
            host: hostname(),
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

fn hostname() -> String {
    // `ROTE_HOST` is a test seam: a suite that has to run on any machine cannot
    // assert against the machine it happens to be running on.
    std::env::var("ROTE_HOST").unwrap_or_else(|_| {
        gethostname::gethostname()
            .into_string()
            .unwrap_or_else(|_| "unknown".to_owned())
    })
}

/// The two homes.
#[derive(Clone, Debug)]
pub struct Paths {
    /// The log tree, in `ark`.
    pub log_dir: Utf8PathBuf,
    /// The machine-local tree.
    pub state_dir: Utf8PathBuf,
}

impl Paths {
    /// Resolve both homes: an override first, then the config, then the default.
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
        Ok(Self { log_dir, state_dir })
    }

    /// The log file.
    pub fn log(&self) -> Utf8PathBuf {
        self.log_dir.join("log.jsonl")
    }

    /// The lock beside it.
    pub fn lock(&self) -> Utf8PathBuf {
        self.log_dir.join("log.lock")
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

/// Something wrong with the log itself, as opposed to with a drill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    /// A line that would not parse. It is kept, never dropped.
    Malformed {
        /// One-based line number.
        line: usize,
        /// What the parser said.
        why: String,
    },
    /// A line whose `prev` does not match the line before it.
    ChainBreak {
        /// One-based line number.
        line: usize,
    },
    /// A line from a schema this binary does not know.
    FromTheFuture {
        /// One-based line number.
        line: usize,
        /// The schema it claims.
        v: u32,
    },
}

/// The log, as read.
#[derive(Clone, Debug, Default)]
pub struct Journal {
    lines: Vec<Line>,
    tail: Option<Digest>,
    /// Everything wrong with the file.
    pub issues: Vec<Issue>,
    /// Every machine that has written to it. More than one is a warning: an
    /// append-only log is not a mergeable structure.
    pub hosts: BTreeSet<String>,
}

impl Journal {
    /// Read the log. A missing file is an empty log, not an error.
    ///
    /// # Errors
    ///
    /// When the file exists and cannot be read.
    pub fn load(path: &Utf8Path) -> Result<Self> {
        let text = match fs_err::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        Ok(Self::parse(&text))
    }

    /// Read a log from text.
    pub fn parse(text: &str) -> Self {
        let mut journal = Self::default();
        let mut expected = Digest::GENESIS;
        for (index, raw) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
            let number = index.saturating_add(1);
            let line = Line::read(raw);
            match &line {
                Line::Parsed(record) => {
                    if record.v > SCHEMA {
                        journal.issues.push(Issue::FromTheFuture {
                            line: number,
                            v: record.v,
                        });
                    }
                    if record.prev != expected {
                        journal.issues.push(Issue::ChainBreak { line: number });
                    }
                    journal.hosts.insert(record.host.clone());
                }
                Line::Malformed { why, .. } => journal.issues.push(Issue::Malformed {
                    line: number,
                    why: why.clone(),
                }),
            }
            expected = Digest::of(raw);
            journal.tail = Some(expected);
            journal.lines.push(line);
        }
        journal
    }

    /// Every line, parsed or not.
    pub fn lines(&self) -> impl Iterator<Item = &Line> {
        self.lines.iter()
    }

    /// How many lines the log holds, parsed or not.
    pub fn records(&self) -> usize {
        self.lines.len()
    }

    /// The digest the next record's `prev` must carry.
    pub fn tail(&self) -> Digest {
        self.tail.unwrap_or(Digest::GENESIS)
    }

    /// Whether any line came from a schema this binary does not know.
    pub fn has_future_records(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| matches!(issue, Issue::FromTheFuture { .. }))
    }

    /// Note a line that has just been written.
    fn accept(&mut self, raw: &str, line: Line) {
        self.tail = Some(Digest::of(raw));
        self.lines.push(line);
    }
}

/// A locked handle on the log, for the commands that write.
pub struct Store {
    paths: Paths,
    journal: Journal,
    _lock: relic_core::lock::Lock,
}

impl Store {
    /// Take the lock and read the log.
    ///
    /// # Errors
    ///
    /// When the tree cannot be created, the lock cannot be taken, or the log
    /// cannot be read.
    pub fn open(paths: Paths) -> Result<Self> {
        ensure_dir(&paths.log_dir)?;
        ensure_dir(&paths.state_dir)?;
        let lock =
            relic_core::lock::Lock::acquire(&paths.lock(), relic_core::lock::Wait::INTERACTIVE)?;
        let journal = Journal::load(&paths.log())?;
        Ok(Self {
            paths,
            journal,
            _lock: lock,
        })
    }

    /// The log.
    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    /// Append one record.
    ///
    /// Refuses outright while the log holds a record from a newer schema: a
    /// reading built from a half-understood log is a lie, and appending to it
    /// would make the lie durable.
    ///
    /// # Errors
    ///
    /// When the log holds a record this binary cannot read, or the write fails.
    pub fn append(&mut self, record: &Record) -> Result<()> {
        if self.journal.has_future_records() {
            return Err(anyhow!(
                "the log holds a record written by a newer rote, so this one will not add to it"
            ));
        }
        let mut record = record.clone();
        record.prev = self.journal.tail();
        let raw = crate::log::render(&record)?;
        let path = self.paths.log();
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
        self.journal.accept(&raw, Line::Parsed(Box::new(record)));
        Ok(())
    }
}

/// The verifier file: one PHC string per slug, and nothing else.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields, transparent)]
pub struct Verifiers(BTreeMap<String, Stored>);

/// One slug's verifier.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Stored {
    /// The PHC string.
    pub phc: String,
}

impl Verifiers {
    /// Read the file. A missing file is an empty set.
    ///
    /// # Errors
    ///
    /// When the file exists and will not parse.
    pub fn load(path: &Utf8Path) -> Result<Self> {
        match fs_err::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(error) => Err(error.into()),
        }
    }

    /// The verifier for a slug, if there is one.
    ///
    /// # Errors
    ///
    /// When the stored string is not a PHC string.
    pub fn get(&self, slug: &Slug) -> Result<Option<Verifier>> {
        self.0
            .get(slug.as_str())
            .map(|stored| Verifier::parse(&stored.phc))
            .transpose()
            .with_context(|| format!("reading the verifier for {slug}"))
    }

    /// Set a slug's verifier, replacing any previous one.
    ///
    /// Replacing rather than accumulating is deliberate: a retired verifier is
    /// an oracle for a retired secret, and an append-only structure could not
    /// forget it.
    pub fn set(&mut self, slug: &Slug, verifier: &Verifier) {
        self.0.insert(
            slug.as_str().to_owned(),
            Stored {
                phc: verifier.as_str().to_owned(),
            },
        );
    }

    /// Drop a slug's verifier.
    pub fn remove(&mut self, slug: &Slug) {
        self.0.remove(slug.as_str());
    }

    /// Write the file.
    ///
    /// # Errors
    ///
    /// When the tree cannot be created or the file cannot be written.
    pub fn save(&self, path: &Utf8Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            ensure_dir(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        relic_core::fs::write_atomic(path, &text).with_context(|| format!("writing {path}"))?;
        tighten(path)
    }
}

/// What the reminder reads. Derived and disposable: the schedule projected down
/// to the one question a nag asks, so the reminder never opens the log.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cache {
    /// Schema, so a stale cache from an older binary is ignored rather than
    /// misread.
    pub v: u32,
    /// The day each active slug next falls due.
    pub due: Vec<Date>,
}

impl Cache {
    /// Read the cache. Anything wrong with it reads as absent — the reminder is
    /// decoration, and decoration fails silent.
    pub fn load(path: &Utf8Path) -> Option<Self> {
        let text = fs_err::read_to_string(path).ok()?;
        let cache: Self = serde_json::from_str(&text).ok()?;
        (cache.v == SCHEMA).then_some(cache)
    }

    /// How many slugs are due on a given day.
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
        relic_core::fs::write_atomic(path, &serde_json::to_string(self)?)
            .with_context(|| format!("writing {path}"))?;
        tighten(path)
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use jiff::civil::date;
    use jiff::tz::TimeZone;

    use std::os::unix::fs::PermissionsExt as _;

    use super::{Cache, Clock, Env, Issue, Journal, Paths, Store, Verifiers, ensure_dir};
    use crate::config::Config;
    use crate::log::{Added, Digest, Event, Record, SCHEMA};
    use crate::verifier::Verifier;

    fn tree() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let base = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let paths = Paths {
            log_dir: base.join("ark"),
            state_dir: base.join("state"),
        };
        (dir, paths)
    }

    fn add(day: jiff::civil::Date, name: &str) -> Record {
        Record {
            v: SCHEMA,
            at: day.to_zoned(TimeZone::UTC).unwrap().timestamp(),
            day,
            host: "Mac".to_owned(),
            prev: Digest::GENESIS,
            event: Event::Add(Added {
                slug: name.parse().unwrap(),
                version: 1,
                critical: false,
            }),
        }
    }

    #[test]
    fn the_drill_day_follows_the_local_clock_and_the_rollover_hour() {
        let zone = TimeZone::get("Europe/Berlin").unwrap();
        // 01:30 local on the 11th belongs to the 10th's drill day.
        let late = "2026-09-10T23:30:00Z".parse().unwrap();
        assert_eq!(Clock::new(late, zone.clone(), 4).today(), date(2026, 9, 10));
        // 08:00 local on the 11th belongs to the 11th.
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
    fn the_defaults_put_the_log_in_ark_and_the_verifiers_out_of_it() {
        let env = Env {
            home: Some(Utf8PathBuf::from("/home/x")),
            ..Env::default()
        };
        let paths = Paths::resolve(&env, &Config::default()).unwrap();
        assert_eq!(paths.log_dir, "/home/x/Trove/ark/rote");
        assert_eq!(paths.state_dir, "/home/x/.local/state/rote");
        assert!(
            !paths.verifiers().starts_with(&paths.log_dir),
            "a verifier must never sit inside the tree that goes offsite"
        );
    }

    #[test]
    fn an_override_outranks_the_config_which_outranks_the_default() {
        let env = Env {
            home: Some(Utf8PathBuf::from("/home/x")),
            root: Some(Utf8PathBuf::from("/tmp/override")),
            ..Env::default()
        };
        let config = Config {
            root: Some(Utf8PathBuf::from("/tmp/config")),
            state: Some(Utf8PathBuf::from("/tmp/config-state")),
            ..Config::default()
        };
        let paths = Paths::resolve(&env, &config).unwrap();
        assert_eq!(paths.log_dir, "/tmp/override");
        assert_eq!(paths.state_dir, "/tmp/config-state");
    }

    #[test]
    fn a_created_directory_is_private_from_the_start() {
        let (_dir, paths) = tree();
        let deep = paths.log_dir.join("a").join("b");
        ensure_dir(&deep).unwrap();
        for path in [&paths.log_dir, &deep] {
            let mode = std::fs::metadata(path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o700, "{path} should be private");
        }
    }

    #[test]
    fn an_empty_tree_reads_as_an_empty_log() {
        let (_dir, paths) = tree();
        let journal = Journal::load(&paths.log()).unwrap();
        assert_eq!(journal.records(), 0);
        assert_eq!(journal.tail(), Digest::GENESIS);
        assert!(journal.issues.is_empty());
    }

    #[test]
    fn appending_chains_each_line_to_the_one_before() {
        let (_dir, paths) = tree();
        let mut store = Store::open(paths.clone()).unwrap();
        store.append(&add(date(2026, 9, 10), "a")).unwrap();
        store.append(&add(date(2026, 9, 10), "b")).unwrap();
        drop(store);

        let text = fs_err::read_to_string(paths.log()).unwrap();
        let raws: Vec<&str> = text.lines().collect();
        let journal = Journal::load(&paths.log()).unwrap();
        assert_eq!(journal.records(), 2);
        assert!(journal.issues.is_empty(), "{:?}", journal.issues);
        let second = journal.lines().nth(1).unwrap().record().unwrap();
        assert_eq!(
            second.prev,
            Digest::of(raws.first().unwrap()),
            "each record names the line before it"
        );
    }

    #[test]
    fn the_log_file_is_private() {
        let (_dir, paths) = tree();
        let mut store = Store::open(paths.clone()).unwrap();
        store.append(&add(date(2026, 9, 10), "a")).unwrap();
        let mode = std::fs::metadata(paths.log()).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn an_edited_line_shows_up_as_a_broken_chain() {
        let (_dir, paths) = tree();
        let mut store = Store::open(paths.clone()).unwrap();
        store.append(&add(date(2026, 9, 10), "a")).unwrap();
        store.append(&add(date(2026, 9, 10), "b")).unwrap();
        drop(store);

        let text = fs_err::read_to_string(paths.log()).unwrap();
        let edited = text.replacen("\"day\":\"2026-09-10\"", "\"day\":\"2026-09-09\"", 1);
        let journal = Journal::parse(&edited);
        assert_eq!(journal.issues, vec![Issue::ChainBreak { line: 2 }]);
    }

    #[test]
    fn a_malformed_line_is_reported_and_kept() {
        let journal = Journal::parse("{ not json\n");
        assert_eq!(journal.records(), 1);
        assert!(matches!(
            journal.issues.first(),
            Some(Issue::Malformed { line: 1, .. })
        ));
    }

    #[test]
    fn a_record_from_a_newer_schema_stops_the_writer() {
        let (_dir, paths) = tree();
        let mut store = Store::open(paths.clone()).unwrap();
        let mut future = add(date(2026, 9, 10), "a");
        future.v = SCHEMA + 1;
        store.append(&future).unwrap();
        drop(store);

        let journal = Journal::load(&paths.log()).unwrap();
        assert!(journal.has_future_records());
        let mut store = Store::open(paths).unwrap();
        assert!(store.append(&add(date(2026, 9, 11), "b")).is_err());
    }

    #[test]
    fn a_verifier_file_round_trips_and_is_private() {
        let (_dir, paths) = tree();
        let slug = "escrow-p".parse().unwrap();
        let phc = "$argon2id$v=19$m=64,t=1,p=1$CQkJCQkJCQkJCQkJCQkJCQ$\
                   PdWLZDvNGYPMYWPfbSZq7yZO0eFRHmiGnLTuNBAxUC0";
        let verifier = Verifier::parse(phc).unwrap();
        let mut verifiers = Verifiers::default();
        verifiers.set(&slug, &verifier);
        verifiers.save(&paths.verifiers()).unwrap();

        let mode = std::fs::metadata(paths.verifiers())
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);

        let read = Verifiers::load(&paths.verifiers()).unwrap();
        assert_eq!(read.get(&slug).unwrap().unwrap(), verifier);
    }

    #[test]
    fn removing_a_verifier_leaves_nothing_behind_to_attack() {
        let (_dir, paths) = tree();
        let slug = "a".parse().unwrap();
        let phc = "$argon2id$v=19$m=64,t=1,p=1$CQkJCQkJCQkJCQkJCQkJCQ$\
                   PdWLZDvNGYPMYWPfbSZq7yZO0eFRHmiGnLTuNBAxUC0";
        let mut verifiers = Verifiers::default();
        verifiers.set(&slug, &Verifier::parse(phc).unwrap());
        verifiers.remove(&slug);
        verifiers.save(&paths.verifiers()).unwrap();
        let text = fs_err::read_to_string(paths.verifiers()).unwrap();
        assert!(!text.contains("argon2id"));
    }

    #[test]
    fn the_cache_carries_counts_and_no_names() {
        let (_dir, paths) = tree();
        let cache = Cache {
            v: SCHEMA,
            due: vec![date(2026, 9, 10), date(2026, 9, 20)],
        };
        cache.save(&paths.cache()).unwrap();
        let text = fs_err::read_to_string(paths.cache()).unwrap();
        assert!(!text.contains("escrow"));

        let read = Cache::load(&paths.cache()).unwrap();
        assert_eq!(read.due_by(date(2026, 9, 10)), 1);
        assert_eq!(read.due_by(date(2026, 9, 21)), 2);
        assert_eq!(read.due_by(date(2026, 9, 9)), 0);
    }

    #[test]
    fn a_cache_from_another_schema_reads_as_absent() {
        let (_dir, paths) = tree();
        let cache = Cache {
            v: SCHEMA + 1,
            due: vec![date(2026, 9, 10)],
        };
        cache.save(&paths.cache()).unwrap();
        assert!(Cache::load(&paths.cache()).is_none());
        assert!(Cache::load(&paths.log_dir.join("nothing.json")).is_none());
    }
}
