//! The typed value, and the only type in this crate that ever holds one.
//!
//! Three properties, and each is load-bearing:
//!
//! - **The buffer never grows.** Capacity is reserved once and input past it is
//!   refused. A `String` or `Vec` that reallocates leaves the old allocation on
//!   the heap with the bytes still in it, and nothing later can reach it to
//!   wipe it. This is also why the crate does not use `secrecy`: `SecretString`
//!   is built through `String::into_boxed_str`, which shrinks to fit and so
//!   copies exactly the buffer this type exists to avoid copying.
//! - **It wipes itself.** `Zeroizing<Vec<u8>>` clears the whole capacity on
//!   drop, not just the length.
//! - **It cannot be printed.** No `Display`, no `Serialize`, and a `Debug` that
//!   says nothing. Every path to the bytes goes through [`Secret::expose`], so
//!   auditing the crate is one grep.

use std::fmt;

use zeroize::Zeroizing;

/// Bytes reserved for one entry. A seven-word EFF passphrase is around fifty;
/// this leaves room for something much longer without ever reallocating.
pub const CAPACITY: usize = 1024;

/// A typed secret.
pub struct Secret {
    bytes: Zeroizing<Vec<u8>>,
}

impl Secret {
    /// An empty buffer with its capacity already reserved.
    pub fn new() -> Self {
        let mut bytes = Vec::new();
        bytes.reserve_exact(CAPACITY);
        Self {
            bytes: Zeroizing::new(bytes),
        }
    }

    /// Take bytes as given, for the paths that read a secret from a pipe.
    ///
    /// Longer than [`CAPACITY`] is refused rather than truncated: a silently
    /// truncated secret would enroll a verifier for something nobody typed.
    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() > CAPACITY {
            return None;
        }
        let mut secret = Self::new();
        secret.bytes.extend_from_slice(bytes);
        Some(secret)
    }

    /// Append a character. False when the buffer is full, in which case nothing
    /// was appended.
    pub fn push(&mut self, character: char) -> bool {
        let mut buffer = [0u8; 4];
        let encoded = character.encode_utf8(&mut buffer);
        if self.bytes.len().saturating_add(encoded.len()) > CAPACITY {
            buffer.fill(0);
            return false;
        }
        self.bytes.extend_from_slice(encoded.as_bytes());
        buffer.fill(0);
        true
    }

    /// Remove the last character. False when there was nothing to remove.
    pub fn pop(&mut self) -> bool {
        let boundary = self
            .bytes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, byte)| **byte & 0b1100_0000 != 0b1000_0000)
            .map(|(index, _)| index);
        match boundary {
            Some(index) => {
                for byte in self.bytes.get_mut(index..).unwrap_or_default() {
                    *byte = 0;
                }
                self.bytes.truncate(index);
                true
            }
            None => false,
        }
    }

    /// Whether anything has been typed.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }

    /// Forget everything typed so far.
    pub fn clear(&mut self) {
        self.bytes.iter_mut().for_each(|byte| *byte = 0);
        self.bytes.clear();
    }

    /// The bytes. The one way in, and therefore the one thing to audit.
    pub fn expose(&self) -> &[u8] {
        &self.bytes
    }

    /// Whether two entries match, for the double entry at enrollment.
    ///
    /// Not constant time, and it does not need to be: both sides were typed by
    /// the same person a moment apart, and neither is a stored secret being
    /// guessed at.
    pub fn same_as(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

impl Default for Secret {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::{CAPACITY, Secret};

    #[test]
    fn typing_and_correcting_reach_the_bytes_typed() {
        let mut secret = Secret::new();
        assert!(secret.is_empty());
        for character in "hello".chars() {
            assert!(secret.push(character));
        }
        assert!(secret.pop());
        assert_eq!(secret.expose(), b"hell");
        assert!(!secret.is_empty());
    }

    #[test]
    fn a_correction_removes_a_whole_character_not_a_byte() {
        let mut secret = Secret::new();
        assert!(secret.push('é'));
        assert_eq!(secret.expose().len(), 2);
        assert!(secret.pop());
        assert!(secret.is_empty());
    }

    #[test]
    fn correcting_an_empty_buffer_says_so_rather_than_panicking() {
        let mut secret = Secret::new();
        assert!(!secret.pop());
    }

    #[test]
    fn the_buffer_refuses_to_grow_rather_than_reallocating() {
        let mut secret = Secret::new();
        let before = secret.expose().as_ptr();
        for _ in 0..CAPACITY {
            assert!(secret.push('x'));
        }
        assert!(!secret.push('x'), "the buffer is full");
        assert_eq!(secret.expose().len(), CAPACITY);
        assert_eq!(
            secret.expose().as_ptr(),
            before,
            "the allocation must never move, or the old one keeps the bytes"
        );
    }

    #[test]
    fn a_multibyte_character_that_would_overflow_is_refused_whole() {
        let mut secret = Secret::new();
        for _ in 0..CAPACITY - 1 {
            assert!(secret.push('x'));
        }
        assert!(!secret.push('é'), "two bytes will not fit in one");
        assert_eq!(secret.expose().len(), CAPACITY - 1);
    }

    #[test]
    fn bytes_from_a_pipe_are_taken_whole_or_refused() {
        assert_eq!(Secret::from_bytes(b"abc").unwrap().expose(), b"abc");
        assert!(Secret::from_bytes(&vec![b'x'; CAPACITY]).is_some());
        assert!(
            Secret::from_bytes(&vec![b'x'; CAPACITY + 1]).is_none(),
            "truncating would enroll a verifier for something nobody typed"
        );
    }

    #[test]
    fn clearing_leaves_nothing_behind() {
        let mut secret = Secret::new();
        for character in "hunter2".chars() {
            secret.push(character);
        }
        secret.clear();
        assert!(secret.is_empty());
    }

    #[test]
    fn two_entries_compare_for_the_double_entry() {
        let make = |text: &str| Secret::from_bytes(text.as_bytes()).unwrap();
        assert!(make("a b c").same_as(&make("a b c")));
        assert!(!make("a b c").same_as(&make("a b d")));
        assert!(!make("a").same_as(&make("ab")));
    }

    #[test]
    fn it_cannot_be_printed() {
        let secret = Secret::from_bytes(b"hunter2").unwrap();
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert!(!format!("{secret:#?}").contains("hunter"));
    }
}
