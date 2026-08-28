//! Removing what a tool left behind and would rebuild without noticing.
//!
//! Two lanes, because the question "may this be deleted?" has two different
//! best answers. Inside a git repository the repository itself answers, so a
//! per-repository unignore is respected without this program knowing the rule.
//! Outside one there is nobody to ask, so the answer is by name alone, and the
//! set of names is deliberately small.

#![deny(missing_docs)]
// Off workspace-wide, because it is noise on a binary whose `pub` only crosses
// modules. Here `pub` is the API the binary and its tests write against.
#![deny(clippy::must_use_candidate)]

pub mod cruft;
pub mod ignored;
pub mod plan;
pub mod walk;

pub use plan::{Doomed, Plan};
