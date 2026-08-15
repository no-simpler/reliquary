pub mod agent;
pub mod human;
pub mod json;

use std::path::Path;

use anyhow::Result;

use crate::store::Record;
use crate::ui::Format;

pub struct View<'a> {
    pub project: &'a Path,
    pub records: &'a [Record],
    pub color: bool,
}

pub fn list(view: &View<'_>, format: Format) -> Result<()> {
    match format {
        Format::Human => human::list(view),
        Format::Agent => agent::list(view),
        Format::Json => json::list(view),
    }
}

/// The badge every renderer shows for an item's rung: kind, plus the one
/// qualifier that rung carries.
pub fn kind_badge(item: &crate::item::Item) -> String {
    use crate::item::Rung;
    match &item.rung {
        Rung::Handoff => "handoff".to_owned(),
        Rung::Relay(chain) => format!("relay hop={}", chain.hop),
        Rung::Spec { stage, .. } => format!("spec/{stage}"),
    }
}
