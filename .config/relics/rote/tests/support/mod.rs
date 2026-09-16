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
use jiff::Timestamp;
use jiff::civil::Date;
use proptest::prelude::*;
use rote::corpus::Corpus;
use rote::corpus::record::{
    Attached, Digest, EngramId, Enrolled, Event, Outcome, Record, Retired, Rotated, SCHEMA, render,
};
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

    pub fn stamp_path(&self) -> Utf8PathBuf {
        self.state.join("stamp")
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
            // One instant for the harness and the binary both.
            .env("ROTE_NOW", now().to_string())
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
            host: "Scratch".to_owned(),
            prev: self.prev,
            event,
        };
        let line = render(&record).expect("a line");
        self.prev = Digest::of(&line);
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

    /// Supersede whatever a lineage holds with a fresh engram.
    pub fn rotate(&mut self, day: Date, name: &str) -> EngramId {
        let to = EngramId::mint().expect("an engram");
        self.append(
            day,
            Event::Rotate(Rotated {
                slug: slug(name),
                to,
            }),
        );
        self.hold(to, name, day);
        to
    }

    /// Record that a verifier was made here.
    pub fn attach(&mut self, day: Date, name: &str, engram: EngramId) {
        self.append(day, Event::Attach(Attached { engram }));
        self.hold(engram, name, day);
    }

    /// Take a lineage off the schedule.
    pub fn retire(&mut self, day: Date, name: &str) {
        self.append(day, Event::Retire(Retired { slug: slug(name) }));
    }

    /// One drill, written the way the sitting writes one.
    pub fn drill(&mut self, day: Date, engram: EngramId, drill: Drilled) {
        let Drilled {
            outcome,
            recovered,
            aided,
        } = drill;
        self.append(
            day,
            Event::Drill(rote::corpus::record::Drilled {
                engram,
                outcome,
                ttfk_ms: Some(900),
                follow_ups: u16::from(recovered),
                recovered,
                aided,
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
    /// How the cold capture was judged.
    pub outcome: Outcome,
    /// Whether a follow-up passed.
    pub recovered: bool,
    /// Whether the answer was looked up.
    pub aided: bool,
}

impl Drilled {
    /// A cold pass.
    pub fn passed() -> Self {
        Self {
            outcome: Outcome::Pass,
            recovered: false,
            aided: false,
        }
    }

    /// A cold fail, and nothing after it.
    pub fn failed() -> Self {
        Self {
            outcome: Outcome::Fail,
            recovered: false,
            aided: false,
        }
    }

    /// A cold fail that a follow-up got past.
    pub fn recovered() -> Self {
        Self {
            outcome: Outcome::Fail,
            recovered: true,
            aided: false,
        }
    }

    /// A cold fail recovered with the answer looked up.
    pub fn aided() -> Self {
        Self {
            outcome: Outcome::Fail,
            recovered: true,
            aided: true,
        }
    }
}

/// What a generated step does. A small alphabet: the point is the interleaving,
/// not the variety.
#[derive(Clone, Copy, Debug)]
pub enum Step {
    Drill {
        pass: bool,
        recovered: bool,
        aided: bool,
    },
    Attach,
    Rotate,
    /// A second enrolment over a lineage that is already live, which one
    /// machine never writes and two machines can.
    Enroll,
}

pub fn steps() -> impl Strategy<Value = Vec<(u8, u8, Step)>> {
    let step = prop_oneof![
        6 => (any::<bool>(), any::<bool>(), any::<bool>())
            .prop_map(|(pass, recovered, aided)| Step::Drill { pass, recovered, aided }),
        2 => Just(Step::Attach),
        1 => Just(Step::Rotate),
        1 => Just(Step::Enroll),
    ];
    // (machine 0..2, lineage 0..2, step)
    proptest::collection::vec((0u8..2, 0u8..2, step), 0..24)
}

/// One generated record, with what the chain file would say about it: which
/// machine's file it sits in and its line index there. Neither is written into
/// the record, so the fixture carries them beside it.
#[derive(Clone, Debug)]
pub struct Seeded {
    pub machine: MachineId,
    pub position: usize,
    pub record: Record,
}

impl Seeded {
    /// The merge key, as `chain::Placed::order` derives it.
    pub fn key(&self) -> (Timestamp, &MachineId, usize) {
        (self.record.at, &self.machine, self.position)
    }
}

pub struct World {
    records: Vec<Seeded>,
    written: BTreeMap<MachineId, usize>,
    prev: BTreeMap<MachineId, Digest>,
    current: BTreeMap<u8, EngramId>,
    day: Date,
}

impl World {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            written: BTreeMap::new(),
            prev: BTreeMap::new(),
            current: BTreeMap::new(),
            day: day(2026, 1, 1),
        }
    }

    fn name(lineage: u8) -> String {
        format!("lineage-{lineage}")
    }

    fn push(&mut self, machine: &MachineId, event: Event) {
        let position = self.written.entry(machine.clone()).or_insert(0);
        let at = self
            .day
            .to_zoned(jiff::tz::TimeZone::UTC)
            .expect("a zoned day")
            .timestamp();
        let record = Record {
            v: SCHEMA,
            at,
            day: self.day,
            host: "Scratch".to_owned(),
            prev: self.prev.get(machine).copied().unwrap_or(Digest::GENESIS),
            event,
        };
        self.records.push(Seeded {
            machine: machine.clone(),
            position: *position,
            record: record.clone(),
        });
        *position = position.saturating_add(1);
        self.prev.insert(
            machine.clone(),
            Digest::of(&rote::corpus::record::render(&record).expect("a line")),
        );
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

    pub fn build(script: &[(u8, u8, Step)]) -> Vec<Seeded> {
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
                Step::Attach => world.push(&machine, Event::Attach(Attached { engram })),
                Step::Enroll => {
                    let again = EngramId::mint().expect("an engram");
                    world.current.insert(*lineage, again);
                    world.push(
                        &machine,
                        Event::Enroll(Enrolled {
                            slug,
                            engram: again,
                            critical: *lineage == 0,
                        }),
                    );
                }
                Step::Rotate => {
                    let to = EngramId::mint().expect("an engram");
                    world.current.insert(*lineage, to);
                    world.push(
                        &machine,
                        Event::Rotate(rote::corpus::record::Rotated { slug, to }),
                    );
                }
                Step::Drill {
                    pass,
                    recovered,
                    aided,
                } => {
                    let outcome = if *pass { Outcome::Pass } else { Outcome::Fail };
                    // A follow-up only exists after a fail.
                    let recovered = *recovered && !*pass;
                    world.push(
                        &machine,
                        Event::Drill(rote::corpus::record::Drilled {
                            engram,
                            outcome,
                            ttfk_ms: Some(900),
                            follow_ups: u16::from(recovered),
                            recovered,
                            aided: *aided && !*pass,
                        }),
                    );
                }
            }
        }
        world.records
    }
}

/// The merge order, as the corpus applies it: instant, then machine, then the
/// line's position in that machine's file.
pub fn merged(records: &[Seeded]) -> Vec<&Record> {
    let mut out: Vec<&Seeded> = records.iter().collect();
    out.sort_by(|a, b| a.key().cmp(&b.key()));
    out.into_iter().map(|seeded| &seeded.record).collect()
}

/// A reading small enough to compare, and wide enough to catch a difference.
pub fn shape(corpus: &Corpus) -> Vec<(String, u8, String, String)> {
    let mut out = Vec::new();
    for lineage in corpus.lineages() {
        for dossier in &lineage.engrams {
            out.push((
                lineage.slug.to_string(),
                dossier.rung.get(),
                dossier.anchor.to_string(),
                dossier.last_exposed.to_string(),
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

/// The instant this test process runs at, taken once and never again.
///
/// The harness and the binary each asking the wall clock what day it is are
/// two answers wherever a run straddles the rollover hour — a real disagreement
/// for a few minutes every morning, and one nobody would ever reproduce. Taken
/// once here and handed to the binary through `ROTE_NOW`, it is one answer by
/// construction.
pub fn now() -> jiff::Timestamp {
    static NOW: std::sync::OnceLock<jiff::Timestamp> = std::sync::OnceLock::new();
    *NOW.get_or_init(jiff::Timestamp::now)
}

/// The drill day the binary will believe it is, computed the way it does.
pub fn today() -> Date {
    rote::store::Clock::new(
        now(),
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
