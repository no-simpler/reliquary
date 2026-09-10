//! The name of a drilled secret.
//!
//! By convention a slug matches the title of the 1Password item holding the same
//! secret. Convention rather than integration: nothing is stored, nothing goes
//! out of sync, no `op://` vector is added, and the drill does not fail in
//! exactly the world escrow exists for.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// Longest slug accepted. It is a table column, a map key and a prompt label.
pub const MAX: usize = 32;

/// A validated slug.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Slug(String);

impl Slug {
    /// The slug as a string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Why a string is not a slug.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum Error {
    /// Nothing was given.
    #[error("a slug cannot be empty")]
    Empty,
    /// Longer than [`MAX`].
    #[error("a slug is at most {MAX} characters, and this one is {0}")]
    TooLong(usize),
    /// A character outside the accepted set.
    #[error("a slug holds lowercase letters, digits and hyphens, and this one holds {0:?}")]
    BadCharacter(char),
    /// A leading or trailing hyphen.
    #[error("a slug starts and ends with a letter or a digit")]
    BadEdge,
}

impl FromStr for Slug {
    type Err = Error;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty() {
            return Err(Error::Empty);
        }
        if text.chars().count() > MAX {
            return Err(Error::TooLong(text.chars().count()));
        }
        if let Some(bad) = text
            .chars()
            .find(|c| !(c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '-'))
        {
            return Err(Error::BadCharacter(bad));
        }
        if text.starts_with('-') || text.ends_with('-') {
            return Err(Error::BadEdge);
        }
        Ok(Self(text.to_owned()))
    }
}

impl TryFrom<String> for Slug {
    type Error = Error;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<Slug> for String {
    fn from(slug: Slug) -> Self {
        slug.0
    }
}

impl fmt::Display for Slug {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, MAX, Slug};

    #[test]
    fn an_ordinary_name_parses() {
        assert_eq!("escrow-p".parse::<Slug>().unwrap().as_str(), "escrow-p");
    }

    #[test]
    fn the_accepted_set_is_lowercase_digits_and_hyphens() {
        assert_eq!("Escrow".parse::<Slug>(), Err(Error::BadCharacter('E')));
        assert_eq!("a b".parse::<Slug>(), Err(Error::BadCharacter(' ')));
        assert_eq!("a_b".parse::<Slug>(), Err(Error::BadCharacter('_')));
        assert!("a1-b2".parse::<Slug>().is_ok());
    }

    #[test]
    fn a_hyphen_may_not_sit_at_either_edge() {
        assert_eq!("-a".parse::<Slug>(), Err(Error::BadEdge));
        assert_eq!("a-".parse::<Slug>(), Err(Error::BadEdge));
    }

    #[test]
    fn an_empty_or_overlong_name_is_refused() {
        assert_eq!(String::new().parse::<Slug>(), Err(Error::Empty));
        let long = "a".repeat(MAX + 1);
        assert_eq!(long.parse::<Slug>(), Err(Error::TooLong(MAX + 1)));
        assert!("a".repeat(MAX).parse::<Slug>().is_ok());
    }

    #[test]
    fn the_wire_form_round_trips_and_refuses_a_bad_one() {
        let slug: Slug = "op-master".parse().unwrap();
        let json = serde_json::to_string(&slug).unwrap();
        assert_eq!(json, "\"op-master\"");
        assert_eq!(serde_json::from_str::<Slug>(&json).unwrap(), slug);
        assert!(serde_json::from_str::<Slug>("\"Op Master\"").is_err());
    }
}
