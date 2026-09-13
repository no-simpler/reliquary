//! Taking a secret twice, and turning it into a verifier.
//!
//! Pure, and deliberately not in the dialog layer: the sitting needs the same
//! double-entry policy for an attachment, and it cannot use the dialog helpers
//! because it draws a different card. One policy, two callers, no second copy.
//!
//! **Double entry exists to catch a typo at a keyboard.** A blind field gives no
//! other way to find a slip, and a command that exits on one makes a person
//! retype everything from the start — so a mismatch asks again rather than
//! failing. A pipe cannot mistype, which is why `--stdin` reads each distinct
//! secret exactly once.

use anyhow::{Context as _, Result};

use crate::secret::Secret;
use crate::verifier::{SALT_LEN, Verifier};

/// What the line under the field says when the two entries differed.
pub const DIFFERED: &str = "the two entries differ";

/// What it says when nothing was typed.
pub const EMPTY: &str = "an empty secret is not a secret";

/// What it says when a proof was refused.
pub const NOT_CURRENT: &str = "not the current secret";

/// Where a double entry stands.
#[derive(Debug)]
pub enum Pairing {
    /// One is in hand. Ask for it again.
    Again,
    /// Both rounds matched.
    Agreed(Box<Secret>),
    /// They differed, and there is another round to spend.
    Differed,
    /// They differed, and there is not.
    OutOfRounds,
    /// Nothing was typed.
    Empty,
}

/// A secret being taken twice.
///
/// Holds at most one secret at a time, and drops it the moment the pair either
/// agrees or is abandoned.
#[derive(Debug, Default)]
pub struct Pair {
    first: Option<Secret>,
    spent: u8,
    rounds: u8,
}

impl Pair {
    /// A pair with this many rounds to spend on disagreement.
    pub fn new(rounds: u8) -> Self {
        Self {
            first: None,
            spent: 0,
            rounds: rounds.max(1),
        }
    }

    /// Whether a first entry is in hand, and so what the prompt should say.
    pub fn holds_one(&self) -> bool {
        self.first.is_some()
    }

    /// How many rounds have been spent on disagreement.
    pub fn spent(&self) -> u8 {
        self.spent
    }

    /// Offer one typed entry.
    pub fn offer(&mut self, secret: Secret) -> Pairing {
        if secret.is_empty() {
            self.first = None;
            return Pairing::Empty;
        }
        match self.first.take() {
            None => {
                self.first = Some(secret);
                Pairing::Again
            }
            Some(first) => {
                if first.same_as(&secret) {
                    Pairing::Agreed(Box::new(secret))
                } else {
                    self.spent = self.spent.saturating_add(1);
                    if self.spent >= self.rounds {
                        Pairing::OutOfRounds
                    } else {
                        Pairing::Differed
                    }
                }
            }
        }
    }
}

/// Hash a secret under this binary's parameters, over a fresh random salt.
///
/// # Errors
///
/// When the system will not supply randomness, or argon2 refuses.
pub fn make_verifier(secret: &Secret) -> Result<Verifier> {
    let mut salt = [0u8; SALT_LEN];
    getrandom::fill(&mut salt).context("drawing a salt")?;
    Verifier::create(secret, &salt).context("hashing the secret")
}

#[cfg(test)]
mod tests {
    use super::{Pair, Pairing};
    use crate::secret::Secret;

    fn secret(text: &str) -> Secret {
        Secret::from_bytes(text.as_bytes()).unwrap()
    }

    #[test]
    fn two_that_agree_take_one_round() {
        let mut pair = Pair::new(3);
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(pair.holds_one());
        assert!(matches!(pair.offer(secret("a")), Pairing::Agreed(_)));
        assert_eq!(pair.spent(), 0);
    }

    #[test]
    fn two_that_differ_ask_again_until_the_rounds_run_out() {
        let mut pair = Pair::new(2);
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(matches!(pair.offer(secret("b")), Pairing::Differed));
        assert!(!pair.holds_one(), "a disagreement starts over");
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(matches!(pair.offer(secret("b")), Pairing::OutOfRounds));
    }

    #[test]
    fn an_empty_entry_is_refused_and_drops_whatever_was_held() {
        let mut pair = Pair::new(3);
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(matches!(pair.offer(Secret::new()), Pairing::Empty));
        assert!(!pair.holds_one());
    }

    #[test]
    fn a_pair_always_has_at_least_one_round() {
        let mut pair = Pair::new(0);
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(matches!(pair.offer(secret("b")), Pairing::OutOfRounds));
    }
}
