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
//! the same tmp-then-rename by hand and both had the same collision in it.
//!
//! [`ui`] and [`fmt`] are the platform half: one answer to "who is reading this"
//! and one spelling for the quantities that answer reports.

// A platform crate documents its whole surface: doctests only carry weight if
// they exist. `lints.workspace = true` is exclusive of every other entry in a
// package's [lints] table, so a deliberately per-crate lint has no table form.
#![deny(missing_docs)]

pub mod fmt;
pub mod fs;
pub mod git;
pub mod path;
pub mod ui;
