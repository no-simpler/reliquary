//! Prose density: the share of a codebase's text that is prose rather than code.
//!
//! The pipeline is walk → detect → classify → measure → aggregate → report.
//! Classification works in byte spans rather than lines, so a line carrying
//! both code and a trailing comment splits between them instead of being
//! forced whole into one bucket.

pub mod aggregate;
pub mod analyze;
pub mod detect;
pub mod report;
pub mod span;
pub mod walk;
