//! What this computer is, and whether it may write.
//!
//! **Identity and label are two different things.** The *machine* is a stable
//! identity derived from hardware; the *hostname* is a mutable label it happens
//! to wear today. Every record carries both — the identity keys the chain, and
//! the label is what a person reads.
//!
//! The identity is **derived, never stored**. A generated identity would have to
//! live in a file, and a file is exactly what gets copied onto the new laptop
//! during a migration — after which two machines would share an identity and
//! append to one hash chain, which is the single catastrophic failure this
//! design has. Deriving it from the platform's own identifier removes the file
//! and the failure with it.
//!
//! What is stored is a truncated hash rather than the platform identifier,
//! because only distinctness is needed and a device serial is not ours to keep.
//! It is written as a **proquint** — see [`MachineId`] — because the identity
//! has a second job the hash does not care about: a person accrues machines
//! over the years and comes to recognise each one by the shape of its id.

use anyhow::{Context as _, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// Syllables in an identity. Three of sixteen bits: forty-eight bits, which a
/// person's machines will not collide in across a lifetime of them.
const QUINTS: usize = 3;

/// Characters in an identity: five to a syllable, hyphenated between.
const ID_LEN: usize = QUINTS * 6 - 1;

/// The shape an identity is written in, for the one error that has to say it.
const ID_SHAPE: &str = "three hyphenated syllables, each a consonant, a vowel, \
                        a consonant, a vowel and a consonant";

/// A stable, opaque identity for one computer.
///
/// Written as a **proquint** — pronounceable quintuplets, a plain re-encoding of
/// the same digest bits. Hex is unreadable in the way that matters here: two
/// machines are told apart by eye, over and over, and `3b43d1cf` against
/// `3b34d1cf` is one glance from a mistake. `lusab-babad-gutih` against
/// `tomad-kifun-rasoz` is not, and it can be said aloud.
///
/// Lowercase throughout, so a case-insensitive filesystem can never see two
/// identities as one chain.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MachineId(String);

impl MachineId {
    /// Derive an identity from whatever the platform calls itself.
    pub fn of(source: &str) -> Self {
        let digest = Sha256::digest(source.as_bytes());
        let mut text = String::with_capacity(ID_LEN);
        let (pairs, _) = digest.as_chunks::<2>();
        for (index, [high, low]) in pairs.iter().take(QUINTS).enumerate() {
            if index > 0 {
                text.push('-');
            }
            quint((u16::from(*high) << 8) | u16::from(*low), &mut text);
        }
        Self(text)
    }

    /// The identity as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether a string is shaped like an identity, without allocating one.
    ///
    /// A shape rather than a length, so a conflict copy of a chain file cannot
    /// pass for a second machine however it was renamed.
    pub fn looks_like(text: &str) -> bool {
        let mut seen = 0usize;
        for part in text.split('-') {
            seen = seen.saturating_add(1);
            if seen > QUINTS || !is_quint(part) {
                return false;
            }
        }
        seen == QUINTS
    }
}

/// One sixteen-bit syllable: consonant, vowel, consonant, vowel, consonant.
fn quint(value: u16, out: &mut String) {
    out.push(consonant(value >> 12));
    out.push(vowel(value >> 10));
    out.push(consonant(value >> 6));
    out.push(vowel(value >> 4));
    out.push(consonant(value));
}

/// The sixteen consonants, by their four bits. A match rather than a table so
/// the mapping is total and needs no index.
fn consonant(bits: u16) -> char {
    match bits & 0xf {
        0 => 'b',
        1 => 'd',
        2 => 'f',
        3 => 'g',
        4 => 'h',
        5 => 'j',
        6 => 'k',
        7 => 'l',
        8 => 'm',
        9 => 'n',
        10 => 'p',
        11 => 'r',
        12 => 's',
        13 => 't',
        14 => 'v',
        _ => 'z',
    }
}

/// The four vowels, by their two bits.
fn vowel(bits: u16) -> char {
    match bits & 0x3 {
        0 => 'a',
        1 => 'i',
        2 => 'o',
        _ => 'u',
    }
}

/// Whether a byte is one of the sixteen.
fn is_consonant(byte: u8) -> bool {
    matches!(
        byte,
        b'b' | b'd'
            | b'f'
            | b'g'
            | b'h'
            | b'j'
            | b'k'
            | b'l'
            | b'm'
            | b'n'
            | b'p'
            | b'r'
            | b's'
            | b't'
            | b'v'
            | b'z'
    )
}

/// Whether a byte is one of the four.
fn is_vowel(byte: u8) -> bool {
    matches!(byte, b'a' | b'i' | b'o' | b'u')
}

/// Whether a string is one syllable.
fn is_quint(part: &str) -> bool {
    let shape: [fn(u8) -> bool; 5] = [is_consonant, is_vowel, is_consonant, is_vowel, is_consonant];
    let mut bytes = part.bytes();
    shape.iter().all(|fits| bytes.next().is_some_and(fits)) && bytes.next().is_none()
}

/// A string that is not a machine identity.
#[derive(Debug, thiserror::Error)]
#[error("a machine identity is {ID_SHAPE}")]
pub struct BadMachineId;

impl TryFrom<String> for MachineId {
    type Error = BadMachineId;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        if Self::looks_like(&text) {
            Ok(Self(text))
        } else {
            Err(BadMachineId)
        }
    }
}

impl From<MachineId> for String {
    fn from(id: MachineId) -> Self {
        id.0
    }
}

impl std::fmt::Display for MachineId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Resolve this machine's identity.
///
/// `seam` is `ROTE_MACHINE`, which a test uses to be two machines at once. It
/// is an environment override rather than a file, so it cannot be copied onto a
/// second machine by accident.
///
/// # Errors
///
/// When the platform offers no identifier and no seam was given.
pub fn resolve(seam: Option<&str>) -> Result<MachineId> {
    if let Some(text) = seam {
        return MachineId::try_from(text.to_owned())
            .map_err(|_| anyhow!("ROTE_MACHINE is not a machine identity: {text:?}"));
    }
    let source = platform_identifier().ok_or_else(|| {
        anyhow!(
            "this platform offers no stable machine identifier, so rote cannot \
             name its chain. Set ROTE_MACHINE to {ID_SHAPE}."
        )
    })?;
    Ok(MachineId::of(&source))
}

/// The platform's own identifier for this computer, if it has one.
fn platform_identifier() -> Option<String> {
    if cfg!(target_os = "macos") {
        return platform_uuid_from_ioreg();
    }
    // systemd writes this once at install and never again.
    for path in ["/etc/machine-id", "/var/lib/dbus/machine-id"] {
        if let Ok(text) = fs_err::read_to_string(path) {
            let trimmed = text.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_owned());
            }
        }
    }
    None
}

fn platform_uuid_from_ioreg() -> Option<String> {
    let tool = relic_core::tool::Tool::find("ioreg")?;
    let mut command = tool.command();
    command.args(["-rd1", "-c", "IOPlatformExpertDevice"]);
    let output = tool
        .capture_within(&mut command, std::time::Duration::from_secs(2))
        .ok()?;
    parse_ioreg(&output.stdout)
}

/// Pull `IOPlatformUUID` out of an `ioreg` dump.
fn parse_ioreg(dump: &str) -> Option<String> {
    for line in dump.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if !key.contains("IOPlatformUUID") {
            continue;
        }
        let uuid = value.trim().trim_matches('"').trim();
        if !uuid.is_empty() {
            return Some(uuid.to_owned());
        }
    }
    None
}

/// This machine's label. Mutable, and never an identity.
pub fn hostname() -> String {
    // `ROTE_HOST` is a test seam: a suite that has to run on any machine cannot
    // assert against the machine it happens to be running on.
    std::env::var("ROTE_HOST").unwrap_or_else(|_| {
        gethostname::gethostname()
            .into_string()
            .unwrap_or_else(|_| "unknown".to_owned())
    })
}

/// How a machine is named wherever one is named in prose.
///
/// Both halves, because neither is enough on its own: the identity is what is
/// actually stable and actually keys the chain, and the hostname is what a
/// person recognises. One function, so every surface says it the same way.
pub fn label(machine: &MachineId, host: &str) -> String {
    format!("{machine} ({})", short_host(host))
}

/// A hostname with the suffix mDNS gives it stripped off.
///
/// The suffix says which network answered for the name, which is not a fact
/// about the machine and changes without it changing.
pub fn short_host(host: &str) -> &str {
    host.strip_suffix(".local").unwrap_or(host)
}

/// Where the marker lives when nothing overrides it.
///
/// Under `~/.local/state` rather than in a synced tree, and that is the whole
/// point: a marker that travelled with the dotfiles would declare every machine
/// the flagship, which is exactly backwards.
pub fn marker_path(home: &Utf8Path, seam: Option<&Utf8Path>) -> Utf8PathBuf {
    match seam {
        Some(path) => path.to_owned(),
        None => home
            .join(".local")
            .join("state")
            .join("reliquary")
            .join("flagship"),
    }
}

/// Whether this machine may write to the corpus.
///
/// An accident guard, not a control. It exists so that running `rote` on a
/// satellite does not start a second chain nobody will ever restore.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Flagship {
    /// The marker is here, and either names this machine or names nobody.
    Here,
    /// There is no marker.
    Absent,
    /// The marker names a different machine.
    Elsewhere {
        /// What it names.
        named: String,
    },
}

impl Flagship {
    /// Read the marker.
    ///
    /// An empty marker authorises whoever holds it. A non-empty one must name
    /// this machine, so a marker copied onto a second computer refuses loudly
    /// instead of quietly authorising a second writer.
    ///
    /// # Errors
    ///
    /// When the marker exists and cannot be read.
    pub fn read(path: &Utf8Path, machine: &MachineId) -> Result<Self> {
        let text = match fs_err::read_to_string(path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Self::Absent),
            Err(error) => {
                return Err(error).with_context(|| format!("reading the flagship marker {path}"));
            }
        };
        let named = text
            .lines()
            .map(str::trim)
            .find(|line| !line.is_empty() && !line.starts_with('#'));
        Ok(match named {
            None => Self::Here,
            Some(name) if name == machine.as_str() => Self::Here,
            Some(name) => Self::Elsewhere {
                named: name.to_owned(),
            },
        })
    }

    /// Whether writing is allowed.
    pub fn writes_allowed(&self) -> bool {
        matches!(self, Self::Here)
    }

    /// Why not, in the words a person needs to fix it.
    pub fn refusal(&self, path: &Utf8Path, machine: &MachineId, host: &str) -> String {
        match self {
            Self::Here => String::new(),
            Self::Absent => format!(
                "this machine is not the flagship, so rote will not write here. Reads are fine.\n\
                 {path} is absent. If this machine is the flagship, write its identity there:\n\
                 echo {machine} > {path}"
            ),
            Self::Elsewhere { named } => format!(
                "the flagship marker names another machine, so rote will not write here.\n\
                 {path} names {named}, and this machine is {}.",
                label(machine, host)
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::{Flagship, ID_LEN, MachineId, QUINTS, marker_path, parse_ioreg};

    /// The inverse of `quint`, which only a test needs: the property worth
    /// asserting is that the encoding loses nothing, and nothing in the binary
    /// reads an identity back apart.
    fn unquint(part: &str) -> Option<u16> {
        const CONSONANTS: &str = "bdfghjklmnprstvz";
        const VOWELS: &str = "aiou";
        let mut value = 0u16;
        for (index, character) in part.chars().enumerate() {
            let (table, width) = if index % 2 == 0 {
                (CONSONANTS, 4)
            } else {
                (VOWELS, 2)
            };
            let position = u16::try_from(table.find(character)?).ok()?;
            value = (value << width) | position;
        }
        Some(value)
    }

    #[test]
    fn an_identity_is_a_truncated_hash_and_never_the_source() {
        let id = MachineId::of("ABCDEF01-2345-6789-ABCD-EF0123456789");
        assert_eq!(id.as_str().len(), ID_LEN);
        assert_eq!(id.as_str().split('-').count(), QUINTS);
        assert!(MachineId::looks_like(id.as_str()));
        assert!(
            !id.as_str().contains("ABCDEF"),
            "the platform identifier must not survive into the identity"
        );
    }

    #[test]
    fn the_same_source_always_gives_the_same_identity() {
        assert_eq!(MachineId::of("a"), MachineId::of("a"));
        assert_ne!(MachineId::of("a"), MachineId::of("b"));
    }

    #[test]
    fn an_identity_round_trips_and_refuses_a_bad_one() {
        let id = MachineId::of("x");
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(serde_json::from_str::<MachineId>(&json).unwrap(), id);
        assert!(serde_json::from_str::<MachineId>("\"short\"").is_err());
        assert!(
            serde_json::from_str::<MachineId>("\"AAAAAAAAAAAAAAAA\"").is_err(),
            "uppercase is a different spelling of the same bytes, so it is refused"
        );
    }

    #[test]
    fn ioreg_is_parsed_out_of_its_own_dump() {
        let dump = "\
+-o Root  <class IORegistryEntry>
    {
      \"IOPlatformSerialNumber\" = \"C02ABCDEFGH\"
      \"IOPlatformUUID\" = \"ABCDEF01-2345-6789-ABCD-EF0123456789\"
    }
";
        assert_eq!(
            parse_ioreg(dump).as_deref(),
            Some("ABCDEF01-2345-6789-ABCD-EF0123456789")
        );
        assert_eq!(parse_ioreg("nothing of the sort"), None);
    }

    #[test]
    fn the_marker_defaults_under_machine_local_state() {
        let home = Utf8PathBuf::from("/home/x");
        assert_eq!(
            marker_path(&home, None),
            Utf8PathBuf::from("/home/x/.local/state/reliquary/flagship")
        );
        let seam = Utf8PathBuf::from("/tmp/marker");
        assert_eq!(marker_path(&home, Some(&seam)), seam);
    }

    #[test]
    fn an_absent_marker_refuses_and_says_how_to_make_one() {
        let machine = MachineId::of("this");
        let path = Utf8PathBuf::from("/tmp/flagship");
        let state = Flagship::Absent;
        assert!(!state.writes_allowed());
        let said = state.refusal(&path, &machine, "Testbed");
        assert!(said.contains("Reads are fine"));
        assert!(said.contains(machine.as_str()), "it names what to write");
    }

    #[test]
    fn an_empty_marker_authorises_and_a_filled_one_must_match() {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("flagship")).unwrap();
        let machine = MachineId::of("this");

        assert_eq!(
            Flagship::read(&path, &machine).unwrap(),
            Flagship::Absent,
            "no marker at all"
        );

        std::fs::write(&path, "").unwrap();
        assert_eq!(Flagship::read(&path, &machine).unwrap(), Flagship::Here);

        std::fs::write(&path, "# a comment\n\n").unwrap();
        assert_eq!(
            Flagship::read(&path, &machine).unwrap(),
            Flagship::Here,
            "comments and blank lines are not a name"
        );

        std::fs::write(&path, format!("{machine}\n")).unwrap();
        assert_eq!(Flagship::read(&path, &machine).unwrap(), Flagship::Here);

        let other = MachineId::of("that");
        std::fs::write(&path, format!("{other}\n")).unwrap();
        let state = Flagship::read(&path, &machine).unwrap();
        assert_eq!(
            state,
            Flagship::Elsewhere {
                named: other.to_string()
            }
        );
        assert!(!state.writes_allowed());
        assert!(
            state
                .refusal(&path, &machine, "Testbed")
                .contains(other.as_str())
        );
    }

    proptest::proptest! {
        /// The encoding is a re-spelling and not a lossy one: every syllable
        /// carries its sixteen bits back out again.
        #[test]
        fn a_syllable_carries_every_bit_it_was_given(value: u16) {
            let mut text = String::new();
            super::quint(value, &mut text);
            proptest::prop_assert_eq!(unquint(&text), Some(value));
        }

        /// Distinct sources are distinct machines, which is the whole job.
        #[test]
        fn two_sources_that_differ_name_two_machines(left: String, right: String) {
            proptest::prop_assume!(left != right);
            proptest::prop_assert_ne!(MachineId::of(&left), MachineId::of(&right));
        }
    }

    #[test]
    fn only_the_written_shape_is_an_identity() {
        for text in [
            "",
            "lusab",
            "lusab-babad",
            "lusab-babad-gutih-lusab",
            "lusab babad gutih",
            "LUSAB-BABAD-GUTIH",
            "lusab-babad-gutih ",
            "3b43d1cf18a69c61",
            // A conflict copy of a chain, which must never read as a machine.
            "lusab-babad-gutih (1)",
            // Right length, wrong alphabet: e is not a vowel here.
            "lesab-babad-gutih",
        ] {
            assert!(!MachineId::looks_like(text), "{text}");
        }
        assert!(MachineId::looks_like("lusab-babad-gutih"));
    }
}
