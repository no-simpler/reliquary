use anyhow::Result;

use super::{View, aligned, row};

/// Aligned and uncoloured. Padding is worth its bytes here because it is what
/// lets a reader — model or person — scan one column instead of parsing every
/// line, and the trailing tagline column is never padded.
pub fn list(view: &View<'_>) -> Result<()> {
    println!("docket {}", view.project.display());
    if view.records.is_empty() {
        println!("(empty)");
        return Ok(());
    }

    let rows: Vec<_> = view
        .records
        .iter()
        .enumerate()
        .map(|(index, record)| row(index + 1, record))
        .collect();
    let (lines, head) = aligned(&rows, "");
    let pad = " ".repeat(head);

    for (line, row) in lines.iter().zip(&rows) {
        println!("{line}");
        for note in &row.notes {
            println!("{pad}{note}");
        }
    }
    Ok(())
}
