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

use anyhow::{Context as _, Result, anyhow};
use camino::{Utf8Path, Utf8PathBuf};
use serde::{Deserialize, Serialize};
use sha2::{Digest as _, Sha256};

/// Hex characters in a machine identity. Eight bytes of SHA-256.
pub const ID_LEN: usize = 16;

/// A stable, opaque identity for one computer.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct MachineId(String);

impl MachineId {
    /// Derive an identity from whatever the platform calls itself.
    pub fn of(source: &str) -> Self {
        let digest = Sha256::digest(source.as_bytes());
        let mut text = String::with_capacity(ID_LEN);
        for byte in digest.iter().take(ID_LEN / 2) {
            let _ = std::fmt::Write::write_fmt(&mut text, format_args!("{byte:02x}"));
        }
        Self(text)
    }

    /// The identity as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// Whether a string is shaped like an identity, without allocating one.
    pub fn looks_like(text: &str) -> bool {
        text.len() == ID_LEN
            && text
                .bytes()
                .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
    }
}

/// A string that is not a machine identity.
#[derive(Debug, thiserror::Error)]
#[error("a machine identity is {ID_LEN} lowercase hex characters")]
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
             name its chain. Set ROTE_MACHINE to {ID_LEN} hex characters."
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
    pub fn refusal(&self, path: &Utf8Path, machine: &MachineId) -> String {
        match self {
            Self::Here => String::new(),
            Self::Absent => format!(
                "this machine is not the flagship, so rote will not write here. Reads are fine.\n\
                 {path} is absent. If this machine is the flagship, write its identity there:\n\
                 echo {machine} > {path}"
            ),
            Self::Elsewhere { named } => format!(
                "the flagship marker names another machine, so rote will not write here.\n\
                 {path} names {named}, and this machine is {machine}."
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use super::{Flagship, ID_LEN, MachineId, marker_path, parse_ioreg};

    #[test]
    fn an_identity_is_a_truncated_hash_and_never_the_source() {
        let id = MachineId::of("ABCDEF01-2345-6789-ABCD-EF0123456789");
        assert_eq!(id.as_str().len(), ID_LEN);
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
        let said = state.refusal(&path, &machine);
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
        assert!(state.refusal(&path, &machine).contains(other.as_str()));
    }
}
