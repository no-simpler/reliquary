//! How a relic starts.
//!
//! `scaffold` promotes a Stage-1 one-shot from `~/.config/bin` — or lays down a
//! fresh idea — as a Stage-2 relic. It is the one moment the runtime stance is
//! cheap to follow, so it is the moment it is enforced: **relics are Rust
//! unless exempted**, and anything else has to say why in the same breath.
//!
//! A promoted script's shebang wins over the stance, deliberately. Inference
//! reads what the script *is*; a rewrite is an explicit `-r rust`, not a silent
//! one that leaves the old script unrunnable.

use camino::{Utf8Path, Utf8PathBuf};

use crate::manifest::Runtime;

/// Why a name was refused.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid relic name: {0} (use letters, digits, dash, underscore)")]
pub struct BadName(pub String);

/// A relic name: the published binary's name.
///
/// Held as a type because it becomes a filename on `PATH`, a directory, a
/// cargo package and a `META_NAME` — four places that must agree, and one
/// parse.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Name(String);

impl Name {
    /// The name, as written.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::str::FromStr for Name {
    type Err = BadName;

    /// ```
    /// use relic::scaffold::Name;
    /// assert!("my-thing".parse::<Name>().is_ok());
    /// assert!("../evil".parse::<Name>().is_err());
    /// ```
    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let bad = |t: &str| BadName(t.to_owned());
        let first = text.chars().next().ok_or_else(|| bad(text))?;
        if first == '.' || first == '-' {
            return Err(bad(text));
        }
        if !text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(bad(text));
        }
        Ok(Self(text.to_owned()))
    }
}

/// The runtime a script's shebang names, when it names one this repo knows.
#[must_use]
pub fn infer_runtime(script: &Utf8Path) -> Option<Runtime> {
    let body = fs_err::read_to_string(script.as_std_path()).ok()?;
    let first = body.lines().next()?;
    let rest = first.strip_prefix("#!")?;
    if rest.contains("python") {
        return Some(Runtime::Python);
    }
    if rest.contains("fish") {
        return Some(Runtime::Fish);
    }
    if rest.contains("bash") {
        return Some(Runtime::Bash);
    }
    // `/bin/sh`, `/usr/bin/env sh`, `sh -e`: POSIX shell is written and tested
    // as bash here, which is the runtime that has a linter and a suite runner.
    let looks_like_sh = rest
        .split_whitespace()
        .any(|word| word == "sh" || word.ends_with("/sh"));
    looks_like_sh.then_some(Runtime::Bash)
}

/// Rewrite a `key = "..."` assignment, preserving any trailing `# comment` in
/// the column the file already uses.
///
/// The value is escaped for a TOML basic string. This is a targeted edit rather
/// than a parse-and-serialise on purpose: the template's comments are what make
/// a filled-in manifest read like the template it came from, and a round trip
/// through a serialiser drops every one of them.
#[must_use]
pub fn set_field(body: &str, key: &str, value: &str) -> String {
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    let mut out = String::new();
    for line in body.lines() {
        let assigns = line
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='));
        if !assigns {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        let mut replacement = format!("{key} = \"{escaped}\"");
        if let Some(column) = line.find('#') {
            let comment = line.get(column..).unwrap_or_default();
            while replacement.len() < column {
                replacement.push(' ');
            }
            if replacement.len() >= column && !replacement.ends_with(' ') {
                replacement.push(' ');
            }
            replacement.push_str(comment);
        }
        out.push_str(&replacement);
        out.push('\n');
    }
    out
}

/// Insert `name` into a cargo workspace's `members` array, keeping it sorted.
///
/// Idempotent, and the sort is what makes the next insert land predictably.
/// Assumes the multi-line array form, which is why the workspace manifest is
/// written that way.
#[must_use]
pub fn add_member(body: &str, name: &str) -> String {
    let row = format!("    \"{name}\",\n");
    let entry = format!("\"{name}\",");
    if body.contains(&entry) {
        return body.to_owned();
    }
    let mut out = String::new();
    let mut inside = false;
    let mut added = false;
    for line in body.lines() {
        if !inside && line.trim_start().starts_with("members") && line.contains('[') {
            inside = true;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if inside && line.starts_with(']') {
            if !added {
                out.push_str(&row);
                added = true;
            }
            inside = false;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if inside && !added {
            let existing: String = line
                .chars()
                .filter(|c| c.is_ascii_alphanumeric() || "_/*-".contains(*c))
                .collect();
            if existing.as_str() > name {
                out.push_str(&row);
                added = true;
            }
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// The cargo member manifest a fresh Rust relic gets.
#[must_use]
pub fn cargo_manifest(name: &str) -> String {
    format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         edition.workspace = true\n\
         rust-version.workspace = true\n\
         license.workspace = true\n\
         publish.workspace = true\n\
         \n\
         [[bin]]\n\
         name = \"{name}\"\n\
         path = \"src/main.rs\"\n\
         \n\
         [dependencies]\n"
    )
}

/// The `main.rs` a fresh Rust relic gets: something that runs, and one test.
///
/// The test is not decoration. `relic test` fails a package whose suite has
/// nothing in it — a gate with nothing to run is not a gate — so a skeleton
/// without one fails the very step the scaffold prints as next. It also puts
/// the first test in front of whoever writes the second.
#[must_use]
pub fn cargo_main(name: &str) -> String {
    const SKELETON: &str = r#"fn main() {
    println!("{}", greeting());
}

/// What this relic has to say for itself so far.
fn greeting() -> &'static str {
    "@NAME@"
}

#[cfg(test)]
mod tests {
    use super::greeting;

    #[test]
    fn it_says_its_own_name() {
        assert_eq!(greeting(), "@NAME@");
    }
}
"#;
    SKELETON.replace("@NAME@", name)
}

/// The `CLAUDE.md` stub a fresh relic gets.
#[must_use]
pub fn claude_md(name: &str) -> String {
    format!(
        "# `{name}` — in-house (Stage-2) relic\n\
         \n\
         Scaffolded from `~/.config/reliquary/template`. See\n\
         `~/.config/reliquary/GRADUATION.md` for the lifecycle, manifest schema, and\n\
         publish flow.\n\
         \n\
         TODO: describe what `{name}` does and any agent context worth keeping.\n"
    )
}

/// Copy a directory tree.
///
/// # Errors
///
/// Any filesystem refusal, naming the path.
pub fn copy_tree(from: &Utf8Path, to: &Utf8Path) -> std::io::Result<()> {
    fs_err::create_dir_all(to.as_std_path())?;
    for entry in fs_err::read_dir(from.as_std_path())? {
        let entry = entry?;
        let name = entry.file_name();
        let source = from.join(name.to_string_lossy().as_ref());
        let target = to.join(name.to_string_lossy().as_ref());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&source, &target)?;
        } else if kind.is_symlink() {
            let link = fs_err::read_link(source.as_std_path())?;
            #[cfg(unix)]
            std::os::unix::fs::symlink(&link, target.as_std_path())?;
            #[cfg(not(unix))]
            fs_err::copy(source.as_std_path(), target.as_std_path()).map(|_| ())?;
        } else {
            fs_err::copy(source.as_std_path(), target.as_std_path())?;
        }
    }
    Ok(())
}

/// Point a relative symlink at `target` from `link`.
///
/// # Errors
///
/// Any filesystem refusal.
pub fn symlink(target: &str, link: &Utf8PathBuf) -> std::io::Result<()> {
    let _ = fs_err::remove_file(link.as_std_path());
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(target, link.as_std_path())
    }
    #[cfg(not(unix))]
    {
        fs_err::write(link.as_std_path(), target)
    }
}

#[cfg(test)]
mod tests {
    use super::{Name, add_member, infer_runtime, set_field};
    use crate::manifest::Runtime;
    use camino::Utf8PathBuf;

    #[test]
    fn a_fresh_skeleton_carries_a_test_so_its_own_gate_passes() {
        let body = super::cargo_main("widget");
        assert!(body.contains("#[cfg(test)]"), "{body}");
        assert!(body.contains("#[test]"), "{body}");
        assert!(
            body.contains("assert_eq!"),
            "an assertion-free test kills no mutants"
        );
        assert!(body.contains("fn main()"), "{body}");
    }

    #[test]
    fn a_name_is_a_filename_on_path_and_nothing_cleverer() {
        for good in ["thing", "my-thing", "my_thing", "x1"] {
            assert!(good.parse::<Name>().is_ok(), "{good}");
        }
        for bad in ["", "../evil", "a/b", ".hidden", "-flag", "a b", "a.b"] {
            assert!(bad.parse::<Name>().is_err(), "{bad:?}");
        }
    }

    fn script(body: &str) -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let path = Utf8PathBuf::from_path_buf(dir.path().join("s")).expect("utf8 scratch path");
        fs_err::write(path.as_std_path(), body).expect("a script");
        (dir, path)
    }

    #[test]
    fn a_shebang_names_the_runtime() {
        for (body, want) in [
            ("#!/usr/bin/env python3\n", Some(Runtime::Python)),
            ("#!/usr/bin/env bash\n", Some(Runtime::Bash)),
            ("#!/bin/bash -e\n", Some(Runtime::Bash)),
            ("#!/usr/bin/env fish\n", Some(Runtime::Fish)),
            ("#!/bin/sh\n", Some(Runtime::Bash)),
            ("#!/usr/bin/env sh\n", Some(Runtime::Bash)),
            ("#!/usr/bin/env perl\n", None),
            ("no shebang\n", None),
            ("", None),
        ] {
            let (_guard, path) = script(body);
            assert_eq!(infer_runtime(&path), want, "{body:?}");
        }
    }

    #[test]
    fn a_script_that_is_not_there_infers_nothing() {
        assert!(infer_runtime(camino::Utf8Path::new("/nowhere")).is_none());
    }

    #[test]
    fn setting_a_field_keeps_the_comment_it_had() {
        let body = "name = \"\"           # the published name\nruntime = \"\"\n";
        let out = set_field(body, "name", "widget");
        assert!(out.contains("name = \"widget\""), "{out}");
        assert!(out.contains("# the published name"), "{out}");
        assert!(
            out.contains("runtime = \"\""),
            "the other line moved: {out}"
        );
    }

    #[test]
    fn a_value_is_escaped_for_a_basic_string() {
        let out = set_field("why = \"\"\n", "why", "he said \"no\" \\ then left");
        assert_eq!(out, "why = \"he said \\\"no\\\" \\\\ then left\"\n");
    }

    #[test]
    fn a_key_that_is_not_there_changes_nothing() {
        let body = "name = \"x\"\n";
        assert_eq!(set_field(body, "absent", "y"), body);
    }

    #[test]
    fn a_member_lands_in_sorted_position_and_only_once() {
        let body = "members = [\n    \"alpha\",\n    \"gamma\",\n]\n";
        let once = add_member(body, "beta");
        assert_eq!(
            once,
            "members = [\n    \"alpha\",\n    \"beta\",\n    \"gamma\",\n]\n"
        );
        assert_eq!(add_member(&once, "beta"), once, "a second insert added one");
    }

    #[test]
    fn a_member_sorting_past_the_end_lands_before_the_bracket() {
        let body = "members = [\n    \"alpha\",\n]\n";
        assert_eq!(
            add_member(body, "zeta"),
            "members = [\n    \"alpha\",\n    \"zeta\",\n]\n"
        );
    }

    #[test]
    fn an_empty_member_list_takes_the_first_one() {
        assert_eq!(
            add_member("members = [\n]\n", "alpha"),
            "members = [\n    \"alpha\",\n]\n"
        );
    }
}
