//! Path to profile.

use std::io::Read;
use std::path::Path;

use crate::analyze::profiles::{PROFILES, Profile};

/// Bytes read looking for a shebang. Anything past this is not a first line.
const PEEK: usize = 256;

/// The profile for `path`, or `None` when the format is unsupported.
///
/// Exact filenames win over extensions, so a name-only format can override a
/// misleading suffix later. An extension is the author's declaration of format,
/// so only its absence makes the shebang worth opening the file for — which is
/// what identifies the extensionless scripts a personal bin directory is
/// made of.
pub fn profile_for(path: &Path) -> Option<&'static Profile> {
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && let Some(profile) = PROFILES
            .iter()
            .find(|p| p.filenames.iter().any(|f| f.eq_ignore_ascii_case(name)))
    {
        return Some(profile);
    }

    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        return PROFILES
            .iter()
            .find(|p| p.extensions.iter().any(|e| e.eq_ignore_ascii_case(ext)))
            .copied();
    }

    let line = first_line(path)?;
    let named = interpreter(&line)?;
    PROFILES
        .iter()
        .find(|p| p.interpreters.contains(&named))
        .copied()
}

/// The length of `src`'s shebang line, or `None` when the first line is not one.
///
/// Opening with `#!` is not the test. Rust spells an inner attribute
/// `#![deny(missing_docs)]`, which is the most common first line there is in
/// that language and is code, not an unavoidable header — so what makes a
/// shebang is naming an interpreter.
pub(crate) fn shebang_len(src: &str) -> Option<usize> {
    let end = src.find('\n').unwrap_or(src.len());
    interpreter(&src[..end])?;
    Some(end)
}

/// The interpreter a shebang names: the basename of the program, or of the
/// first argument that is neither a flag nor an assignment when that program
/// is `env`.
///
/// The program is required to be an absolute path, which every real shebang
/// uses because the kernel does no lookup. It is also what separates a shebang
/// from the other things a line may open with — `#![…]` above all.
fn interpreter(line: &str) -> Option<&str> {
    let mut words = line.strip_prefix("#!")?.split_whitespace();
    let program = words.next()?;
    if !program.starts_with('/') {
        return None;
    }
    let first = basename(program);
    if first != "env" {
        return Some(first);
    }
    words
        .find(|word| !word.starts_with('-') && !word.contains('='))
        .map(basename)
}

fn basename(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// The file's first line, up to `PEEK` bytes. Read as bytes rather than to a
/// string: this runs against every extensionless file, binaries included.
fn first_line(path: &Path) -> Option<String> {
    let mut buffer = [0u8; PEEK];
    let read = std::fs::File::open(path).ok()?.read(&mut buffer).ok()?;
    let head = &buffer[..read];
    let end = head.iter().position(|b| *b == b'\n').unwrap_or(read);
    std::str::from_utf8(&head[..end]).ok().map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn language_of(path: &str) -> Option<&'static str> {
        profile_for(Path::new(path)).map(|p| p.language)
    }

    #[test]
    fn recognises_supported_extensions() {
        assert_eq!(language_of("a/b.php"), Some("php"));
        assert_eq!(language_of("a/b.PHP"), Some("php"));
        assert_eq!(language_of("c.yml"), Some("yaml"));
        assert_eq!(language_of("c.yaml"), Some("yaml"));
        assert_eq!(language_of("scripts/publish.sh"), Some("shell"));
        assert_eq!(language_of("interactive.d/030-config.bash"), Some("shell"));
        assert_eq!(language_of("src/main.rs"), Some("rust"));
        assert_eq!(language_of("Cargo.toml"), Some("toml"));
        assert_eq!(language_of("stubs/redis.phpstub"), Some("php"));
    }

    /// TSX is TypeScript and reports as such — the second grammar is an
    /// artifact of JSX and the angle-bracket type assertion being unable to
    /// share one, not a distinction the breakdown should carry.
    #[test]
    fn recognises_the_ecmascript_extensions() {
        for path in ["app.js", "app.mjs", "app.cjs", "Banner.jsx"] {
            assert_eq!(language_of(path), Some("javascript"), "{path}");
        }
        for path in ["app.ts", "app.mts", "app.cts", "Panel.tsx", "types.d.ts"] {
            assert_eq!(language_of(path), Some("typescript"), "{path}");
        }
    }

    /// The two grammars are not interchangeable, which is what makes the split
    /// necessary rather than tidy.
    #[test]
    fn tsx_and_typescript_resolve_to_different_grammars() {
        let ts = profile_for(Path::new("app.ts")).unwrap();
        let tsx = profile_for(Path::new("app.tsx")).unwrap();
        assert_eq!(ts.language, tsx.language);
        assert!(!std::ptr::eq(ts, tsx));
    }

    /// A shell dotfile carries neither an extension nor a shebang, so its name
    /// is the only thing left to go on.
    #[test]
    fn recognises_shell_dotfiles_by_name() {
        assert_eq!(language_of("home/.bashrc"), Some("shell"));
        assert_eq!(language_of("home/.bash_profile"), Some("shell"));
        assert_eq!(language_of("home/.profile"), Some("shell"));
    }

    #[test]
    fn declines_everything_else() {
        assert!(profile_for(Path::new("Makefile")).is_none());
        assert!(profile_for(Path::new("c.zsh")).is_none());
        assert!(profile_for(Path::new("c.fish")).is_none());
        // Generated, and TOML — kept out by not claiming its extension.
        assert!(profile_for(Path::new("Cargo.lock")).is_none());
    }

    #[test]
    fn a_shebang_names_its_interpreter() {
        assert_eq!(interpreter("#!/bin/bash"), Some("bash"));
        assert_eq!(interpreter("#!/bin/sh"), Some("sh"));
        assert_eq!(interpreter("#!/bin/bash -e"), Some("bash"));
        assert_eq!(interpreter("#!/usr/bin/env bash"), Some("bash"));
        assert_eq!(
            interpreter("#!/usr/bin/env -S bash -euo pipefail"),
            Some("bash")
        );
        assert_eq!(interpreter("#!/usr/bin/env FOO=1 python3"), Some("python3"));
        assert_eq!(interpreter("#!/usr/bin/env php"), Some("php"));
    }

    #[test]
    fn a_line_that_is_not_a_shebang_names_nothing() {
        assert_eq!(interpreter("# a comment"), None);
        assert_eq!(interpreter(""), None);
        assert_eq!(interpreter("#!"), None);
        assert_eq!(interpreter("#!/usr/bin/env"), None);
        assert_eq!(interpreter("#!/usr/bin/env -S"), None);
    }

    /// A Rust file opening with an inner attribute begins `#!` and is not a
    /// shebang. Billing that line as unavoidable would write off the most
    /// common first line in the language.
    #[test]
    fn an_inner_attribute_is_not_a_shebang() {
        for line in [
            "#![deny(missing_docs)]",
            "#![allow(clippy::all)]",
            "#! [allow(dead_code)]",
            "#![doc = include_str!(\"../README.md\")]",
        ] {
            assert_eq!(interpreter(line), None, "{line}");
            assert_eq!(shebang_len(line), None, "{line}");
        }
        assert_eq!(shebang_len("#!/bin/bash\nx=1\n"), Some(11));
    }

    #[test]
    fn an_extensionless_script_is_found_by_its_shebang() {
        let dir = std::env::temp_dir().join("ernest-detect");
        std::fs::create_dir_all(&dir).unwrap();

        let script = dir.join("tool");
        std::fs::write(&script, "#!/usr/bin/env bash\nset -euo pipefail\n").unwrap();
        assert_eq!(profile_for(&script).map(|p| p.language), Some("shell"));

        // An interpreter no profile claims, and a file with no shebang at all.
        let other = dir.join("other");
        std::fs::write(&other, "#!/usr/bin/env ruby\nputs 1\n").unwrap();
        assert!(profile_for(&other).is_none());

        let plain = dir.join("plain");
        std::fs::write(&plain, "just text\n").unwrap();
        assert!(profile_for(&plain).is_none());
    }

    /// An extension is a declaration of format; a shebang under one is noise.
    #[test]
    fn an_extension_is_never_second_guessed() {
        let dir = std::env::temp_dir().join("ernest-detect");
        std::fs::create_dir_all(&dir).unwrap();

        let decoy = dir.join("notes.txt");
        std::fs::write(&decoy, "#!/usr/bin/env bash\n").unwrap();
        assert!(profile_for(&decoy).is_none());
    }
}
