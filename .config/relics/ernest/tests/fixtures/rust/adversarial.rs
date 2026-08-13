#![deny(missing_docs)]
//! Every slash below that is not a comment, and every comment shape the
//! language offers. The first line begins `#!` and is an attribute, not a
//! shebang.

// SPDX-License-Identifier: MIT
// =========================
//// Four slashes are a comment, but deliberately not a doc comment.

use std::fmt;

/// Resolves the tenant for a request.
///
/// # Examples
///
/// ```
/// let t = tenant(1); // a comment inside a doc fence
/// ```
#[derive(Debug, Clone)]
#[doc = "see http://example.com // not a comment"]
pub struct Tenant {
    /// The identifier.
    pub id: u32,
}

#[allow(dead_code)]
fn decoys<'a>(s: &'a str) -> &'a str {
    let url = "http://not-a-comment/#frag";
    let block = "/* not a comment */";
    let raw = r"C:\path\ // still a string";
    let hashed = r#"a "quoted" // thing"#;
    let deeper = r##"contains "# inside"##;
    let bytes = b"bytes // here";
    let escaped = "escaped quote \" then // not a comment";
    let slash = '/';
    let quote = '\'';
    let pair = ('a', 'b'); // a real comment after unpaired-looking quotes

    'outer: loop {
        break 'outer;
    }

    let _ = (url, block, raw, hashed, deeper, bytes, escaped, slash, quote, pair);
    s
}

/* outer /* inner */ still outer */
fn arithmetic(a: usize, b: usize) -> usize {
    a / b / 2
}

fn macros(f: &mut fmt::Formatter<'_>) {
    println!(
        "{}", // a comment inside a token tree
        1
    );
    let _ = vec![1, 2, /* three */ 3];
    let _ = write!(f, "//");
}

macro_rules! passthrough {
    // A comment in the matcher region.
    ($x:expr) => {
        /* and one in the expansion */
        $x
    };
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_expands() {
        assert_eq!(passthrough!(1), 1, "msg // not a comment");
    }
}
