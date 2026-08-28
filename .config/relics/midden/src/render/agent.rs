use anyhow::Result;

use relic_core::fmt::plural;

use super::{Digest, View, aligned, heading, row};

/// Aligned and uncoloured. Padding is worth its bytes here because it is what
/// lets a reader — model or person — scan one column instead of parsing every
/// line, and the unbounded title column is never padded.
pub fn list(view: &View<'_>) -> Result<()> {
    println!("{}", heading(view.scope, view.records.len()));
    if view.records.is_empty() {
        println!("(empty)");
        return Ok(());
    }

    let rows: Vec<_> = view
        .records
        .iter()
        .enumerate()
        .map(|(index, record)| row(index + 1, record, view.now))
        .collect();
    let (lines, head) = aligned(&rows, "");
    let pad = " ".repeat(head);

    for (line, row) in lines.iter().zip(&rows) {
        println!("{line}");
        for note in std::iter::once(&row.detail).chain(&row.notes) {
            if !note.is_empty() {
                println!("{pad}{note}");
            }
        }
    }
    Ok(())
}

/// One section per place a fix would land, heaviest first — the shape a review
/// session works down.
pub fn digest(view: &Digest<'_>) -> Result<()> {
    let total: usize = view.groups.iter().map(|group| group.records.len()).sum();
    println!("{}", heading(view.scope, total));
    if view.groups.is_empty() {
        println!("(empty)");
        return Ok(());
    }

    for (index, group) in view.groups.iter().enumerate() {
        if index > 0 {
            println!();
        }
        println!(
            "{}  [{}]",
            group.target,
            plural(group.records.len(), "note", "notes")
        );
        let rows: Vec<_> = group
            .records
            .iter()
            .enumerate()
            .map(|(position, record)| row(position + 1, record, view.now))
            .collect();
        let (lines, head) = aligned(&rows, "  ");
        let pad = " ".repeat(head);
        for (line, row) in lines.iter().zip(&rows) {
            println!("{line}");
            if !row.detail.is_empty() {
                println!("{pad}{}", row.detail);
            }
        }
    }
    Ok(())
}
