//! What more than one relic needs.
//!
//! Admission is by demand, not by anticipation: code moves here when a **second**
//! relic needs it, and not before. A shared crate that collects what might one
//! day be shared is a god crate, and every relic pays for it.
//!
//! Both modules exist because two relics keyed a project by the same code and it
//! diverged. [`path::project_key`] is that key, and it is the reason the rest is
//! here — [`git`] because a key derived from an ambient repository is the wrong
//! key, [`path`] because a key that depends on how a path was spelled is not one
//! key.

pub mod fs;
pub mod git;
pub mod path;
