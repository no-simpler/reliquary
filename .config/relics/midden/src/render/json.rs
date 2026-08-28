use anyhow::Result;
use serde_json::{Value, json};

use super::{Digest, View};
use crate::note::Note;
use crate::store::Record;

pub fn list(view: &View<'_>) -> Result<()> {
    let notes: Vec<Value> = view
        .records
        .iter()
        .enumerate()
        .map(|(index, record)| record_json(index + 1, record))
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope": view.scope,
            "notes": notes,
        }))?
    );
    Ok(())
}

pub fn digest(view: &Digest<'_>) -> Result<()> {
    let groups: Vec<Value> = view
        .groups
        .iter()
        .map(|group| {
            json!({
                "target": group.target,
                "weight": group.weight(),
                "notes": group
                    .records
                    .iter()
                    .enumerate()
                    .map(|(index, record)| record_json(index + 1, record))
                    .collect::<Vec<_>>(),
            })
        })
        .collect();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "scope": view.scope,
            "groups": groups,
        }))?
    );
    Ok(())
}

pub fn record_json(position: usize, record: &Record) -> Value {
    match &record.note {
        Ok(note) => {
            let mut value = note_json(note);
            // Indexing a `Value` panics when it is not an object; asking for the
            // map says the same thing and cannot.
            if let Some(map) = value.as_object_mut() {
                map.insert("position".into(), json!(position));
                map.insert("path".into(), json!(record.path));
                map.insert("archived".into(), json!(record.archived));
                map.insert("valid".into(), json!(true));
            }
            value
        }
        Err(error) => json!({
            "position": position,
            "id": record.id.to_string(),
            "path": record.path,
            "archived": record.archived,
            "valid": false,
            "error": error,
        }),
    }
}

pub fn note_json(note: &Note) -> Value {
    json!({
        "id": note.id.to_string(),
        "kind": note.kind.to_string(),
        "title": note.title,
        "detail": note.detail,
        "target": note.target,
        "status": note.status.to_string(),
        "occurrences": note.occurrences,
        "project": note.project,
        "cwd": note.cwd,
        "branch": note.branch,
        "session": note.session,
        "created": note.created.to_string(),
        "updated": note.updated.to_string(),
        "seen": note.seen.iter().map(std::string::ToString::to_string).collect::<Vec<_>>(),
        "fingerprint": note.fingerprint,
    })
}
