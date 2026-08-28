//! What may never reach a public tree, read from data.
//!
//! The definition is encrypted, because it names what it protects. This module
//! only knows how to read it and how its parts compose — and composition lives
//! here alone, because two composers is how two consumers of one definition
//! come to disagree about what it means.

use camino::{Utf8Path, Utf8PathBuf};
use regex::RegexBuilder;
use serde::Deserialize;

/// Where the definition sits when nothing says otherwise: beside the hook that
/// was its first consumer.
const DEFAULT: &str = ".config/yadm/hooks/identity-guard.toml";

/// Why the definition could not be used.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// It is not on disk. On this machine that means the archive has not been
    /// decrypted, which is a remedy rather than a defect.
    #[error("{0} is not readable — run 'yadm decrypt'")]
    Absent(Utf8PathBuf),
    /// It is there but could not be read.
    #[error("reading {0}")]
    Unreadable(Utf8PathBuf, #[source] std::io::Error),
    /// It is there but is not the shape this expects.
    #[error("{0}: {1}")]
    Malformed(Utf8PathBuf, #[source] toml::de::Error),
    /// It parsed but would refuse nothing, which is worse than no guard: it
    /// would pass every commit while reporting that it checked.
    #[error("{0} defines nothing to test for")]
    Empty(Utf8PathBuf),
    /// Its parts do not compose into a usable expression.
    #[error("{0} does not compose into a pattern")]
    Uncompilable(Utf8PathBuf, #[source] regex::Error),
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Document {
    guard: Table,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Table {
    #[serde(default)]
    keywords: Vec<String>,
    #[serde(default)]
    character_class: String,
}

/// The definition, compiled.
#[derive(Debug)]
pub struct Definition {
    keywords: Vec<(String, regex::Regex)>,
    class: Option<regex::Regex>,
    combined: regex::Regex,
}

impl Definition {
    /// The definition at its default place under `home`.
    ///
    /// # Errors
    ///
    /// Whatever [`Definition::load`] reports.
    pub fn discover(home: &Utf8Path) -> Result<Self, Error> {
        Self::load(&home.join(DEFAULT))
    }

    /// The definition at `path`.
    ///
    /// # Errors
    ///
    /// [`Error`], every variant of which names the file.
    pub fn load(path: &Utf8Path) -> Result<Self, Error> {
        let text = match fs_err::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Err(Error::Absent(path.to_owned()));
            }
            Err(e) => return Err(Error::Unreadable(path.to_owned(), e)),
        };
        let document: Document =
            toml::from_str(&text).map_err(|e| Error::Malformed(path.to_owned(), e))?;
        Self::compile(path, document.guard)
    }

    fn compile(path: &Utf8Path, table: Table) -> Result<Self, Error> {
        let terms: Vec<String> = table
            .keywords
            .into_iter()
            .filter(|k| !k.trim().is_empty())
            .collect();
        let class = table.character_class.trim().to_owned();
        if terms.is_empty() && class.is_empty() {
            return Err(Error::Empty(path.to_owned()));
        }

        let build = |source: &str| {
            RegexBuilder::new(source)
                .case_insensitive(true)
                .build()
                .map_err(|e| Error::Uncompilable(path.to_owned(), e))
        };

        let mut keywords = Vec::with_capacity(terms.len());
        for term in &terms {
            keywords.push((term.clone(), build(&regex::escape(term))?));
        }
        let compiled_class = if class.is_empty() {
            None
        } else {
            Some(build(&class)?)
        };

        // One alternation over the same parts, so the cheap pre-filter and the
        // per-term reports can never disagree about whether a file is clean.
        let mut parts: Vec<String> = Vec::new();
        if !class.is_empty() {
            parts.push(class);
        }
        parts.extend(terms.iter().map(|t| regex::escape(t)));
        let combined = build(&parts.join("|"))?;

        Ok(Self {
            keywords,
            class: compiled_class,
            combined,
        })
    }

    /// The one-pass pre-filter: whether anything at all is worth reporting.
    #[must_use]
    pub fn matches(&self, haystack: &str) -> bool {
        self.combined.is_match(haystack)
    }

    /// The character class, when one is defined.
    #[must_use]
    pub fn class(&self) -> Option<&regex::Regex> {
        self.class.as_ref()
    }

    /// Each term and the expression that finds it.
    #[must_use]
    pub fn keywords(&self) -> &[(String, regex::Regex)] {
        &self.keywords
    }
}
