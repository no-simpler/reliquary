use anyhow::Result;

use super::{View, aligned, hit_row};
use crate::ui::plural;

/// Aligned and uncoloured. Padding is worth its bytes here because it is what
/// lets a reader — model or person — scan one column instead of parsing every
/// line, and the trailing tagline column is never padded.
pub fn list(view: &View<'_>) -> Result<()> {
    match view.project {
        Some(project) => println!("docket {}", project.display()),
        None => println!("docket {}", plural(view.projects(), "project", "projects")),
    }
    if view.hits.is_empty() {
        println!(
            "{}",
            if view.narrowed {
                "(no match)"
            } else {
                "(empty)"
            }
        );
        return Ok(());
    }

    let rows: Vec<_> = view
        .hits
        .iter()
        .map(|hit| hit_row(hit, view.project.is_none()))
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
