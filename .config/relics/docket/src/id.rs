use std::fmt;

use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};

/// Crockford base32, lowercase: the digits and letters that survive being read
/// aloud, transcribed, and double-clicked. `i`, `l`, `o` and `u` are absent.
const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";

pub const WIDTH: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Id([u8; WIDTH]);

impl Id {
    pub fn mint() -> Id {
        let mut raw = [0u8; WIDTH];
        getrandom::fill(&mut raw).expect("system entropy");
        let mut out = [0u8; WIDTH];
        for (slot, byte) in out.iter_mut().zip(raw) {
            *slot = ALPHABET[usize::from(byte) % ALPHABET.len()];
        }
        Id(out)
    }

    pub fn as_str(&self) -> &str {
        std::str::from_utf8(&self.0).expect("alphabet is ascii")
    }
}

impl std::str::FromStr for Id {
    type Err = anyhow::Error;

    /// Accepts the bracketed form the listings print, so an id pasted straight
    /// out of a terminal selection resolves.
    fn from_str(raw: &str) -> Result<Id> {
        let trimmed = raw.trim().trim_start_matches('[').trim_end_matches(']');
        let lowered = trimmed.to_ascii_lowercase();
        let bytes = lowered.as_bytes();
        if bytes.len() != WIDTH || !bytes.iter().all(|b| ALPHABET.contains(b)) {
            bail!(
                "not an id: {raw:?} — ids are {WIDTH} characters of 0-9a-z, without i, l, o or u"
            );
        }
        let mut out = [0u8; WIDTH];
        out.copy_from_slice(bytes);
        Ok(Id(out))
    }
}

impl TryFrom<String> for Id {
    type Error = anyhow::Error;

    fn try_from(value: String) -> Result<Id> {
        value.parse()
    }
}

impl From<Id> for String {
    fn from(id: Id) -> String {
        id.as_str().to_owned()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl fmt::Debug for Id {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn minted_ids_are_well_formed() {
        for _ in 0..1000 {
            let id = Id::mint();
            assert_eq!(id.as_str().len(), WIDTH);
            let reparsed: Id = id.as_str().parse().unwrap();
            assert_eq!(id, reparsed);
        }
    }

    #[test]
    fn ambiguous_characters_never_appear() {
        for _ in 0..2000 {
            let id = Id::mint();
            assert!(!id.as_str().contains(['i', 'l', 'o', 'u']));
        }
    }

    #[test]
    fn parsing_tolerates_brackets_whitespace_and_case() {
        let id: Id = "b71c".parse().unwrap();
        assert_eq!("[b71c]".parse::<Id>().unwrap(), id);
        assert_eq!(" B71C ".parse::<Id>().unwrap(), id);
    }

    #[test]
    fn parsing_rejects_wrong_shapes() {
        for bad in ["", "b71", "b71cd", "b7!c", "b7ic", "b7lc", "b7oc", "b7uc"] {
            assert!(bad.parse::<Id>().is_err(), "{bad:?} should not parse");
        }
    }
}
