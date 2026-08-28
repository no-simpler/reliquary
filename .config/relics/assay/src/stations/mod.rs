//! The built-in stations.
//!
//! A check lives here only when nothing else owns it. Anything a relic knows
//! about its own health stays in that relic and reaches the run through the
//! registry adapter, which probes `<name> doctor --format json`.

pub mod bedrock;
pub mod brew_health;
pub mod git_identity;
pub mod md_blocks;
pub mod path;
pub mod perf_budgets;
pub mod registry;
pub mod relic_cache;
pub mod shell_lint;
pub mod shell_parity;
pub mod shell_startup;
pub mod yadm_coverage;
