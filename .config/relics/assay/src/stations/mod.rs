//! The built-in stations.
//!
//! A check lives here only when nothing else owns it. Anything a relic knows
//! about its own health stays in that relic and reaches the run through the
//! registry adapter, which probes `<name> doctor --format json`.

pub mod bedrock;
pub mod md_blocks;
