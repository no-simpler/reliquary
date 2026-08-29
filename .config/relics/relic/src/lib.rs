//! Managing Reliquary relics: the pipeline this relic is itself the first
//! member of.
//!
//! A **relic** is a personal tool the author keeps, at one of three stages —
//! a one-shot script, an in-house directory with a manifest, or an independent
//! repository. This is the surface over stages 1 and 2: what exists, what is
//! published, what has drifted, and the gates each one passes before it lands
//! on `PATH`.
//!
//! Two things about it are load-bearing and easy to lose.
//!
//! **It publishes everything else**, which is why it was the last thing this
//! programme rewrote and why nothing here may presuppose a published binary.
//! The bootstrap seed that produces *this* binary on a bare machine cannot be
//! this binary; that is a fixed point, not a preference.
//!
//! **Discovery is attic-safe.** A relic is surfaced only when its manifest is
//! *readable*, so an encrypted private lane reveals nothing — not a name, not a
//! count. A manifest that is readable but broken is reported rather than
//! skipped: silence there is how a relic disappears.

pub mod doctor;
pub mod external;
pub mod gate;
pub mod lane;
pub mod manifest;
pub mod paths;
pub mod publish;
pub mod ratchet;
pub mod registry;
pub mod render;
pub mod scaffold;
