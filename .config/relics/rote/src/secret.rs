//! The typed value, and the two types in this crate that ever hold one.
//!
//! Three properties, and each is load-bearing:
//!
//! - **The buffer never grows.** Capacity is reserved once and input past it is
//!   refused. A `String` or `Vec` that reallocates leaves the old allocation on
//!   the heap with the bytes still in it, and nothing later can reach it to
//!   wipe it. This is also why the crate does not use `secrecy`: `SecretString`
//!   is built through `String::into_boxed_str`, which shrinks to fit and so
//!   copies exactly the buffer this type exists to avoid copying. Every guard
//!   here reads [`CAPACITY`] and never `Vec::capacity`, which `reserve_exact` is
//!   free to overshoot.
//! - **It wipes itself.** `Zeroizing<Vec<u8>>` clears the whole capacity on
//!   drop, not just the length. A removal wipes sooner than that: shifting the
//!   tail down leaves a live copy of it above the new length, and `truncate`
//!   does not reach it.
//! - **It cannot be printed.** No `Display`, no `Serialize`, and a `Debug` that
//!   says nothing. Every path to the bytes goes through [`Secret::expose`] and
//!   [`Secret::text`], so auditing the crate is one grep.
//!
//! Editing is by character index, because a caret is. Nothing here calls
//! `Vec::insert`, `Vec::remove` or `Vec::drain`: each of them checks
//! `len == capacity` and calls `reserve`, which is the one thing this type
//! exists to prevent. Nor does anything assume valid UTF-8 — [`Secret::from_bytes`]
//! takes whatever a pipe delivered — so an index walks lead bytes and clamps.
//!
//! Word boundaries are deliberately **not** derived here. A word length is the
//! one thing about a passphrase this tool may never disclose, and the module
//! that holds the value is the wrong place to teach how to find them.
//!
//! [`Pasted`] is the one other type here that holds a typed value, and it holds
//! it under the same three properties minus the fixed buffer, which the
//! terminal allocated before this crate saw it. It lives beside [`Secret`] so
//! that the grep stays one grep.

use std::fmt;

use zeroize::Zeroizing;

/// Bytes reserved for one entry. A seven-word EFF passphrase is around fifty;
/// this leaves room for something much longer without ever reallocating.
pub const CAPACITY: usize = 1024;

/// How many bytes two characters can take between them, for a transposition.
const PAIR: usize = 8;

/// Whether a byte starts a character rather than continuing one.
const fn lead(byte: u8) -> bool {
    byte & 0b1100_0000 != 0b1000_0000
}

/// Zero everything from a byte offset to the end of a buffer.
///
/// Its own function because it is the security delta of every removal and the
/// only part of one that a test can see: a `Vec` shrunk with `truncate` keeps
/// the bytes above its new length, and nothing in safe Rust can look at them
/// afterwards to prove they were wiped. Over a slice they are in plain view.
fn scrub(bytes: &mut [u8], from: usize) {
    if let Some(tail) = bytes.get_mut(from..) {
        tail.fill(0);
    }
}

/// The byte offset of a character index, clamped to the end.
fn offset(bytes: &[u8], index: usize) -> usize {
    bytes
        .iter()
        .enumerate()
        .filter(|(_, byte)| lead(**byte))
        .map(|(at, _)| at)
        .nth(index)
        .unwrap_or(bytes.len())
}

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

    /// How many characters are in the buffer.
    ///
    /// The count a caret indexes and a masked field draws, which is why it
    /// counts characters and never bytes.
    pub fn chars(&self) -> usize {
        self.bytes.iter().filter(|byte| lead(**byte)).count()
    }

    /// The buffer as text, or nothing when it is not valid UTF-8.
    ///
    /// Only a revealed field asks. A pipe may deliver anything, so this is an
    /// `Option` rather than an assumption.
    pub fn text(&self) -> Option<&str> {
        std::str::from_utf8(&self.bytes).ok()
    }

    /// Insert a character at a caret. False when the buffer is full, in which
    /// case nothing was inserted.
    pub fn insert(&mut self, at: usize, character: char) -> bool {
        let mut buffer = [0u8; 4];
        let encoded = character.encode_utf8(&mut buffer);
        let taken = self.splice(at, encoded.as_bytes());
        buffer.fill(0);
        taken
    }

    /// Insert text that arrived as a paste at a caret. False when it will not
    /// fit, in which case nothing was inserted.
    ///
    /// Whole or not at all, for the reason [`Secret::from_bytes`] gives: a
    /// secret half-entered without anybody noticing is what this type exists to
    /// prevent, and the caller says so rather than swallowing it.
    pub fn insert_str(&mut self, at: usize, text: &str) -> bool {
        self.splice(at, text.as_bytes())
    }

    /// Make room at a character index and fill it. The one growing path.
    fn splice(&mut self, at: usize, incoming: &[u8]) -> bool {
        let len = self.bytes.len();
        let grown = len.saturating_add(incoming.len());
        if grown > CAPACITY {
            return false;
        }
        if incoming.is_empty() {
            return true;
        }
        let at = offset(&self.bytes, at);
        self.bytes.resize(grown, 0);
        self.bytes
            .copy_within(at..len, at.saturating_add(incoming.len()));
        if let Some(room) = self.bytes.get_mut(at..at.saturating_add(incoming.len())) {
            room.copy_from_slice(incoming);
        }
        true
    }

    /// Remove the characters in a half-open range of character indices. False
    /// when the range held nothing.
    ///
    /// Both ends clamp, so a caller that has lost track of the end of the
    /// buffer removes what is there rather than panicking inside a dialog.
    pub fn remove_range(&mut self, from: usize, to: usize) -> bool {
        if from >= to {
            return false;
        }
        let len = self.bytes.len();
        let start = offset(&self.bytes, from);
        let end = offset(&self.bytes, to);
        if start >= end {
            return false;
        }
        let kept = len.saturating_sub(end.saturating_sub(start));
        self.bytes.copy_within(end..len, start);
        // The shift left leaves a live copy of the tail above the new length,
        // and `truncate` does not reach it.
        scrub(&mut self.bytes, kept);
        self.bytes.truncate(kept);
        true
    }

    /// Swap the character before a caret with the one at it, as readline's
    /// `transpose-chars` does, and say whether anything moved.
    ///
    /// At the end of the buffer it swaps the last two, which is what every
    /// other line editor does there. Nothing about the buffer's length or the
    /// caret changes, which is what makes it safe to offer over a masked field.
    pub fn transpose(&mut self, at: usize) -> bool {
        let count = self.chars();
        if at == 0 || count < 2 {
            return false;
        }
        let right = if at >= count {
            count.saturating_sub(1)
        } else {
            at
        };
        let left = right.saturating_sub(1);
        let start = offset(&self.bytes, left);
        let middle = offset(&self.bytes, right);
        let end = offset(&self.bytes, right.saturating_add(1));
        let first = middle.saturating_sub(start);
        let second = end.saturating_sub(middle);
        let span = first.saturating_add(second);
        if span == 0 || span > PAIR {
            return false;
        }
        let mut scratch = [0u8; PAIR];
        let swapped = self.swap_into(&mut scratch, start, middle, end);
        scratch.fill(0);
        swapped
    }

    /// The byte shuffle behind [`Secret::transpose`], kept apart so the scratch
    /// buffer is wiped on every path out of it.
    ///
    /// Both characters go out to the stack in their new order and come back in
    /// one write. Nothing here allocates: a heap copy of a character is a copy
    /// this type has no way to reach afterwards.
    fn swap_into(
        &mut self,
        scratch: &mut [u8; PAIR],
        start: usize,
        middle: usize,
        end: usize,
    ) -> bool {
        let first = middle.saturating_sub(start);
        let second = end.saturating_sub(middle);
        let span = end.saturating_sub(start);
        {
            let Some(source) = self.bytes.get(start..end) else {
                return false;
            };
            let (Some(head), Some(tail)) = (source.get(..first), source.get(first..)) else {
                return false;
            };
            let Some(room) = scratch.get_mut(..span) else {
                return false;
            };
            if second > room.len() {
                return false;
            }
            let (front, back) = room.split_at_mut(second);
            front.copy_from_slice(tail);
            back.copy_from_slice(head);
        }
        let (Some(room), Some(source)) = (self.bytes.get_mut(start..end), scratch.get(..span))
        else {
            return false;
        };
        room.copy_from_slice(source);
        true
    }

    /// Whether anything has been typed.
    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
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

/// Text the terminal delivered as a bracketed paste.
///
/// Held the way a typed value is held, because a paste is usually the secret
/// itself read out of a vault: it wipes itself on drop, it cannot be printed,
/// and the one way to the bytes is [`Pasted::expose`]. It is not a [`Secret`]
/// and never becomes one on its own — it is either inserted into the buffer at
/// a prompt that takes it, or dropped at a prompt that does not.
pub struct Pasted(Zeroizing<String>);

impl Pasted {
    /// Take what the terminal read, moved rather than copied, so the only
    /// buffer holding it is the one it arrived in.
    pub fn new(text: String) -> Self {
        Self(Zeroizing::new(text))
    }

    /// The text. The one way in, and therefore the one thing to audit.
    pub fn expose(&self) -> &str {
        &self.0
    }
}

/// Compared so that a key can be, which is what the suite asserts on. Not
/// constant time, and it does not need to be: neither side is a stored secret
/// being guessed at.
impl PartialEq for Pasted {
    fn eq(&self, other: &Self) -> bool {
        self.expose() == other.expose()
    }
}

impl Eq for Pasted {}

impl fmt::Debug for Pasted {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Pasted(<redacted>)")
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
    use super::{CAPACITY, Pasted, Secret, scrub};

    fn typed(text: &str) -> Secret {
        let mut secret = Secret::new();
        for (at, character) in text.chars().enumerate() {
            assert!(secret.insert(at, character));
        }
        secret
    }

    #[test]
    fn typing_and_correcting_reach_the_bytes_typed() {
        assert!(Secret::new().is_empty());
        let mut secret = typed("hello");
        assert_eq!(secret.chars(), 5);
        assert!(secret.remove_range(4, 5));
        assert_eq!(secret.expose(), b"hell");
        assert!(!secret.is_empty());
    }

    #[test]
    fn a_correction_removes_a_whole_character_not_a_byte() {
        let mut secret = typed("é");
        assert_eq!(secret.expose().len(), 2);
        assert_eq!(secret.chars(), 1);
        assert!(secret.remove_range(0, 1));
        assert!(secret.is_empty());
    }

    #[test]
    fn correcting_an_empty_buffer_says_so_rather_than_panicking() {
        let mut secret = Secret::new();
        assert!(!secret.remove_range(0, 1));
        assert!(!secret.remove_range(0, 0));
    }

    #[test]
    fn a_removal_wipes_the_tail_the_shift_left_leaves_behind() {
        // The one part of a removal a test can see. Over a slice the stale copy
        // is in plain view; inside a `Vec` that has been truncated it is not.
        let mut buffer = *b"correct horse";
        let (start, end) = (0, 8);
        buffer.copy_within(end.., start);
        let kept = buffer.len() - (end - start);
        scrub(&mut buffer, kept);
        assert_eq!(&buffer[..kept], b"horse");
        assert!(
            buffer[kept..].iter().all(|byte| *byte == 0),
            "the shift left leaves a live copy of the tail: {buffer:?}"
        );
    }

    #[test]
    fn an_index_past_the_end_clamps_rather_than_panicking() {
        let mut secret = typed("abc");
        assert!(secret.insert(99, 'd'));
        assert_eq!(secret.expose(), b"abcd");
        assert!(secret.remove_range(2, 99));
        assert_eq!(secret.expose(), b"ab");
        assert!(!secret.remove_range(99, 200));
    }

    #[test]
    fn a_character_lands_at_the_caret_and_not_at_the_end() {
        let mut secret = typed("ac");
        assert!(secret.insert(1, 'b'));
        assert_eq!(secret.expose(), b"abc");
        assert!(secret.insert(0, 'z'));
        assert_eq!(secret.expose(), b"zabc");
    }

    #[test]
    fn a_range_removal_takes_the_middle_out() {
        let mut secret = typed("one two three");
        assert!(secret.remove_range(4, 8));
        assert_eq!(secret.expose(), b"one three");
    }

    #[test]
    fn the_buffer_refuses_to_grow_rather_than_reallocating() {
        let mut secret = Secret::new();
        let before = secret.expose().as_ptr();
        for at in 0..CAPACITY {
            assert!(secret.insert(at, 'x'));
        }
        assert!(!secret.insert(0, 'x'), "the buffer is full");
        assert_eq!(secret.expose().len(), CAPACITY);
        // Every mutating path, not just the growing one: a removal that moved
        // the allocation would leave the old one holding the whole secret.
        assert!(secret.remove_range(0, 1));
        assert!(secret.insert(0, 'y'));
        assert!(secret.remove_range(0, 2));
        assert!(secret.insert_str(0, "yz"));
        assert!(secret.transpose(1));
        assert_eq!(
            secret.expose().as_ptr(),
            before,
            "the allocation must never move, or the old one keeps the bytes"
        );
    }

    #[test]
    fn a_multibyte_character_that_would_overflow_is_refused_whole() {
        let mut secret = Secret::new();
        for at in 0..CAPACITY - 1 {
            assert!(secret.insert(at, 'x'));
        }
        assert!(!secret.insert(0, 'é'), "two bytes will not fit in one");
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
    fn a_paste_is_inserted_whole_or_not_at_all() {
        let mut secret = typed("ad");
        assert!(secret.insert_str(1, "bc"));
        assert_eq!(secret.expose(), b"abcd");
        assert!(
            !secret.insert_str(0, &"x".repeat(CAPACITY)),
            "it does not fit beside what is already there"
        );
        assert_eq!(
            secret.expose(),
            b"abcd",
            "and half of it must not be left in the field"
        );
    }

    #[test]
    fn a_paste_that_exactly_fills_the_field_is_taken() {
        let mut secret = Secret::new();
        assert!(secret.insert_str(0, &"x".repeat(CAPACITY)));
        assert_eq!(secret.expose().len(), CAPACITY);
        assert!(!secret.insert_str(0, "x"));
    }

    #[test]
    fn a_transposition_swaps_the_pair_at_the_caret() {
        let mut secret = typed("abcd");
        assert!(secret.transpose(2));
        assert_eq!(secret.expose(), b"acbd");
    }

    #[test]
    fn a_transposition_at_the_end_swaps_the_last_two() {
        let mut secret = typed("abcd");
        assert!(secret.transpose(4));
        assert_eq!(secret.expose(), b"abdc");
    }

    #[test]
    fn a_transposition_moves_whole_characters_of_different_widths() {
        let mut secret = typed("aé");
        assert!(secret.transpose(2));
        assert_eq!(secret.text(), Some("éa"));
        assert_eq!(secret.chars(), 2);
    }

    #[test]
    fn a_transposition_with_nothing_to_swap_says_so() {
        let mut secret = typed("a");
        assert!(!secret.transpose(1));
        assert!(!secret.transpose(0));
        assert!(!Secret::new().transpose(0));
    }

    #[test]
    fn text_is_none_when_a_pipe_delivered_something_that_is_not_text() {
        assert_eq!(typed("héllo").text(), Some("héllo"));
        assert_eq!(Secret::from_bytes(&[0xff, 0xfe]).unwrap().text(), None);
    }

    #[test]
    fn clearing_leaves_nothing_behind() {
        let mut secret = typed("hunter2");
        assert!(secret.remove_range(0, secret.chars()));
        assert!(secret.is_empty());
        assert_eq!(secret.chars(), 0);
    }

    #[test]
    fn two_entries_compare_for_the_double_entry() {
        let make = |text: &str| Secret::from_bytes(text.as_bytes()).unwrap();
        assert!(make("a b c").same_as(&make("a b c")));
        assert!(!make("a b c").same_as(&make("a b d")));
        assert!(!make("a").same_as(&make("ab")));
    }

    #[test]
    fn a_paste_cannot_be_printed_either() {
        let pasted = Pasted::new("hunter2".to_owned());
        assert_eq!(pasted.expose(), "hunter2");
        assert_eq!(format!("{pasted:?}"), "Pasted(<redacted>)");
        assert!(!format!("{pasted:#?}").contains("hunter"));
    }

    #[test]
    fn it_cannot_be_printed() {
        let secret = Secret::from_bytes(b"hunter2").unwrap();
        assert_eq!(format!("{secret:?}"), "Secret(<redacted>)");
        assert!(!format!("{secret:#?}").contains("hunter"));
    }
}
