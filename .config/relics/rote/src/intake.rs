//! Taking a secret twice, and turning it into a verifier.
//!
//! Pure, and deliberately not in the dialog layer: the policy is one thing and
//! the card it is drawn on is another, so a second caller draws its own card
//! over the same [`Pair`] rather than growing a second copy of the rule.
//!
//! **Double entry exists to catch a typo at a keyboard.** A masked field gives no
//! other way to find a slip. Two entries that differ write nothing: the pair is
//! dropped, the card says so, and the command exits — there is no third go,
//! because a bound is a thing to see and to count and this tool now does
//! neither. An empty entry is refused and the first half is kept. A pipe cannot
//! mistype, which is why `--stdin` reads each secret exactly once.

use anyhow::{Context as _, Result};

use crate::secret::Secret;
use crate::verifier::{SALT_LEN, Verifier};

/// What the line under the field says when the two entries differed.
pub const DIFFERED: &str = "the two entries differ";

/// What it says when nothing was typed.
pub const EMPTY: &str = "an empty secret is not a secret";

/// What every prompt that takes a secret says it is for. One spelling each,
/// read by the sitting's card and by the commands alike.
pub const ENROLL_INTENTION: &str = "the secret this lineage will hold, typed twice";

/// The attachment prompt, which is the one place a wrong answer is accepted in
/// silence and written into the record as truth.
pub const ATTACH_INTENTION: &str = "type it as you know it — nothing here can check it";

/// The new secret a rotation takes.
pub const ROTATE_INTENTION: &str = "the new secret, typed twice";

/// The second half of a double entry.
pub const AGAIN: &str = "again";

/// Where a double entry stands.
#[derive(Debug)]
pub enum Pairing {
    /// One is in hand. Ask for it again.
    Again,
    /// Both entries matched.
    Agreed(Box<Secret>),
    /// They differed. The pair is gone, and so is the command.
    Differed,
    /// Nothing was typed. Refused, and whatever was in hand is still in hand.
    Empty,
}

/// A secret being taken twice.
///
/// Holds at most one secret at a time, and drops it the moment the pair either
/// agrees or differs.
#[derive(Debug, Default)]
pub struct Pair {
    first: Option<Secret>,
}

impl Pair {
    /// A pair with nothing in hand.
    pub fn new() -> Self {
        Self::default()
    }

    /// Whether a first entry is in hand, and so what the prompt should say.
    pub fn holds_one(&self) -> bool {
        self.first.is_some()
    }

    /// Offer one typed entry.
    pub fn offer(&mut self, secret: Secret) -> Pairing {
        if secret.is_empty() {
            // Refused rather than conceded: there is nothing here to concede
            // to, and the half already typed is not made wrong by it.
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
                    Pairing::Differed
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

/// A verifier re-minted at this binary's parameters, when the one held is
/// below the floor and the secret has just been proved against it.
///
/// The one moment the plaintext is in hand is the one moment a stale verifier
/// can be brought up to cost without asking for anything. `None` when the
/// secret was refused, or when the held one already meets the floor.
///
/// # Errors
///
/// When the held string will not parse, or the fresh one cannot be made.
pub fn refreshed(held: &Verifier, secret: &Secret) -> Result<Option<Verifier>> {
    if held.meets_floor()? || !held.accepts(secret)? {
        return Ok(None);
    }
    make_verifier(secret).map(Some)
}

#[cfg(test)]
mod tests {
    use super::{Pair, Pairing};
    use crate::secret::Secret;

    fn secret(text: &str) -> Secret {
        Secret::from_bytes(text.as_bytes()).unwrap()
    }

    #[test]
    fn two_that_agree_are_one_secret() {
        let mut pair = Pair::new();
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(pair.holds_one());
        assert!(matches!(pair.offer(secret("a")), Pairing::Agreed(_)));
        assert!(!pair.holds_one(), "an agreed pair holds nothing");
    }

    #[test]
    fn two_that_differ_end_the_pair_and_drop_what_it_held() {
        let mut pair = Pair::new();
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(matches!(pair.offer(secret("b")), Pairing::Differed));
        assert!(!pair.holds_one(), "a disagreement drops the first half");
    }

    #[test]
    fn an_empty_entry_is_refused_and_keeps_whatever_was_held() {
        let mut pair = Pair::new();
        assert!(matches!(pair.offer(Secret::new()), Pairing::Empty));
        assert!(!pair.holds_one(), "nothing was there to keep");
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(matches!(pair.offer(Secret::new()), Pairing::Empty));
        assert!(pair.holds_one(), "the first half is not made wrong by it");
        assert!(matches!(pair.offer(secret("a")), Pairing::Agreed(_)));
    }

    /// A verifier below the floor, for the secret `x`.
    fn weak() -> crate::verifier::Verifier {
        use argon2::password_hash::{PasswordHasher as _, SaltString};
        use argon2::{Algorithm, Argon2, Params, Version};
        let salt = SaltString::encode_b64(&[3u8; crate::verifier::SALT_LEN]).unwrap();
        let cheap = Params::new(64, 1, 1, Some(crate::verifier::OUTPUT_LEN)).unwrap();
        let phc = Argon2::new(Algorithm::Argon2id, Version::V0x13, cheap)
            .hash_password(b"x", &salt)
            .unwrap()
            .to_string();
        crate::verifier::Verifier::parse(&phc).unwrap()
    }

    #[test]
    fn a_proved_secret_brings_a_stale_verifier_up_to_cost_and_a_refused_one_does_not() {
        let held = weak();
        assert!(!held.meets_floor().unwrap());
        assert!(
            super::refreshed(&held, &secret("y")).unwrap().is_none(),
            "nothing is re-minted for a secret the verifier refused"
        );
        // One mint at the shipped cost: the same budget the integration suite
        // spends once per run.
        let fresh = super::refreshed(&held, &secret("x"))
            .unwrap()
            .expect("a stale verifier is re-minted for a proved secret");
        assert!(fresh.meets_floor().unwrap());
        assert!(fresh.accepts(&secret("x")).unwrap());
        assert!(
            super::refreshed(&fresh, &secret("x")).unwrap().is_none(),
            "one at the floor is left alone"
        );
    }
}
