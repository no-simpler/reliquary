//! Chains: one append-only file per machine, and the merge across them.
//!
//! **Never merge a chain; sequence chains.** A hash chain is not a mergeable
//! structure, so two machines never share one file. Each writes its own, and the
//! corpus is the deterministic merge of all of them ordered by `(at, machine,
//! seq)`. A flagship handover is then a new file rather than a fork in an old
//! one.
//!
//! The filename grammar is load-bearing rather than cosmetic. A conflict copy
//! left beside a chain — `<id> (1).jsonl` from a file sync, `<id>.jsonl.orig`
//! from an editor — would otherwise be read as a second chain and double every
//! event in the corpus. Only `<machine>.jsonl` is a chain; anything else visible
//! is reported and excluded. The doctrine that a malformed **line** is kept
//! rather than dropped does not extend to files.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use camino::Utf8Path;
use jiff::Timestamp;

use super::record::{Digest, Line, Record, SCHEMA};
use crate::machine::MachineId;

/// The extension every chain wears.
pub const EXTENSION: &str = "jsonl";

/// The file one machine writes.
pub fn file_name(machine: &MachineId) -> String {
    format!("{machine}.{EXTENSION}")
}

/// The machine a filename names, when it names one at all.
pub fn machine_of(file_name: &str) -> Option<MachineId> {
    let stem = file_name.strip_suffix(&format!(".{EXTENSION}"))?;
    MachineId::try_from(stem.to_owned()).ok()
}

/// Something wrong with the record itself, as opposed to with a drill.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Issue {
    /// A line that would not parse. It is kept, never dropped.
    Malformed {
        /// Whose chain.
        machine: MachineId,
        /// One-based line number.
        line: usize,
        /// What the parser said.
        why: String,
    },
    /// A line whose `prev` does not match the line before it.
    ChainBreak {
        /// Whose chain.
        machine: MachineId,
        /// One-based line number.
        line: usize,
    },
    /// A line whose position does not follow the one before it, which is how a
    /// truncated head becomes visible.
    SeqBreak {
        /// Whose chain.
        machine: MachineId,
        /// One-based line number.
        line: usize,
        /// The position the chain was at.
        expected: u64,
        /// The position the line claims.
        found: u64,
    },
    /// A line from a schema this binary does not know.
    FromTheFuture {
        /// Whose chain.
        machine: MachineId,
        /// One-based line number.
        line: usize,
        /// The schema it claims.
        v: u32,
    },
    /// A record that names a machine other than the file it sits in. The
    /// filename is the authority.
    MachineMismatch {
        /// The file.
        file: String,
        /// What the record claims.
        claimed: MachineId,
    },
    /// A file in the chains directory that is not a chain. Its records are
    /// excluded, because reading a conflict copy would double the corpus.
    Foreign {
        /// The file.
        file: String,
    },
}

impl Issue {
    /// Whether this leaves the corpus unreadable rather than merely suspect.
    pub fn breaks_the_record(&self) -> bool {
        match self {
            Self::Malformed { .. }
            | Self::ChainBreak { .. }
            | Self::FromTheFuture { .. }
            | Self::MachineMismatch { .. }
            | Self::Foreign { .. } => true,
            Self::SeqBreak { .. } => false,
        }
    }
}

/// One line, with what it takes to place it in the merge.
#[derive(Clone, Debug)]
pub struct Placed {
    /// The line.
    pub line: Line,
    /// Whose chain it came from.
    pub machine: MachineId,
    /// When, for the merge. A malformed line inherits the instant of the last
    /// parsed line before it, so it sorts where it actually lives.
    pub at: Timestamp,
    /// Position, for the merge.
    pub seq: u64,
}

impl Placed {
    fn order(&self) -> (Timestamp, &MachineId, u64) {
        (self.at, &self.machine, self.seq)
    }
}

/// One machine's append-only file.
#[derive(Clone, Debug)]
pub struct Chain {
    machine: MachineId,
    placed: Vec<Placed>,
    tail: Digest,
    next_seq: u64,
}

impl Chain {
    /// Read one chain from text.
    pub fn parse(machine: &MachineId, text: &str, issues: &mut Vec<Issue>) -> Self {
        let mut chain = Self {
            machine: machine.clone(),
            placed: Vec::new(),
            tail: Digest::GENESIS,
            next_seq: 0,
        };
        let mut expected_prev = Digest::GENESIS;
        let mut expected_seq = 0u64;
        let mut last_at: Option<Timestamp> = None;

        for (index, raw) in text.lines().filter(|l| !l.trim().is_empty()).enumerate() {
            let number = index.saturating_add(1);
            let line = Line::read(raw);
            let mut at = last_at.unwrap_or(Timestamp::UNIX_EPOCH);
            let mut seq = expected_seq;
            match &line {
                Line::Parsed(record) => {
                    if record.v > SCHEMA {
                        issues.push(Issue::FromTheFuture {
                            machine: machine.clone(),
                            line: number,
                            v: record.v,
                        });
                    }
                    if record.prev != expected_prev {
                        issues.push(Issue::ChainBreak {
                            machine: machine.clone(),
                            line: number,
                        });
                    }
                    if record.seq != expected_seq {
                        issues.push(Issue::SeqBreak {
                            machine: machine.clone(),
                            line: number,
                            expected: expected_seq,
                            found: record.seq,
                        });
                    }
                    if record.machine != *machine {
                        issues.push(Issue::MachineMismatch {
                            file: file_name(machine),
                            claimed: record.machine.clone(),
                        });
                    }
                    at = record.at;
                    seq = record.seq;
                    last_at = Some(record.at);
                    expected_seq = record.seq.saturating_add(1);
                }
                Line::Malformed { why, .. } => {
                    issues.push(Issue::Malformed {
                        machine: machine.clone(),
                        line: number,
                        why: why.clone(),
                    });
                    expected_seq = expected_seq.saturating_add(1);
                }
            }
            expected_prev = Digest::of(raw);
            chain.tail = expected_prev;
            chain.next_seq = expected_seq;
            chain.placed.push(Placed {
                line,
                machine: machine.clone(),
                at,
                seq,
            });
        }
        chain
    }

    /// The machine that writes it.
    pub fn machine(&self) -> &MachineId {
        &self.machine
    }

    /// The digest the next record's `prev` must carry.
    pub fn tail(&self) -> Digest {
        self.tail
    }

    /// The position the next record takes.
    pub fn next_seq(&self) -> u64 {
        self.next_seq
    }

    /// How many lines it holds, parsed or not.
    pub fn len(&self) -> usize {
        self.placed.len()
    }

    /// Whether it holds nothing.
    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }

    /// Note a line that has just been written.
    fn accept(&mut self, raw: &str, record: &Record) {
        self.tail = Digest::of(raw);
        self.next_seq = record.seq.saturating_add(1);
        self.placed.push(Placed {
            line: Line::Parsed(Box::new(record.clone())),
            machine: self.machine.clone(),
            at: record.at,
            seq: record.seq,
        });
    }
}

/// Every chain in the corpus tree.
#[derive(Clone, Debug, Default)]
pub struct Chains {
    chains: BTreeMap<MachineId, Chain>,
    /// Everything wrong with the files.
    pub issues: Vec<Issue>,
}

impl Chains {
    /// Read every chain under a directory. A missing directory is an empty
    /// corpus, not an error.
    ///
    /// # Errors
    ///
    /// When the directory exists and cannot be walked, or a chain cannot be
    /// read.
    pub fn load(dir: &Utf8Path) -> Result<Self> {
        let mut chains = Self::default();
        let entries = match fs_err::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(chains),
            Err(error) => {
                return Err(error).with_context(|| format!("reading {dir}"));
            }
        };
        let mut found: Vec<(MachineId, String)> = Vec::new();
        for entry in entries {
            let entry = entry.with_context(|| format!("reading {dir}"))?;
            let name = entry.file_name().to_string_lossy().into_owned();
            // Hidden files are the platform's litter, not a claim to be a chain.
            if name.starts_with('.') {
                continue;
            }
            match machine_of(&name) {
                Some(machine) => found.push((machine, name)),
                None => chains.issues.push(Issue::Foreign { file: name }),
            }
        }
        found.sort();
        for (machine, name) in found {
            let text = fs_err::read_to_string(dir.join(&name))
                .with_context(|| format!("reading {name}"))?;
            let chain = Chain::parse(&machine, &text, &mut chains.issues);
            chains.chains.insert(machine, chain);
        }
        Ok(chains)
    }

    /// One machine's chain, if the corpus holds one.
    pub fn get(&self, machine: &MachineId) -> Option<&Chain> {
        self.chains.get(machine)
    }

    /// Every machine that has written, in name order.
    pub fn machines(&self) -> impl Iterator<Item = &MachineId> {
        self.chains.keys()
    }

    /// How many lines the whole corpus holds.
    pub fn len(&self) -> usize {
        self.chains.values().map(Chain::len).sum()
    }

    /// Whether the corpus holds nothing at all.
    pub fn is_empty(&self) -> bool {
        self.chains.values().all(Chain::is_empty)
    }

    /// Every line across every chain, in merged order.
    pub fn placed(&self) -> Vec<&Placed> {
        let mut all: Vec<&Placed> = self.chains.values().flat_map(|c| c.placed.iter()).collect();
        all.sort_by(|a, b| a.order().cmp(&b.order()));
        all
    }

    /// Every readable record, in merged order.
    pub fn records(&self) -> Vec<&Record> {
        self.placed()
            .into_iter()
            .filter_map(|placed| placed.line.record())
            .collect()
    }

    /// Whether any line came from a schema this binary does not know.
    pub fn has_future_records(&self) -> bool {
        self.issues
            .iter()
            .any(|issue| matches!(issue, Issue::FromTheFuture { .. }))
    }

    /// Note a line that has just been written to one machine's chain.
    pub fn accept(&mut self, machine: &MachineId, raw: &str, record: &Record) {
        self.chains
            .entry(machine.clone())
            .or_insert_with(|| Chain {
                machine: machine.clone(),
                placed: Vec::new(),
                tail: Digest::GENESIS,
                next_seq: 0,
            })
            .accept(raw, record);
    }

    /// What the next record on this machine must carry.
    pub fn head(&self, machine: &MachineId) -> (Digest, u64) {
        self.get(machine).map_or((Digest::GENESIS, 0), |chain| {
            (chain.tail(), chain.next_seq())
        })
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Chain, Chains, Issue, file_name, machine_of};
    use crate::corpus::record::{Digest, EngramId, Enrolled, Event, Record, SCHEMA, render};
    use crate::machine::MachineId;

    fn record(machine: &MachineId, seq: u64, prev: Digest, day: Date, name: &str) -> Record {
        Record {
            v: SCHEMA,
            at: day.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp(),
            day,
            machine: machine.clone(),
            host: "Mac".to_owned(),
            seq,
            prev,
            event: Event::Enroll(Enrolled {
                slug: name.parse().unwrap(),
                engram: EngramId::mint().unwrap(),
                critical: false,
            }),
        }
    }

    /// Build a well-formed chain body from names, one record per name.
    fn body(machine: &MachineId, names: &[&str]) -> String {
        let mut text = String::new();
        let mut prev = Digest::GENESIS;
        for (index, name) in names.iter().enumerate() {
            let day = date(2026, 9, 10);
            let seq = u64::try_from(index).unwrap();
            let line = render(&record(machine, seq, prev, day, name)).unwrap();
            prev = Digest::of(&line);
            text.push_str(&line);
            text.push('\n');
        }
        text
    }

    #[test]
    fn only_a_machine_identity_names_a_chain() {
        let machine = MachineId::of("a");
        assert_eq!(machine_of(&file_name(&machine)), Some(machine.clone()));
        for impostor in [
            "log.jsonl",
            ".DS_Store",
            "not-hex-at-all.jsonl",
            &format!("{machine}.jsonl.orig"),
            &format!("{machine} (1).jsonl"),
            &format!("{machine}"),
        ] {
            assert_eq!(machine_of(impostor), None, "{impostor} is not a chain");
        }
    }

    #[test]
    fn a_well_formed_chain_reports_nothing() {
        let machine = MachineId::of("a");
        let mut issues = Vec::new();
        let chain = Chain::parse(&machine, &body(&machine, &["a", "b"]), &mut issues);
        assert!(issues.is_empty(), "{issues:?}");
        assert_eq!(chain.len(), 2);
        assert_eq!(chain.next_seq(), 2);
        assert_ne!(chain.tail(), Digest::GENESIS);
    }

    #[test]
    fn editing_a_line_breaks_its_position_and_every_digest_after_it() {
        let machine = MachineId::of("a");
        let text = body(&machine, &["a", "b", "c"]);
        let edited: Vec<String> = text
            .lines()
            .enumerate()
            .map(|(index, line)| {
                if index == 1 {
                    line.replace("\"seq\":1", "\"seq\":7")
                } else {
                    line.to_owned()
                }
            })
            .collect();

        let mut issues = Vec::new();
        Chain::parse(&machine, &edited.join("\n"), &mut issues);
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, Issue::SeqBreak { line: 2, .. })),
            "the edited line's own position no longer follows"
        );
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, Issue::ChainBreak { line: 3, .. })),
            "and the line after it no longer chains"
        );
    }

    #[test]
    fn a_truncated_head_shows_up_as_a_position_that_does_not_start_at_zero() {
        let machine = MachineId::of("a");
        let text = body(&machine, &["a", "b", "c"]);
        let kept: Vec<&str> = text.lines().skip(1).collect();
        let mut issues = Vec::new();
        Chain::parse(&machine, &kept.join("\n"), &mut issues);
        assert!(issues.iter().any(|i| matches!(
            i,
            Issue::SeqBreak {
                expected: 0,
                found: 1,
                ..
            }
        )));
    }

    #[test]
    fn a_malformed_line_is_kept_and_reported() {
        let machine = MachineId::of("a");
        let mut issues = Vec::new();
        let chain = Chain::parse(&machine, "{ not json\n", &mut issues);
        assert_eq!(chain.len(), 1, "the line is kept");
        assert!(issues.iter().any(|i| matches!(i, Issue::Malformed { .. })));
    }

    #[test]
    fn a_conflict_copy_is_excluded_rather_than_counted_twice() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let machine = MachineId::of("a");
        let text = body(&machine, &["a", "b"]);
        std::fs::write(root.join(file_name(&machine)), &text).unwrap();
        std::fs::write(root.join(format!("{machine}.jsonl.orig")), &text).unwrap();
        std::fs::write(root.join(".DS_Store"), "junk").unwrap();

        let chains = Chains::load(&root).unwrap();
        assert_eq!(chains.records().len(), 2, "the copy is not a second chain");
        assert!(
            chains
                .issues
                .iter()
                .any(|i| matches!(i, Issue::Foreign { .. })),
            "and it is reported"
        );
        assert!(
            !chains.issues.iter().any(|issue| matches!(
                issue,
                Issue::Foreign { file } if file == ".DS_Store"
            )),
            "a hidden file is the platform's litter, not a claim"
        );
    }

    #[test]
    fn a_record_that_names_another_machine_than_its_file_is_reported() {
        let mine = MachineId::of("a");
        let theirs = MachineId::of("b");
        let mut issues = Vec::new();
        Chain::parse(&mine, &body(&theirs, &["a"]), &mut issues);
        assert!(
            issues
                .iter()
                .any(|i| matches!(i, Issue::MachineMismatch { .. }))
        );
    }

    #[test]
    fn the_merge_orders_by_instant_then_machine_then_position() {
        let dir = tempfile::tempdir().unwrap();
        let root = camino::Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).unwrap();
        let first = MachineId::of("aaa");
        let second = MachineId::of("zzz");
        std::fs::write(root.join(file_name(&first)), body(&first, &["a", "b"])).unwrap();
        std::fs::write(root.join(file_name(&second)), body(&second, &["c"])).unwrap();

        let chains = Chains::load(&root).unwrap();
        assert!(chains.issues.is_empty(), "{:?}", chains.issues);
        let order: Vec<(String, u64)> = chains
            .records()
            .iter()
            .map(|record| (record.machine.to_string(), record.seq))
            .collect();
        let machines: Vec<&String> = order.iter().map(|(machine, _)| machine).collect();
        assert_eq!(machines.len(), 3);
        // Same instant throughout, so machine breaks the tie and position
        // orders within one.
        assert_eq!(order.first().map(|(_, seq)| *seq), Some(0));
        assert!(
            machines.first() < machines.last(),
            "a cross-machine tie breaks deterministically"
        );
        assert_eq!(chains.machines().count(), 2);
        assert_eq!(chains.len(), 3);
    }

    #[test]
    fn a_missing_directory_is_an_empty_corpus_rather_than_an_error() {
        let chains = Chains::load(camino::Utf8Path::new("/nowhere/at/all")).unwrap();
        assert!(chains.is_empty());
        assert!(chains.issues.is_empty());
        assert_eq!(chains.head(&MachineId::of("a")), (Digest::GENESIS, 0));
    }
}
