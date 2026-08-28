//! The commit guard: what may never reach a public tree, and the test for it.
//!
//! A library, because the thing that invokes it changes and the logic should
//! not. Today a yadm hook; the same call is an ordinary git hook, or a station
//! inside an aggregator.
//!
//! Two halves that stay apart on purpose. [`Definition`] is *what* counts —
//! read from data, never from code, so adding a term is a data edit. [`scan`]
//! is the test, and knows nothing about where the definition came from.

#![deny(missing_docs)]
// Off workspace-wide, because it is noise on a binary whose `pub` only crosses
// modules. Here `pub` is the API the hook and its tests write against.
#![deny(clippy::must_use_candidate)]

pub mod config;
pub mod definition;
pub mod scan;
pub mod staged;

pub use config::Config;
pub use definition::Definition;
pub use scan::{Finding, Verdict};
