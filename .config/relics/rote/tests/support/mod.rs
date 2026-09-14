//! A machine, as a fixture.
//!
//! The suite seeds a corpus through the crate's own types rather than through
//! JSON literals. A literal drifts from the wire form the moment either
//! changes, and the constants it would have to duplicate — the schema, the
//! argon2 cost — are exactly the ones a test must not restate.

// Scaffolding, not production code. `clippy.toml` carves the restriction lints
// out of tests, but only inside a #[test] function — a fixture module shared by
// three test binaries sits outside that, and the alternative is threading a
// Result through every helper so a temp directory can fail to be made.
#![allow(
    dead_code,
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::panic,
    clippy::indexing_slicing
)]

use std::collections::BTreeMap;
use std::sync::OnceLock;

use assert_cmd::Command;
use camino::Utf8PathBuf;
use jiff::civil::Date;
use proptest::prelude::*;
use rote::corpus::Corpus;
use rote::corpus::record::{
    Attached, Captured, Digest, EngramId, Enrolled, Event, Outcome, Record, Retired, Rotated,
    SCHEMA, SittingId, render,
};
use rote::ladder::Occasion;
use rote::machine::MachineId;
use rote::secret::Secret;
use rote::slug::Slug;
use rote::verifier::Verifier;
use rote::verifier::file::Verifiers;

/// The secret every seeded verifier holds.
pub const SECRET: &str = "correct horse battery staple";

/// One verifier at the shipped parameters, minted once for the whole run.
fn shipped() -> &'static Verifier {
    static ONCE: OnceLock<Verifier> = OnceLock::new();
    ONCE.get_or_init(|| {
        let secret = Secret::from_bytes(SECRET.as_bytes()).expect("a secret");
        Verifier::create(&secret, &[9u8; rote::verifier::SALT_LEN]).expect("a verifier")
    })
}

/// One verifier below the memory floor, for the check that reports it.
fn weak() -> &'static str {
    "$argon2id$v=19$m=64,t=1,p=1$CQkJCQkJCQkJCQkJCQkJCQ$\
     PdWLZDvNGYPMYWPfbSZq7yZO0eFRHmiGnLTuNBAxUC0"
}

/// A machine with its own trees, its own identity, and its own chain.
pub struct Rote {
    _dir: tempfile::TempDir,
    pub ark: Utf8PathBuf,
    pub state: Utf8PathBuf,
    pub home: Utf8PathBuf,
    pub marker: Utf8PathBuf,
    pub config: Utf8PathBuf,
    pub machine: MachineId,
    prev: Digest,
    seq: u64,
}

impl Rote {
    /// A fresh flagship with nothing enrolled.
    pub fn new() -> Self {
        Self::named("one")
    }

    /// A fresh flagship whose identity is derived from `seed`, so a test can be
    /// two machines at once.
    pub fn named(seed: &str) -> Self {
        let dir = tempfile::tempdir().expect("a temp dir");
        // macOS puts /tmp behind a symlink and the binary prints resolved paths.
        let base = Utf8PathBuf::from_path_buf(dir.path().canonicalize().expect("a real path"))
            .expect("utf8");
        let rote = Self {
            ark: base.join("ark"),
            state: base.join("state"),
            home: base.join("home"),
            marker: base.join("flagship"),
            config: base.join("config.toml"),
            machine: MachineId::of(seed),
            prev: Digest::GENESIS,
            seq: 0,
            _dir: dir,
        };
        std::fs::create_dir_all(rote.chains()).expect("the chains dir");
        std::fs::create_dir_all(&rote.state).expect("the state dir");
        std::fs::create_dir_all(&rote.home).expect("a home");
        std::fs::write(&rote.marker, "").expect("the marker");
        rote
    }

    /// Share another machine's trees, so both write into one corpus.
    pub fn beside(other: &Self, seed: &str) -> Self {
        Self {
            _dir: tempfile::tempdir().expect("a temp dir"),
            ark: other.ark.clone(),
            state: other.state.clone(),
            home: other.home.clone(),
            marker: other.marker.clone(),
            config: other.config.clone(),
            machine: MachineId::of(seed),
            prev: Digest::GENESIS,
            seq: 0,
        }
    }

    pub fn chains(&self) -> Utf8PathBuf {
        self.ark.join("chains")
    }

    pub fn chain(&self) -> Utf8PathBuf {
        self.chains().join(format!("{}.jsonl", self.machine))
    }

    pub fn verifiers_path(&self) -> Utf8PathBuf {
        self.state.join("verifiers.toml")
    }

    pub fn cache_path(&self) -> Utf8PathBuf {
        self.state.join("cache.json")
    }

    /// Take the flagship marker away.
    pub fn demote(&self) {
        let _ = std::fs::remove_file(&self.marker);
    }

    /// Name another machine in the marker.
    pub fn marker_names(&self, other: &MachineId) {
        std::fs::write(&self.marker, format!("{other}\n")).expect("the marker");
    }

    /// Write a config file.
    pub fn configure(&self, body: &str) {
        std::fs::write(&self.config, body).expect("a config");
    }

    /// The binary, wired to this machine.
    pub fn cmd(&self, args: &[&str]) -> Command {
        let mut command = Command::cargo_bin("rote").expect("the binary");
        command
            .args(args)
            .env("ROTE_ROOT", self.ark.as_str())
            .env("ROTE_STATE", self.state.as_str())
            .env("ROTE_CONFIG", self.config.as_str())
            .env("ROTE_FLAGSHIP", self.marker.as_str())
            .env("ROTE_MACHINE", self.machine.to_string())
            .env("ROTE_HOST", "Scratch")
            .env("HOME", self.home.as_str())
            // Pin the shape so assertions do not depend on a tty.
            .env("CLAUDECODE", "1")
            .env_remove("ROTE_UI")
            .env_remove("NO_COLOR");
        command
    }

    /// Append one event to this machine's chain.
    pub fn append(&mut self, day: Date, event: Event) {
        let record = Record {
            v: SCHEMA,
            at: day
                .to_zoned(jiff::tz::TimeZone::UTC)
                .expect("a zoned day")
                .timestamp(),
            day,
            machine: self.machine.clone(),
            host: "Scratch".to_owned(),
            seq: self.seq,
            prev: self.prev,
            event,
        };
        let line = render(&record).expect("a line");
        self.prev = Digest::of(&line);
        self.seq = self.seq.saturating_add(1);
        let path = self.chain();
        let mut text = std::fs::read_to_string(&path).unwrap_or_default();
        text.push_str(&line);
        text.push('\n');
        std::fs::write(&path, text).expect("the chain");
    }

    /// Open a lineage, and hold a verifier for it here.
    pub fn enroll(&mut self, day: Date, name: &str, critical: bool) -> EngramId {
        let engram = EngramId::mint().expect("an engram");
        self.append(
            day,
            Event::Enroll(Enrolled {
                slug: slug(name),
                engram,
                critical,
            }),
        );
        self.hold(engram, name, day);
        engram
    }

    /// Open a lineage without a verifier here: what a restore looks like.
    pub fn enroll_dormant(&mut self, day: Date, name: &str) -> EngramId {
        let engram = EngramId::mint().expect("an engram");
        self.append(
            day,
            Event::Enroll(Enrolled {
                slug: slug(name),
                engram,
                critical: false,
            }),
        );
        engram
    }

    /// Supersede one engram with the next.
    pub fn rotate(&mut self, day: Date, name: &str, from: EngramId, proved: bool) -> EngramId {
        let to = EngramId::mint().expect("an engram");
        self.append(
            day,
            Event::Rotate(Rotated {
                slug: slug(name),
                from,
                to,
                proved,
            }),
        );
        self.hold(to, name, day);
        to
    }

    /// Record that a verifier was made here.
    pub fn attach(&mut self, day: Date, name: &str, engram: EngramId) {
        self.append(
            day,
            Event::Attach(Attached {
                slug: slug(name),
                engram,
            }),
        );
        self.hold(engram, name, day);
    }

    /// Take a lineage off the schedule.
    pub fn retire(&mut self, day: Date, name: &str) {
        self.append(day, Event::Retire(Retired { slug: slug(name) }));
    }

    /// One drill, as one first sample, a day after the last exposure.
    pub fn capture(&mut self, day: Date, name: &str, engram: EngramId, drill: Drilled) {
        let Drilled {
            occasion,
            aided,
            outcome,
            rung_after,
        } = drill;
        self.append(
            day,
            Event::Capture(Captured {
                slug: slug(name),
                engram,
                sitting: SittingId::mint().expect("a sitting"),
                ordinal: 1,
                occasion,
                aided,
                outcome,
                ttfk_ms: Some(900),
                total_ms: Some(3_000),
                corrections: 0,
                paste_accepted: 0,
                paste_refused: 0,
                scheduled_interval_days: 1,
                actual_interval_days: 1,
                effective_interval_days: 1,
                rung_before: 0,
                rung_after,
            }),
        );
    }

    /// Put a verifier for an engram on this machine.
    pub fn hold(&self, engram: EngramId, name: &str, day: Date) {
        let mut file = Verifiers::load(&self.verifiers_path()).expect("the verifier file");
        file.set(engram, &slug(name), day, shipped());
        file.save(&self.verifiers_path())
            .expect("the verifier file");
    }

    /// Put a verifier below the memory floor on this machine.
    pub fn hold_weak(&self, engram: EngramId, name: &str, day: Date) {
        let mut file = Verifiers::load(&self.verifiers_path()).expect("the verifier file");
        file.set(
            engram,
            &slug(name),
            day,
            &Verifier::parse(weak()).expect("a weak verifier"),
        );
        file.save(&self.verifiers_path())
            .expect("the verifier file");
    }

    /// Take a verifier away, which is what a restore onto a new machine does.
    pub fn drop_verifiers(&self) {
        let _ = std::fs::remove_file(self.verifiers_path());
    }
}

/// What one seeded drill was.
#[derive(Clone, Copy, Debug)]
pub struct Drilled {
    /// Whether the schedule asked.
    pub occasion: Occasion,
    /// Whether the answer was consulted.
    pub aided: bool,
    /// How it ended.
    pub outcome: Outcome,
    /// Where it leaves the ladder.
    pub rung_after: u8,
}

impl Drilled {
    /// An unaided review that passed.
    pub fn passed() -> Self {
        Self {
            occasion: Occasion::Review,
            aided: false,
            outcome: Outcome::Pass,
            rung_after: 1,
        }
    }

    /// An unaided review that missed.
    pub fn missed() -> Self {
        Self {
            occasion: Occasion::Review,
            aided: false,
            outcome: Outcome::Fail,
            rung_after: 0,
        }
    }

    /// One taken with the answer in front of the person.
    pub fn aided(outcome: Outcome) -> Self {
        Self {
            occasion: Occasion::Review,
            aided: true,
            outcome,
            rung_after: 0,
        }
    }
}

/// What a generated step does. A small alphabet: the point is the interleaving,
/// not the variety.
#[derive(Clone, Copy, Debug)]
pub enum Step {
    Drill {
        pass: bool,
        aided: bool,
        review: bool,
    },
    Attach,
    Rotate,
}

pub fn steps() -> impl Strategy<Value = Vec<(u8, u8, Step)>> {
    let step = prop_oneof![
        6 => (any::<bool>(), any::<bool>(), any::<bool>())
            .prop_map(|(pass, aided, review)| Step::Drill { pass, aided, review }),
        2 => Just(Step::Attach),
        1 => Just(Step::Rotate),
    ];
    // (machine 0..2, lineage 0..2, step)
    proptest::collection::vec((0u8..2, 0u8..2, step), 0..24)
}

pub struct World {
    records: Vec<Record>,
    seq: BTreeMap<MachineId, u64>,
    prev: BTreeMap<MachineId, Digest>,
    current: BTreeMap<u8, EngramId>,
    day: Date,
}

impl World {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            seq: BTreeMap::new(),
            prev: BTreeMap::new(),
            current: BTreeMap::new(),
            day: day(2026, 1, 1),
        }
    }

    fn name(lineage: u8) -> String {
        format!("lineage-{lineage}")
    }

    fn push(&mut self, machine: &MachineId, event: Event) {
        let seq = self.seq.entry(machine.clone()).or_insert(0);
        let at = self
            .day
            .to_zoned(jiff::tz::TimeZone::UTC)
            .expect("a zoned day")
            .timestamp();
        let record = Record {
            v: SCHEMA,
            at,
            day: self.day,
            machine: machine.clone(),
            host: "Scratch".to_owned(),
            seq: *seq,
            prev: self.prev.get(machine).copied().unwrap_or(Digest::GENESIS),
            event,
        };
        *seq = seq.saturating_add(1);
        self.prev.insert(
            machine.clone(),
            Digest::of(&rote::corpus::record::render(&record).expect("a line")),
        );
        self.records.push(record);
        self.day = self
            .day
            .checked_add(jiff::Span::new().days(1))
            .unwrap_or(self.day);
    }

    fn engram(&mut self, machine: &MachineId, lineage: u8) -> EngramId {
        if let Some(engram) = self.current.get(&lineage) {
            return *engram;
        }
        let engram = EngramId::mint().expect("an engram");
        self.current.insert(lineage, engram);
        self.push(
            machine,
            Event::Enroll(Enrolled {
                slug: Self::name(lineage).parse().expect("a slug"),
                engram,
                critical: lineage == 0,
            }),
        );
        engram
    }

    pub fn build(script: &[(u8, u8, Step)]) -> Vec<Record> {
        let mut world = Self::new();
        let machines: Vec<MachineId> = (0..2).map(|n| MachineId::of(&format!("m{n}"))).collect();
        for (which, lineage, step) in script {
            let Some(machine) = machines.get(usize::from(*which)) else {
                continue;
            };
            let machine = machine.clone();
            let engram = world.engram(&machine, *lineage);
            let slug = Self::name(*lineage).parse().expect("a slug");
            match step {
                Step::Attach => world.push(&machine, Event::Attach(Attached { slug, engram })),
                Step::Rotate => {
                    let to = EngramId::mint().expect("an engram");
                    world.current.insert(*lineage, to);
                    world.push(
                        &machine,
                        Event::Rotate(rote::corpus::record::Rotated {
                            slug,
                            from: engram,
                            to,
                            proved: true,
                        }),
                    );
                }
                Step::Drill {
                    pass,
                    aided,
                    review,
                } => {
                    let outcome = if *pass { Outcome::Pass } else { Outcome::Fail };
                    let occasion = if *review {
                        Occasion::Review
                    } else {
                        Occasion::Practice
                    };
                    world.push(
                        &machine,
                        Event::Capture(Captured {
                            slug,
                            engram,
                            sitting: SittingId::mint().expect("a sitting"),
                            ordinal: 1,
                            occasion,
                            aided: *aided,
                            outcome,
                            ttfk_ms: Some(900),
                            total_ms: Some(3_000),
                            corrections: 0,
                            paste_accepted: 0,
                            paste_refused: 0,
                            scheduled_interval_days: 30,
                            actual_interval_days: 30,
                            effective_interval_days: 30,
                            rung_before: 0,
                            rung_after: u8::from(*pass),
                        }),
                    );
                }
            }
        }
        world.records
    }
}

/// The merge order, as the corpus applies it.
pub fn merged(records: &[Record]) -> Vec<&Record> {
    let mut out: Vec<&Record> = records.iter().collect();
    out.sort_by(|a, b| a.order().cmp(&b.order()));
    out
}

/// A reading small enough to compare, and wide enough to catch a difference.
pub fn shape(corpus: &Corpus) -> Vec<(String, u8, String, String, usize)> {
    let mut out = Vec::new();
    for lineage in corpus.lineages() {
        for dossier in &lineage.engrams {
            out.push((
                lineage.slug.to_string(),
                dossier.rung.get(),
                dossier.anchor.to_string(),
                dossier.last_exposed.to_string(),
                usize::from(dossier.aided_mismatch),
            ));
        }
    }
    out
}

pub fn slug(name: &str) -> Slug {
    name.parse().expect("a slug")
}

/// A day, so tests read as dates rather than as arithmetic.
pub fn day(year: i16, month: i8, date: i8) -> Date {
    jiff::civil::date(year, month, date)
}

/// The drill day the binary will believe it is, computed the way it does.
pub fn today() -> Date {
    rote::store::Clock::new(
        jiff::Timestamp::now(),
        jiff::tz::TimeZone::system(),
        rote::config::DEFAULT_ROLLOVER_HOUR,
    )
    .today()
}

/// A drill day this many days before today.
pub fn days_ago(days: i64) -> Date {
    today()
        .checked_sub(jiff::Span::new().days(days))
        .unwrap_or_else(|_| today())
}
