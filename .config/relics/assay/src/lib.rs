//! The machine's verification surface: many checks, one finding shape.
//!
//! `assay` is an aggregator, not a tool that absorbs everything. A station lives
//! here only when nothing else owns the check; a relic that knows its own health
//! keeps that knowledge and answers `doctor --format json`, which the registry
//! adapter collects. The dependency direction never reverses.
//!
//! Everything the runner speaks is [`relic_core::finding`] — the contract is
//! platform, so a station in another program is the same kind of thing as one in
//! this crate.

pub mod probe;
pub mod render;
pub mod repo;
pub mod roster;
pub mod run;
pub mod station;
pub mod stations;

pub use station::{Context, Station};
