//! Machine-readable snapshots. These are what `ernest diff` consumes, so the
//! shape is a contract: `schema_version` moves whenever it changes.

use anyhow::{Context, Result};
use std::path::Path;

use crate::aggregate::{Report, SCHEMA_VERSION};

/// The snapshot `ernest diff` consumes.
///
/// # Errors
///
/// When the report will not serialise.
pub fn render(report: &Report) -> Result<String> {
    serde_json::to_string_pretty(report).context("serialising the report")
}

/// Just enough of a snapshot to check it before trusting its shape.
#[derive(serde::Deserialize)]
struct Stamp {
    schema_version: u32,
}

/// Read a snapshot back.
///
/// # Errors
///
/// When the file cannot be read, when it is not the JSON this writes, or when it
/// carries a different `schema_version` — which is told apart from the rest, so a
/// stale snapshot is named as stale rather than as malformed.
pub fn load(path: &Path) -> Result<Report> {
    let text = fs_err::read_to_string(path)?;
    // The version is read on its own first: a snapshot from another schema
    // should be told what is wrong with it, not fail on a field that schema
    // never had.
    let stamp: Stamp = serde_json::from_str(&text)
        .with_context(|| format!("parsing snapshot {}", path.display()))?;
    if stamp.schema_version != SCHEMA_VERSION {
        anyhow::bail!(
            "snapshot {} has schema_version {}, this ernest writes {} — re-measure it",
            path.display(),
            stamp.schema_version,
            SCHEMA_VERSION
        );
    }
    serde_json::from_str(&text).with_context(|| format!("parsing snapshot {}", path.display()))
}
