use anyhow::Result;
use serde_json::{Value, json};

use super::View;
use crate::item::{Item, Rung};
use crate::store::Record;

pub fn list(view: &View<'_>) -> Result<()> {
    let items: Vec<Value> = view
        .records
        .iter()
        .enumerate()
        .map(|(index, record)| record_json(index + 1, record))
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "project": view.project,
            "items": items,
        }))?
    );
    Ok(())
}

pub fn record_json(position: usize, record: &Record) -> Value {
    match &record.item {
        Ok(item) => {
            let mut value = item_json(item);
            value["position"] = json!(position);
            value["path"] = json!(record.path);
            value["valid"] = json!(true);
            value
        }
        Err(error) => json!({
            "position": position,
            "id": record.id.to_string(),
            "path": record.path,
            "valid": false,
            "error": error,
        }),
    }
}

pub fn item_json(item: &Item) -> Value {
    let chain = item.rung.chain();
    json!({
        "id": item.id.to_string(),
        "kind": item.kind().to_string(),
        "stage": match &item.rung {
            Rung::Spec { stage, .. } => Some(stage.to_string()),
            _ => None,
        },
        "title": item.title,
        "description": item.description,
        "project": item.project,
        "created": item.created.to_string(),
        "updated": item.updated.to_string(),
        "order": item.order,
        "blocked": item.blocked,
        "origin": item.origin,
        "tags": item.tags,
        "chain": chain.map(|c| c.chain.to_string()),
        "hop": chain.map(|c| c.hop),
        "supersedes": chain.and_then(|c| c.supersedes).map(|s| s.to_string()),
    })
}
