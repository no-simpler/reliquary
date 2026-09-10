//! The verifier: argon2id over a random salt, in PHC string format.
//!
//! The secret is held in no recoverable form, so the tool cannot reveal it on a
//! failure — it can only say *wrong*.
//!
//! **Cost.** The memory floor matches `age -p`'s measured 262 MiB, on one rule
//! from `design/custody.md`: a verifier weaker than the artifact it verifies
//! becomes the cheaper attack path. Measured on the flagship, 2026-09-10, in a
//! release build: at `m=262144 t=4 p=1` one verification costs 468 ms, against
//! `age -p`'s 450 ms. `t` was raised from 1 (112 ms) to reach it; `p` buys
//! nothing, since lanes divide the same memory rather than adding passes over
//! it.
//!
//! **Where a verifier lives is a separate question**, and it is not in `ark`.
//! See `store` and the relic's `CLAUDE.md`: a verifier is an offline-attackable
//! confirmation oracle, and it is regenerable by re-enrollment, so it has no
//! business in a tree that replicates to two providers and keeps prior versions.

use argon2::password_hash::{Ident, Output, ParamsString, PasswordHash, Salt, SaltString};
use argon2::{Algorithm, Argon2, Block, Params, Version};
use zeroize::Zeroize as _;

use crate::secret::Secret;

/// Memory cost, in KiB. The floor, and what this binary writes.
pub const M_COST_KIB: u32 = 262_144;

/// Time cost. Chosen by measurement so one verification is not cheaper in wall
/// time than the artifact whose memory cost it matches.
pub const T_COST: u32 = 4;

/// Lanes. One: lanes divide the memory rather than adding passes over it, so
/// raising this would weaken the verifier while looking like hardening.
pub const P_COST: u32 = 1;

/// Digest length, in bytes.
pub const OUTPUT_LEN: usize = 32;

/// Salt length, in bytes.
pub const SALT_LEN: usize = 16;

/// What can go wrong around a verifier.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The cost parameters were refused by the implementation.
    #[error("argon2 refused the cost parameters")]
    Params(#[source] argon2::Error),
    /// Hashing itself failed.
    #[error("argon2 could not hash")]
    Hash(#[source] argon2::Error),
    /// The stored string is not a PHC string this binary can read.
    #[error("the stored verifier is not a readable PHC string")]
    Stored(#[source] argon2::password_hash::Error),
    /// Assembling the PHC string failed.
    #[error("could not render the verifier as a PHC string")]
    Render(#[source] argon2::password_hash::Error),
}

/// A stored verifier, as a PHC string.
///
/// The format is the Password Hashing Competition's own, so every verifier
/// carries the parameters it was made with. `doctor` reads the cost off the
/// string rather than trusting a constant that may since have moved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Verifier(String);

impl Verifier {
    /// Take a stored string at face value, checking only that it parses.
    ///
    /// # Errors
    ///
    /// When the string is not a PHC string.
    pub fn parse(text: &str) -> Result<Self, Error> {
        PasswordHash::new(text).map_err(Error::Stored)?;
        Ok(Self(text.to_owned()))
    }

    /// Hash a secret under this binary's parameters.
    ///
    /// # Errors
    ///
    /// When argon2 refuses the parameters or the salt, or the PHC string will
    /// not assemble.
    pub fn create(secret: &Secret, salt: &[u8; SALT_LEN]) -> Result<Self, Error> {
        let params = params()?;
        let encoded = SaltString::encode_b64(salt).map_err(Error::Render)?;
        let output = digest_bytes(secret, salt, &params)?;
        let hash = PasswordHash {
            algorithm: Ident::new(argon2::ARGON2ID_IDENT.as_str()).map_err(Error::Render)?,
            version: Some(u32::from(Version::V0x13)),
            params: ParamsString::try_from(&params).map_err(Error::Render)?,
            salt: Some(encoded.as_salt()),
            hash: Some(Output::new(&output).map_err(Error::Render)?),
        };
        Ok(Self(hash.to_string()))
    }

    /// Whether a secret matches. The comparison is constant time.
    ///
    /// # Errors
    ///
    /// When the stored string will not parse, or argon2 refuses the parameters
    /// it carries.
    pub fn accepts(&self, secret: &Secret) -> Result<bool, Error> {
        let stored = PasswordHash::new(&self.0).map_err(Error::Stored)?;
        let params = Params::try_from(&stored).map_err(Error::Stored)?;
        let expected = stored
            .hash
            .ok_or(Error::Stored(argon2::password_hash::Error::PhcStringField))?;
        let salt = stored
            .salt
            .ok_or(Error::Stored(argon2::password_hash::Error::PhcStringField))?;
        let mut salt_bytes = [0u8; Salt::MAX_LENGTH];
        let salt_bytes = salt
            .decode_b64(&mut salt_bytes)
            .map_err(Error::Stored)?
            .to_vec();
        let params = Params::new(
            params.m_cost(),
            params.t_cost(),
            params.p_cost(),
            Some(expected.len()),
        )
        .map_err(Error::Params)?;
        let output = digest_bytes(secret, &salt_bytes, &params)?;
        let actual = Output::new(&output).map_err(Error::Stored)?;
        // `Output`'s `PartialEq` is `subtle`'s constant-time comparison.
        Ok(actual == expected)
    }

    /// The memory cost the stored string was made with.
    ///
    /// # Errors
    ///
    /// When the stored string will not parse.
    pub fn m_cost_kib(&self) -> Result<u32, Error> {
        let stored = PasswordHash::new(&self.0).map_err(Error::Stored)?;
        Ok(Params::try_from(&stored).map_err(Error::Stored)?.m_cost())
    }

    /// Whether the stored string meets this binary's memory floor.
    ///
    /// # Errors
    ///
    /// When the stored string will not parse.
    pub fn meets_floor(&self) -> Result<bool, Error> {
        Ok(self.m_cost_kib()? >= M_COST_KIB)
    }

    /// The PHC string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn params() -> Result<Params, Error> {
    Params::new(M_COST_KIB, T_COST, P_COST, Some(OUTPUT_LEN)).map_err(Error::Params)
}

/// Hash into a buffer we own, so the working memory can be wiped afterwards.
///
/// The crate's own `hash_password` allocates its block array internally and
/// drops it without zeroizing. Inverting that state is as hard as inverting
/// `BLAKE2b`, so this is belt rather than braces — but the belt costs one `Vec`.
fn digest_bytes(secret: &Secret, salt: &[u8], params: &Params) -> Result<Vec<u8>, Error> {
    let argon = Argon2::new(Algorithm::Argon2id, Version::V0x13, params.clone());
    let mut blocks = vec![Block::default(); params.block_count()];
    let mut out = vec![0u8; params.output_len().unwrap_or(OUTPUT_LEN)];
    let result = argon.hash_password_into_with_memory(secret.expose(), salt, &mut out, &mut blocks);
    blocks.zeroize();
    result.map_err(Error::Hash)?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use argon2::password_hash::{PasswordHasher as _, PasswordVerifier as _, SaltString};
    use argon2::{Algorithm, Argon2, Params, Version};

    use super::{M_COST_KIB, OUTPUT_LEN, P_COST, SALT_LEN, T_COST, Verifier};
    use crate::secret::Secret;

    /// The real parameters cost 468 ms a call and 256 MiB. Unit tests use a
    /// cheap set and cross-check against the reference implementation, which is
    /// what actually proves the hand-assembled PHC string is right.
    fn cheap() -> Params {
        Params::new(64, 1, 1, Some(OUTPUT_LEN)).unwrap()
    }

    fn secret(text: &str) -> Secret {
        Secret::from_bytes(text.as_bytes()).unwrap()
    }

    fn reference_phc(text: &str, salt: &[u8; SALT_LEN]) -> String {
        let salt = SaltString::encode_b64(salt).unwrap();
        Argon2::new(Algorithm::Argon2id, Version::V0x13, cheap())
            .hash_password(text.as_bytes(), &salt)
            .unwrap()
            .to_string()
    }

    #[test]
    fn a_string_this_binary_assembles_is_read_by_the_reference_implementation() {
        let salt = [9u8; SALT_LEN];
        let phc = super::digest_bytes(&secret("hunter2"), &salt, &cheap()).unwrap();
        let encoded = SaltString::encode_b64(&salt).unwrap();
        let ours = Argon2::new(Algorithm::Argon2id, Version::V0x13, cheap())
            .hash_password(b"hunter2", &encoded)
            .unwrap();
        assert_eq!(
            ours.hash.unwrap().as_bytes(),
            phc.as_slice(),
            "our digest must be the crate's digest"
        );
    }

    #[test]
    fn a_reference_string_is_accepted_by_our_verifier() {
        let salt = [3u8; SALT_LEN];
        let verifier = Verifier::parse(&reference_phc("correct horse", &salt)).unwrap();
        assert!(verifier.accepts(&secret("correct horse")).unwrap());
        assert!(!verifier.accepts(&secret("correct hors")).unwrap());
        assert!(!verifier.accepts(&secret("")).unwrap());
    }

    #[test]
    fn our_string_is_accepted_by_the_reference_verifier() {
        let salt = [5u8; SALT_LEN];
        let phc = reference_phc("seven words go here and here too", &salt);
        let parsed = argon2::password_hash::PasswordHash::new(&phc).unwrap();
        assert!(
            Argon2::default()
                .verify_password(b"seven words go here and here too", &parsed)
                .is_ok()
        );
    }

    #[test]
    fn a_verifier_carries_its_own_cost_and_answers_the_floor() {
        let salt = [1u8; SALT_LEN];
        let weak = Verifier::parse(&reference_phc("x", &salt)).unwrap();
        assert_eq!(weak.m_cost_kib().unwrap(), 64);
        assert!(!weak.meets_floor().unwrap());
    }

    #[test]
    fn the_shipped_parameters_meet_the_floor_they_declare() {
        let params = super::params().unwrap();
        assert_eq!(params.m_cost(), M_COST_KIB);
        assert_eq!(params.t_cost(), T_COST);
        assert_eq!(params.p_cost(), P_COST);
        const { assert!(M_COST_KIB >= 262_144, "at or above age -p's 262 MiB") };
    }

    #[test]
    fn a_string_that_is_not_a_phc_string_is_refused() {
        assert!(Verifier::parse("not a verifier").is_err());
        assert!(Verifier::parse("").is_err());
    }

    /// The real cost, kept out of the fast loop. `relic test` must stay fast and
    /// a debug build pays 6.2 s a call.
    #[test]
    #[ignore = "costs 256 MiB and half a second in release, six seconds in debug"]
    fn the_shipped_parameters_round_trip() {
        let phrase = secret("this is the whole point of the exercise");
        let verifier = Verifier::create(&phrase, &[7u8; SALT_LEN]).unwrap();
        assert!(verifier.meets_floor().unwrap());
        assert!(verifier.accepts(&phrase).unwrap());
        assert!(
            !verifier
                .accepts(&secret("something else entirely"))
                .unwrap()
        );
    }
}
