use anyhow::Result;
use serde_json::{Value, json};

use super::View;
use crate::item::{Item, Rung};
use crate::query::Hit;

/// One shape, whichever scope produced it: every item carries its own project,
/// so a top-level one would be a second copy of the same fact — and across
/// projects it would be a copy that is wrong for all but one row.
pub fn list(view: &View<'_>) -> Result<()> {
    let items: Vec<Value> = view.hits.iter().map(hit_json).collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "items": items }))?
    );
    Ok(())
}

fn hit_json(hit: &Hit) -> Value {
    let record = &hit.record;
    let mut value = match &record.item {
        Ok(item) => {
            let mut value = item_json(item);
            if let Some(map) = value.as_object_mut() {
                map.insert("valid".into(), json!(true));
            }
            value
        }
        // The keys the shelf can answer for are still answered, so the shape
        // does not change with whether an item parsed.
        Err(error) => json!({
            "id": record.id.to_string(),
            "kind": record.kind.to_string(),
            "project": record.project,
            "valid": false,
            "error": error,
        }),
    };
    // Indexing a `Value` panics when it is not an object; asking for the map says
    // the same thing and cannot.
    if let Some(map) = value.as_object_mut() {
        map.insert("position".into(), json!(hit.position));
        map.insert("path".into(), json!(record.path));
        if let Some(excerpt) = &hit.excerpt {
            map.insert("excerpt".into(), json!(excerpt));
        }
    }
    value
}

pub fn item_json(item: &Item) -> Value {
    let chain = item.rung.chain();
    json!({
        "id": item.id.to_string(),
        "kind": item.kind().to_string(),
        "stage": match &item.rung {
            Rung::Spec { stage, .. } => Some(stage.to_string()),
            Rung::Handoff | Rung::Relay(_) => None,
        },
        "name": item.name,
        "tagline": item.tagline,
        "project": item.project,
        "created": item.created.to_string(),
        "updated": item.updated.to_string(),
        "order": item.order,
        "blocked": item.blocked,
        "origin": item.origin,
        "tags": item.tags,
        "chain": chain.map(|c| c.id.to_string()),
        "hop": chain.map(|c| c.hop),
        "supersedes": chain.and_then(|c| c.supersedes).map(|s| s.to_string()),
    })
}
