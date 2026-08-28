//! Whether every plugin under `~/.claude/skills/` actually loads.
//!
//! That directory is not a skills folder — it is a **plugin auto-load root**.
//! Claude Code adopts every non-dot entry under it as a local plugin named after
//! the directory, with no install step and no `enabledPlugins` entry. The
//! familiar one-`SKILL.md` directory is just the degenerate single-skill plugin.
//!
//! Adoption is what makes this worth a station: there is **no install step to
//! fail**. A plugin that carries nothing, a manifest that is not JSON, a symlink
//! whose target is gone — none of them produces an error anyone sees. The
//! surface is simply absent, and its absence looks exactly like never having
//! written it.
//!
//! **A skill's `description` is the load-bearing field.** It is the only channel
//! that reaches an agent without a tool call: a skill whose description is
//! missing is a skill no agent will ever reach for, however good its body is.
//! That is why it grades the same as a skill that does not parse.
//!
//! **The lane rule is not checked here.** Top level is public and
//! plaintext-tracked, `attic/` is private and swept by one encrypt pattern; a
//! file in the wrong lane is `yadm-coverage`'s R1 and R4, which already run that
//! test in both directions. One fact, one owner.
//!
//! **The component table is used positively and never negatively.** Claude Code's
//! list of what a plugin root may carry is a third-party table, so an unknown
//! directory is never a finding — it is a plugin kind this station has not heard
//! of. What the table is used for is deciding that a directory carries
//! *something*, and when it appears to carry nothing that is a `Note` rather
//! than a verdict, because "I did not recognise anything here" and "there is
//! nothing here" are different claims.

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};

use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};

use crate::station::{Context, Station};

/// The plugin auto-load root, `$HOME`-relative.
const ROOT: &str = ".claude/skills";

/// A plugin's manifest, relative to its own directory.
const MANIFEST: &str = ".claude-plugin/plugin.json";

/// A skill's body, relative to a skill directory.
const SKILL: &str = "SKILL.md";

/// What a plugin root may carry, as Claude Code documents it.
///
/// Read only to decide that a directory holds *something*. An entry not on this
/// list is never a finding — see the module note.
const COMPONENTS: &[&str] = &[
    "commands",
    "agents",
    "skills",
    "output-styles",
    "workflows",
    "routines",
    "hooks",
];

/// Files at a plugin root that are content in their own right.
const ROOT_FILES: &[&str] = &[SKILL, ".mcp.json", ".lsp.json"];

/// Every file a plugin may carry that must be valid JSON, relative to its root.
const JSON_FILES: &[&str] = &[MANIFEST, ".mcp.json", ".lsp.json"];

/// The station.
pub struct ClaudePlugins {
    id: StationId,
}

impl Default for ClaudePlugins {
    fn default() -> Self {
        Self {
            id: StationId::from_static("claude-plugins"),
        }
    }
}

impl Station for ClaudePlugins {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "every auto-loaded plugin carries something, and everything it carries parses"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let root = cx.at(ROOT);
        if !root.is_dir() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "this machine has no plugin auto-load root",
            )));
        }
        let Ok(entries) = root.read_dir_utf8() else {
            return Ok(Outcome::Ran(vec![
                self.id
                    .broken(Summary::lossy(
                        "the plugin auto-load root could not be read",
                    ))
                    .at(Location::file(root)),
            ]));
        };

        let mut plugins: Vec<Utf8PathBuf> = entries
            .flatten()
            .map(|entry| entry.path().to_owned())
            .filter(|path| !path.file_name().is_some_and(|name| name.starts_with('.')))
            .collect();
        plugins.sort();

        let mut findings = Vec::new();
        for plugin in &plugins {
            findings.extend(self.plugin(plugin));
        }
        Ok(Outcome::Ran(findings))
    }
}

impl ClaudePlugins {
    /// One adopted entry under the root.
    fn plugin(&self, at: &Utf8Path) -> Vec<Finding> {
        let name = at.file_name().unwrap_or("?").to_owned();

        // `is_dir` follows the link, so a dangling symlink fails it — which is
        // the case worth separating, because the entry is *there* and adopts
        // nothing.
        if !at.is_dir() {
            let dangling = at.is_symlink();
            return vec![
                self.id
                    .broken(Summary::lossy(&if dangling {
                        format!("the {name} plugin is a symlink whose target is gone")
                    } else {
                        format!("{name} is not a directory, so it adopts as no plugin")
                    }))
                    .at(Location::file(at.to_owned()))
                    .detailed_with(Detail::new(
                        "Adoption has no install step, so nothing reports this. The surface is \
                         simply absent.",
                    )),
            ];
        }

        let mut findings = Vec::new();
        findings.extend(self.parses(at, &name));
        findings.extend(self.skills(at, &name));
        findings.extend(self.carries(at, &name));
        findings
    }

    /// Every JSON file the plugin carries is JSON.
    fn parses(&self, at: &Utf8Path, name: &str) -> Vec<Finding> {
        JSON_FILES
            .iter()
            .map(|relative| (relative, at.join(relative)))
            .filter(|(_, file)| file.is_file())
            .filter_map(|(relative, file)| {
                let refused = match fs_err::read_to_string(&file) {
                    Err(error) => format!("could not be read: {error}"),
                    Ok(text) => serde_json::from_str::<serde_json::Value>(&text)
                        .err()
                        .map(|error| format!("is not valid JSON: {error}"))?,
                };
                Some(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "the {name} plugin's {relative} {refused}"
                        )))
                        .at(Location::file(file)),
                )
            })
            .collect()
    }

    /// Every skill the plugin carries announces itself.
    ///
    /// Both shapes: the degenerate single-skill plugin with a `SKILL.md` at its
    /// root, and a multi-skill plugin's `skills/<name>/SKILL.md`.
    fn skills(&self, at: &Utf8Path, name: &str) -> Vec<Finding> {
        let mut skills: Vec<Utf8PathBuf> = Vec::new();
        if at.join(SKILL).is_file() {
            skills.push(at.join(SKILL));
        }
        if let Ok(entries) = at.join("skills").read_dir_utf8() {
            let mut nested: Vec<Utf8PathBuf> = entries
                .flatten()
                .map(|entry| entry.path().join(SKILL))
                .filter(|file| file.is_file())
                .collect();
            nested.sort();
            skills.append(&mut nested);
        }

        skills
            .into_iter()
            .filter_map(|file| {
                let text = fs_err::read_to_string(&file).ok()?;
                let missing = missing_fields(&text);
                if missing.is_empty() {
                    return None;
                }
                Some(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "a skill in the {name} plugin declares no {}",
                            missing.join(" and no ")
                        )))
                        .at(Location::file(file))
                        .detailed_with(Detail::new(
                            "The description is the only channel that reaches an agent without a \
                             tool call. Without it the skill is never reached for, however good \
                             its body is.",
                        ))
                        .fixed_by(FixHint::lossy(
                            "add the field to the YAML frontmatter at the top of the file",
                        )),
                )
            })
            .collect()
    }

    /// The plugin carries something this station recognises.
    fn carries(&self, at: &Utf8Path, name: &str) -> Vec<Finding> {
        let has_component = COMPONENTS
            .iter()
            .any(|component| at.join(component).is_dir());
        let has_file = ROOT_FILES.iter().any(|file| at.join(file).is_file());
        if has_component || has_file {
            return Vec::new();
        }
        vec![
            self.id
                .note(Summary::lossy(&format!(
                    "the {name} plugin carries nothing this station recognises"
                )))
                .at(Location::file(at.to_owned()))
                .detailed_with(Detail::new(
                    "A note rather than a verdict: the list of what a plugin may carry is Claude \
                     Code's, so this may be a plugin kind that did not exist when the station was \
                     written. \"I recognised nothing here\" and \"there is nothing here\" are \
                     different claims.",
                )),
        ]
    }
}

/// Which of the two required frontmatter fields a skill is missing.
///
/// The frontmatter is the first `---`-delimited block, and only a top-level
/// `key:` counts — a `description:` nested inside a later block is not the
/// skill's own. Returned in declaration order so the message is stable.
fn missing_fields(text: &str) -> Vec<&'static str> {
    let Some(front) = frontmatter(text) else {
        return vec!["frontmatter"];
    };
    ["name", "description"]
        .into_iter()
        .filter(|field| {
            !front.lines().any(|line| {
                line.strip_prefix(field)
                    .and_then(|rest| rest.strip_prefix(':'))
                    .is_some_and(|rest| !rest.trim().is_empty())
            })
        })
        .collect()
}

/// The first `---`-delimited block, when the file opens with one.
fn frontmatter(text: &str) -> Option<&str> {
    let rest = text.strip_prefix("---\n")?;
    let end = rest.find("\n---")?;
    rest.get(..end)
}

#[cfg(test)]
mod tests {
    use relic_core::finding::Severity;

    use super::*;

    /// A machine with a plugin root a test composes.
    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
    }

    impl Machine {
        /// One single-skill plugin and one multi-skill plugin, both sound.
        fn sound() -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let home =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            let machine = Self { _dir: dir, home };
            machine.skill("one", SKILL, "one");
            machine.write(
                "many/.claude-plugin/plugin.json",
                r#"{"name":"many","version":"1.0.0"}"#,
            );
            machine.skill("many", "skills/first/SKILL.md", "first");
            machine.write("many/commands/modes/a.md", "# a\n");
            machine
        }

        fn write(&self, relative: &str, body: &str) -> Utf8PathBuf {
            let at = self.home.join(ROOT).join(relative);
            fs_err::create_dir_all(at.parent().expect("a parent")).expect("a parent");
            fs_err::write(&at, body).expect("written");
            at
        }

        fn skill(&self, plugin: &str, relative: &str, name: &str) -> Utf8PathBuf {
            self.write(
                &format!("{plugin}/{relative}"),
                &format!("---\nname: {name}\ndescription: what it is for.\n---\n\n# {name}\n"),
            )
        }

        fn outcome(&self) -> Outcome {
            ClaudePlugins::default()
                .check(&Context::new(self.home.clone(), Vec::new()))
                .expect("the station ran")
        }

        fn findings(&self) -> Vec<Finding> {
            match self.outcome() {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("unexpectedly skipped: {reason}"),
            }
        }

        fn only(&self) -> Finding {
            let findings = self.findings();
            assert_eq!(findings.len(), 1, "{findings:?}");
            findings.into_iter().next().expect("one")
        }
    }

    #[test]
    fn plugins_that_all_load_have_nothing_to_say() {
        assert!(Machine::sound().findings().is_empty());
    }

    #[test]
    fn a_skill_with_no_description_is_a_skill_no_agent_will_reach_for() {
        let machine = Machine::sound();
        machine.write("one/SKILL.md", "---\nname: one\n---\n\n# one\n");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("no description"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_skill_with_no_frontmatter_at_all_says_that_rather_than_naming_fields() {
        let machine = Machine::sound();
        machine.write("one/SKILL.md", "# one\n");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("no frontmatter"),
            "{finding:?}"
        );
    }

    #[test]
    fn an_empty_field_is_a_missing_field() {
        let machine = Machine::sound();
        machine.write("one/SKILL.md", "---\nname: one\ndescription:   \n---\n");
        assert_eq!(machine.only().severity, Severity::Broken);
    }

    #[test]
    fn a_nested_skill_is_checked_the_same_as_a_root_one() {
        let machine = Machine::sound();
        machine.write("many/skills/first/SKILL.md", "---\nname: first\n---\n");
        let finding = machine.only();
        assert!(
            finding.summary.as_str().contains("many plugin"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_manifest_that_is_not_json_loses_the_plugin_and_reports_it() {
        let machine = Machine::sound();
        machine.write("many/.claude-plugin/plugin.json", "{ not json");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("not valid JSON"));
    }

    #[test]
    fn every_json_file_a_plugin_may_carry_is_checked_not_only_the_manifest() {
        let machine = Machine::sound();
        machine.write("one/.mcp.json", "{ nope");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains(".mcp.json"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_file_at_the_root_adopts_as_no_plugin() {
        let machine = Machine::sound();
        machine.write("stray.md", "# not a plugin\n");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("not a directory"));
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_whose_target_is_gone_is_named_as_one() {
        let machine = Machine::sound();
        let link = machine.home.join(ROOT).join("elsewhere");
        std::os::unix::fs::symlink(machine.home.join("nowhere"), &link).expect("a symlink");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("target is gone"),
            "an entry that is there and adopts nothing is the case worth separating: {finding:?}"
        );
    }

    #[test]
    fn a_dot_entry_is_not_adopted_and_is_not_judged() {
        let machine = Machine::sound();
        machine.write(".DS_Store", "\0");
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn a_plugin_carrying_nothing_recognised_is_a_note_and_never_a_verdict() {
        let machine = Machine::sound();
        fs_err::create_dir_all(machine.home.join(ROOT).join("hollow")).expect("a dir");
        let finding = machine.only();
        assert_eq!(
            finding.severity,
            Severity::Note,
            "the component list is Claude Code's, so an unknown kind must not grade"
        );
        assert!(finding.summary.as_str().contains("hollow"), "{finding:?}");
    }

    #[test]
    fn a_plugin_whose_only_content_is_a_component_directory_carries_something() {
        let machine = Machine::sound();
        fs_err::create_dir_all(machine.home.join(ROOT).join("cmds/commands")).expect("a dir");
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn no_plugin_root_is_skipped_rather_than_graded_clean() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let home = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
        let Outcome::Skipped(reason) = ClaudePlugins::default()
            .check(&Context::new(home, Vec::new()))
            .expect("the station ran")
        else {
            panic!("a machine with no plugin root has nothing to verify");
        };
        assert!(reason.as_str().contains("no plugin"), "{reason}");
    }

    #[test]
    fn a_field_of_a_later_block_is_not_the_skills_own() {
        assert_eq!(
            missing_fields("---\nname: a\n---\n\n```\n---\ndescription: not mine\n---\n```\n"),
            vec!["description"]
        );
    }

    #[test]
    fn a_key_that_merely_starts_the_same_is_a_different_key() {
        assert_eq!(
            missing_fields("---\nname: a\ndescription_long: x\n---\n"),
            vec!["description"]
        );
    }
}
