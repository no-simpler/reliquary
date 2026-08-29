//! What more than one relic needs.
//!
//! Admission splits by kind. **Platform** code — colour, atomic writes, locking,
//! subprocess capability, PATH resolution — is reuse-first: its second consumer
//! exists by construction. **Domain** code keeps the second-consumer gate, because
//! the wrong abstraction costs more than the duplication.
//!
//! [`path::project_key`] is where it started: two relics keyed a project by the
//! same code and it diverged. [`git`] is here because a key derived from an
//! ambient repository is the wrong key, [`path`] because a key that depends on
//! how a path was spelled is not one key, [`fs`] because both stores had written
//! the same tmp-then-rename by hand and both had the same collision in it, and
//! [`frontmatter`] because a third relic then needed the same document split.
//!
//! [`ui`] and [`fmt`] are the platform half: one answer to "who is reading this"
//! and one spelling for the quantities that answer reports. [`finding`] is the
//! same move for verification: the machine's checks live in several programs, and
//! this is the only thing they agree on.
//!
//! Paths are [`camino::Utf8Path`]. A relic's paths are program data — keys it
//! compares, stores and prints — and `to_string_lossy` maps two directories onto
//! one key. [`path::utf8`] is the parse; past it, a path is a string.

// A platform crate documents its whole surface: doctests only carry weight if
// they exist. `lints.workspace = true` is exclusive of every other entry in a
// package's [lints] table, so a deliberately per-crate lint has no table form.
#![deny(missing_docs)]
// Off workspace-wide, because it is noise on a binary whose `pub` only crosses
// modules. Here `pub` is the API every relic writes against.
#![deny(clippy::must_use_candidate)]

pub mod finding;
pub mod fmt;
pub mod frontmatter;
pub mod fs;
pub mod git;
pub mod lock;
pub mod path;
pub mod tool;
pub mod ui;
