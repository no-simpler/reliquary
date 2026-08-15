use anyhow::Result;

use super::{View, kind_badge};
use crate::ui::age;

/// Unaligned and uncoloured: padding buys nothing when the reader is a model,
/// and every column of whitespace is paid for twice, once written and once read.
pub fn list(view: &View<'_>) -> Result<()> {
    println!("docket {}", view.project.display());
    if view.records.is_empty() {
        println!("(empty)");
        return Ok(());
    }

    for (index, record) in view.records.iter().enumerate() {
        match &record.item {
            Ok(item) => {
                let blocked = if item.is_blocked() { " blocked" } else { "" };
                println!(
                    "{} {} {} {}{} {}",
                    index + 1,
                    record.id,
                    kind_badge(item),
                    age(item.created),
                    blocked,
                    item.title
                );
                for line in item.description.lines().filter(|l| !l.trim().is_empty()) {
                    println!("    {}", line.trim_end());
                }
                if let Some(reason) = item.blocked.as_deref() {
                    for line in reason.lines().filter(|l| !l.trim().is_empty()) {
                        println!("    blocked: {}", line.trim_end());
                    }
                }
            }
            Err(error) => {
                println!("! {} INVALID {}", record.id, error);
                println!("    {}", record.path.display());
            }
        }
    }
    Ok(())
}
