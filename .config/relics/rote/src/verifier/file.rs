//! The verifier file: machine-local, keyed by engram, and never backed up.
//!
//! **Keyed by engram rather than by name**, which is what makes a stale
//! verifier visible at all. Keyed by name, a verifier left over from a rotated
//! secret is indistinguishable from the current one — it would sit there
//! judging today's engram against yesterday's phrase, the textbook silent
//! disarm. Keyed by engram, `doctor` can see it and name it.
//!
//! A rotation must therefore **remove** the outgoing entry. Replacing by name
//! used to do that as a side effect; keying by engram does not, so it is now a
//! deliberate step with a test on it.

use std::collections::BTreeMap;

use anyhow::{Context as _, Result};
use camino::Utf8Path;
use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use super::Verifier;
use crate::corpus::record::EngramId;
use crate::slug::Slug;

/// The schema this binary writes and understands.
pub const SCHEMA: u32 = 3;

/// One engram's verifier, and enough label to name it to a person.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Held {
    /// The PHC string.
    pub phc: String,
    /// Which lineage it belongs to. A label, never authority — the corpus
    /// decides. It exists so `doctor` can say *a verifier for escrow-p, minted
    /// in April* rather than quoting an identity nobody recognises.
    pub slug: Slug,
    /// The day this verifier was minted here.
    pub minted: Date,
}

/// The file.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Verifiers {
    /// Schema, so a file from another binary is refused rather than misread.
    pub v: u32,
    /// One entry per engram this machine holds a verifier for.
    #[serde(default)]
    pub engrams: BTreeMap<EngramId, Held>,
}

impl Default for Verifiers {
    fn default() -> Self {
        Self {
            v: SCHEMA,
            engrams: BTreeMap::new(),
        }
    }
}

impl Verifiers {
    /// Read the file. A missing file is an empty set — which is exactly what a
    /// restore onto a new machine looks like, and it is a first-class state.
    ///
    /// # Errors
    ///
    /// When the file exists and will not parse, or was written to another
    /// schema. There is no migration: a verifier carries no history and is
    /// regenerable by re-typing the secret, so a migration path for one would
    /// never pay for itself.
    pub fn load(path: &Utf8Path) -> Result<Self> {
        let text = match fs_err::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Self::default());
            }
            Err(error) => return Err(error.into()),
        };
        let file: Self = toml::from_str(&text).with_context(|| format!("reading {path}"))?;
        anyhow::ensure!(
            file.v == SCHEMA,
            "{path} was written to schema {} and this rote speaks {SCHEMA}. \
             Verifiers are regenerable: remove it and run rote attach for each lineage.",
            file.v
        );
        Ok(file)
    }

    /// The verifier for an engram, if this machine holds one.
    ///
    /// # Errors
    ///
    /// When the stored string is not a PHC string.
    pub fn get(&self, engram: &EngramId) -> Result<Option<Verifier>> {
        self.engrams
            .get(engram)
            .map(|held| Verifier::parse(&held.phc))
            .transpose()
            .with_context(|| format!("reading the verifier for {engram}"))
    }

    /// Whether this machine holds one.
    pub fn holds(&self, engram: &EngramId) -> bool {
        self.engrams.contains_key(engram)
    }

    /// Put a verifier here, replacing any the same engram already had.
    pub fn set(&mut self, engram: EngramId, slug: &Slug, minted: Date, verifier: &Verifier) {
        self.engrams.insert(
            engram,
            Held {
                phc: verifier.as_str().to_owned(),
                slug: slug.clone(),
                minted,
            },
        );
    }

    /// Drop one, which is what a rotation owes the secret it retires.
    pub fn remove(&mut self, engram: &EngramId) {
        self.engrams.remove(engram);
    }

    /// Every engram this machine holds a verifier for.
    pub fn held(&self) -> impl Iterator<Item = (&EngramId, &Held)> {
        self.engrams.iter()
    }

    /// Write the file.
    ///
    /// # Errors
    ///
    /// When the tree cannot be created or the file cannot be written.
    pub fn save(&self, path: &Utf8Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            crate::store::ensure_dir(parent)?;
        }
        let text = toml::to_string_pretty(self)?;
        relic_core::fs::write_atomic_private(path, &text).with_context(|| format!("writing {path}"))
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;
    use jiff::civil::date;

    use super::{SCHEMA, Verifiers};
    use crate::corpus::record::EngramId;
    use crate::secret::Secret;
    use crate::verifier::Verifier;

    fn verifier() -> Verifier {
        Verifier::parse(
            "$argon2id$v=19$m=262144,t=4,p=1$c29tZXNhbHRzb21lc2E$\
             3f0hX2gYkRt1Zq8vQ0Yb2mKk5cJ8sWm1Q2r3T4u5V6c",
        )
        .unwrap_or_else(|_| {
            Verifier::create(&Secret::from_bytes(b"x").unwrap(), &[7u8; 16]).unwrap()
        })
    }

    #[test]
    fn an_absent_file_reads_as_an_empty_set() {
        let missing = Utf8PathBuf::from("/nowhere/at/all/verifiers.toml");
        assert!(Verifiers::load(&missing).unwrap().engrams.is_empty());
    }

    #[test]
    fn a_file_from_another_schema_is_refused_rather_than_misread() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("verifiers.toml")).unwrap();
        std::fs::write(&path, "v = 99\n").unwrap();
        let error = Verifiers::load(&path).unwrap_err().to_string();
        assert!(error.contains("rote attach"), "it says how to recover");
    }

    #[test]
    fn an_entry_round_trips_under_its_engram() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("verifiers.toml")).unwrap();
        let engram = EngramId::mint().unwrap();
        let slug = "escrow-p".parse().unwrap();

        let mut file = Verifiers::default();
        file.set(engram, &slug, date(2026, 9, 13), &verifier());
        file.save(&path).unwrap();

        let read = Verifiers::load(&path).unwrap();
        assert_eq!(read.v, SCHEMA);
        assert!(read.holds(&engram));
        assert!(read.get(&engram).unwrap().is_some());
        assert_eq!(read.held().count(), 1);

        let raw = std::fs::read_to_string(&path).unwrap();
        assert!(
            raw.contains(&engram.to_string()),
            "the engram is the key, not the slug"
        );
    }

    #[test]
    fn removing_is_how_a_rotation_forgets() {
        let engram = EngramId::mint().unwrap();
        let slug = "a".parse().unwrap();
        let mut file = Verifiers::default();
        file.set(engram, &slug, date(2026, 9, 13), &verifier());
        assert!(file.holds(&engram));
        file.remove(&engram);
        assert!(!file.holds(&engram));
    }
}
