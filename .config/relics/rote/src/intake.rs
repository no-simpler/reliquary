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
//!
//! **A bounded retry says which try this is, and every refusal spends one.**
//! Both halves are load-bearing and neither is worth anything alone. A bound
//! nobody can see is indistinguishable from a loop, so a person retypes until
//! they give up on the command; and one refusal that costs nothing makes the
//! loop unbounded however much the others cost. [`tried`] is the one composer
//! for the first half, shared with the drill try and the proof, so a fourth
//! retry cannot grow without one.

use anyhow::{Context as _, Result};

use crate::secret::Secret;
use crate::verifier::{SALT_LEN, Verifier};

/// What the line under the field says when the two entries differed.
pub const DIFFERED: &str = "the two entries differ";

/// What it says when nothing was typed.
pub const EMPTY: &str = "an empty secret is not a secret";

/// What it says when a proof was refused.
pub const NOT_CURRENT: &str = "not the current secret";

/// What it says when a drill try was wrong.
pub const NOT_IT: &str = "not it";

/// What the line under the field says when a bounded retry has spent a round.
///
/// **One composer for every bounded retry in the binary**, because a loop that
/// does not say which try this is reads as a loop that has no end — and the
/// only difference between the two, to the person in front of it, is whether
/// somebody remembered to say so. The drill try, the proof and the double entry
/// all come through here, so a fourth cannot quietly grow without one.
#[must_use]
pub fn tried(reason: &str, attempt: u8, of: u8) -> String {
    format!("{reason} · try {} of {of}", attempt.clamp(1, of.max(1)))
}

/// Where a double entry stands.
#[derive(Debug)]
pub enum Pairing {
    /// One is in hand. Ask for it again.
    Again,
    /// Both rounds matched.
    Agreed(Box<Secret>),
    /// They differed, and there is another round to spend.
    Differed,
    /// The rounds ran out, and this is what spent the last one.
    OutOfRounds(&'static str),
    /// Nothing was typed, and there is another round to spend.
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

    /// What the line under the field says after a round was spent.
    ///
    /// Asked for after the round is spent, so the number it names is the try
    /// about to be made rather than the one just lost — the same reading the
    /// drill and the proof give.
    #[must_use]
    pub fn status(&self, reason: &str) -> String {
        tried(reason, self.spent.saturating_add(1), self.rounds)
    }

    /// Offer one typed entry.
    pub fn offer(&mut self, secret: Secret) -> Pairing {
        if secret.is_empty() {
            self.first = None;
            return self.spend(EMPTY, Pairing::Empty);
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
                    self.spend(DIFFERED, Pairing::Differed)
                }
            }
        }
    }

    /// Spend one round on a refusal, or say there are none left.
    ///
    /// **Every refusal spends one**, an empty entry included. An entry that
    /// costs nothing is a round that never runs out, and a loop with one
    /// costless key in it is unbounded however many rounds the others spend.
    fn spend(&mut self, reason: &'static str, refusal: Pairing) -> Pairing {
        self.spent = self.spent.saturating_add(1);
        if self.spent >= self.rounds {
            Pairing::OutOfRounds(reason)
        } else {
            refusal
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
        assert!(matches!(
            pair.offer(secret("b")),
            Pairing::OutOfRounds(super::DIFFERED)
        ));
    }

    #[test]
    fn an_empty_entry_is_refused_and_drops_whatever_was_held() {
        let mut pair = Pair::new(3);
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(matches!(pair.offer(Secret::new()), Pairing::Empty));
        assert!(!pair.holds_one());
    }

    #[test]
    fn an_empty_entry_spends_a_round_like_every_other_refusal() {
        // A refusal that costs nothing is a round that never runs out, and one
        // costless key makes the whole loop unbounded.
        let mut pair = Pair::new(3);
        assert!(matches!(pair.offer(Secret::new()), Pairing::Empty));
        assert!(matches!(pair.offer(Secret::new()), Pairing::Empty));
        assert!(matches!(
            pair.offer(Secret::new()),
            Pairing::OutOfRounds(super::EMPTY)
        ));
    }

    #[test]
    fn the_last_round_spent_is_what_the_closing_card_names() {
        let mut pair = Pair::new(2);
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(matches!(pair.offer(secret("b")), Pairing::Differed));
        assert!(
            matches!(
                pair.offer(Secret::new()),
                Pairing::OutOfRounds(super::EMPTY)
            ),
            "the reason is the one that ran the budget out, not the one before it"
        );
    }

    #[test]
    fn every_refusal_says_which_try_the_next_one_is() {
        let mut pair = Pair::new(3);
        assert_eq!(
            pair.status(super::DIFFERED),
            "the two entries differ · try 1 of 3"
        );
        pair.offer(secret("a"));
        pair.offer(secret("b"));
        assert_eq!(
            pair.status(super::DIFFERED),
            "the two entries differ · try 2 of 3"
        );
    }

    #[test]
    fn a_try_count_never_reads_past_its_own_bound() {
        assert_eq!(super::tried("no", 9, 3), "no · try 3 of 3");
        assert_eq!(super::tried("no", 0, 3), "no · try 1 of 3");
    }

    #[test]
    fn a_pair_always_has_at_least_one_round() {
        let mut pair = Pair::new(0);
        assert!(matches!(pair.offer(secret("a")), Pairing::Again));
        assert!(matches!(
            pair.offer(secret("b")),
            Pairing::OutOfRounds(super::DIFFERED)
        ));
    }
}
