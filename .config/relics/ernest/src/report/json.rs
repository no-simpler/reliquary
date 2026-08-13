//! Machine-readable snapshots. These are what `ernest diff` consumes, so the
//! shape is a contract: `schema_version` moves whenever it changes.

use anyhow::{Context, Result};
use std::path::Path;

use crate::aggregate::{Report, SCHEMA_VERSION};

pub fn render(report: &Report) -> Result<String> {
    serde_json::to_string_pretty(report).context("serialising the report")
}

/// Just enough of a snapshot to check it before trusting its shape.
#[derive(serde::Deserialize)]
struct Stamp {
    schema_version: u32,
}

pub fn load(path: &Path) -> Result<Report> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("reading snapshot {}", path.display()))?;
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
