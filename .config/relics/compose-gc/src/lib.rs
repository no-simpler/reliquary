//! Reclaiming Docker Compose state left behind by git worktrees that are gone.
//!
//! A worktree removed without tearing its stack down first leaves state behind
//! with the compose file gone. `docker compose down` can no longer reach such a
//! project — it needs the file — so leftovers pile up: a surviving stack keeps a
//! headless browser (sometimes a whole database) alive, and even a fully-exited
//! one pins its volumes for good. Sweeping by **label** needs no compose file.
//!
//! Leftovers come in two shapes and both must be handled, because a
//! container-only sweep goes blind the moment a stack's containers are gone and
//! its volumes are not — which is the steady state after any teardown that
//! omitted `-v`.
//!
//! [`docker`] is everything the daemon is asked, [`layout`] which stacks are
//! this repository's, [`plan`] what that makes of them, and [`render`] how it
//! reads. Planning is pure over one snapshot, so `--dry-run` and a real run are
//! one computation rather than two that agree.

#![deny(missing_docs)]
// Off workspace-wide, because it is noise on a binary whose `pub` only crosses
// modules. Here `pub` is the API the binary and its tests write against.
#![deny(clippy::must_use_candidate)]

pub mod compose;
pub mod docker;
pub mod layout;
pub mod plan;
pub mod project;
pub mod render;

pub use layout::{Anchor, Liveness};
pub use plan::{Plan, Reaping, Reason};
pub use project::ProjectName;
