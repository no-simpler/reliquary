//! Print a file's parse tree, so a profile can be written against the node
//! kinds a grammar actually returns rather than the ones its grammar.js
//! suggests. Anonymous tokens are marked, since those are nameable too — YAML's
//! `---` is one.
//!
//! ```
//! cargo run --example kinds -- src/main.rs
//! cargo run --example kinds -- --lang toml some-file-with-no-extension
//! ```
//!
//! The grammar is chosen the way the tool chooses it, so this needs no editing
//! when a profile is added — only that the profile exists.

use std::path::Path;

use ernest::analyze::parse;
use ernest::analyze::profiles::{PROFILES, Profile};
use ernest::detect::profile_for;
use tree_sitter::Node;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut forced: Option<&Profile> = None;
    let mut target = None;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--lang" => {
                let name = args.next().expect("--lang wants a language");
                forced = PROFILES.iter().find(|p| p.language == name).copied();
                assert!(forced.is_some(), "no profile named {name}");
            }
            _ => target = Some(arg),
        }
    }
    let target = target.expect("usage: kinds [--lang NAME] PATH");
    let path = Path::new(&target);

    let profile = forced
        .or_else(|| profile_for(path))
        .unwrap_or_else(|| panic!("no profile for {target}; pass --lang"));
    let src = std::fs::read_to_string(path)?;
    let tree = parse(&src, profile)?;

    println!("{} as {}", path.display(), profile.language);
    dump(tree.root_node(), &src, 0);
    Ok(())
}

fn dump(node: Node, src: &str, depth: usize) {
    let text: String = src[node.start_byte()..node.end_byte()].chars().take(48).collect();
    println!(
        "{}{}{} {:?}",
        "  ".repeat(depth),
        node.kind(),
        if node.is_named() { "" } else { " (anonymous)" },
        text.replace('\n', "\\n"),
    );
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        dump(child, src, depth + 1);
    }
}
