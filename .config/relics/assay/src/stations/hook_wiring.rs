//! Whether every configured hook can actually run.
//!
//! A hook is the one kind of configuration that is invisible when it is wrong.
//! Nothing invokes it directly, nothing reports its absence, and both surfaces
//! here swallow their own failures on purpose: Claude Code's session-start hooks
//! end in `|| true` so a broken one cannot block a session, and yadm's
//! `pre_commit` is a file git simply does not run when its execute bit is gone.
//! In both cases the machine behaves exactly as if the hook had been deleted.
//!
//! **Two surfaces, one question: does the thing named exist and can it be run?**
//!
//! - **Claude Code** — every `command` hook in `~/.claude/settings.json` and its
//!   `.local` sibling. The program is resolved the way a shell would: the first
//!   word, `$HOME` expanded, looked up on the search path when it is bare. When
//!   that word is an interpreter, the script it is handed is checked too, since
//!   `python3 /gone/hook.py` resolves perfectly and does nothing.
//! - **yadm** — every file in `~/.config/yadm/hooks/`. git runs a hook only if
//!   it is executable, and a `pre_commit` without the bit is a commit guard that
//!   silently is not there.
//!
//! **The event-name table is deliberately not transcribed.** Checking that
//! `SessionStart` is a real event would mean carrying Claude Code's list of
//! them, and a stale copy fails every hook on the next harness release —
//! reporting a broken machine because the checker is old. The design's rule for
//! a table read out of a third-party binary is that it must fail loudly when
//! that binary moves, which needs the runner-level staleness facility the
//! permission-rule station brings. Until then this station checks only what it
//! can know from the filesystem, which never goes stale.

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;

use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};

use crate::station::{Context, Station};

/// Harness settings files, `$HOME`-relative, in the order they are reported.
const SETTINGS: &[&str] = &[".claude/settings.json", ".claude/settings.local.json"];

/// Where yadm looks for its hooks, `$HOME`-relative.
const YADM_HOOKS: &str = ".config/yadm/hooks";

/// Files in the hook directory that are data rather than hooks.
///
/// yadm runs a hook whose name matches one of its commands. The identity guard's
/// definition and its reader live beside them and are read by `warden`, not run
/// by yadm — so an execute bit on either would be wrong, not missing.
const NOT_HOOKS: &[&str] = &["identity-guard.toml", "identity-guard.py"];

/// Programs that run a script named by their next argument.
///
/// `python3 /gone/hook.py` resolves and does nothing, so the argument is the
/// half worth checking.
const INTERPRETERS: &[&str] = &[
    "python3", "python", "bash", "sh", "zsh", "node", "ruby", "perl",
];

/// What a settings file says about hooks.
///
/// Not `deny_unknown_fields`: this is Claude Code's schema, and it grows. Only
/// the parts that name something runnable are read.
#[derive(Debug, Deserialize)]
struct Settings {
    /// Event name to matchers.
    #[serde(default)]
    hooks: std::collections::BTreeMap<String, Vec<Matcher>>,
}

/// One matcher's list of hooks.
#[derive(Debug, Deserialize)]
struct Matcher {
    /// The hooks it fires.
    #[serde(default)]
    hooks: Vec<Hook>,
}

/// One hook.
#[derive(Debug, Deserialize)]
struct Hook {
    /// What kind it is. Only `command` names a program.
    #[serde(rename = "type", default)]
    kind: String,
    /// The shell command, when it is a command hook.
    #[serde(default)]
    command: Option<String>,
}

/// The station.
pub struct HookWiring {
    id: StationId,
}

impl Default for HookWiring {
    fn default() -> Self {
        Self {
            id: StationId::from_static("hook-wiring"),
        }
    }
}

impl Station for HookWiring {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "every configured hook names something that exists and can be run"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let mut findings = Vec::new();
        let mut looked = false;

        for settings in SETTINGS {
            let at = cx.at(settings);
            if !at.is_file() {
                continue;
            }
            looked = true;
            findings.extend(self.harness(cx, &at));
        }

        let hooks = cx.at(YADM_HOOKS);
        if hooks.is_dir() {
            looked = true;
            findings.extend(self.yadm(&hooks));
        }

        if looked {
            Ok(Outcome::Ran(findings))
        } else {
            Ok(Outcome::Skipped(Summary::lossy(
                "this machine configures no hooks",
            )))
        }
    }
}

impl HookWiring {
    /// Every command hook in one settings file.
    fn harness(&self, cx: &Context, at: &Utf8Path) -> Vec<Finding> {
        let Ok(text) = fs_err::read_to_string(at) else {
            return vec![
                self.id
                    .broken(Summary::lossy("a settings file could not be read"))
                    .at(Location::file(at.to_owned())),
            ];
        };
        let settings: Settings = match serde_json::from_str(&text) {
            Ok(settings) => settings,
            Err(error) => {
                return vec![
                    self.id
                        .broken(Summary::lossy(&format!(
                            "a settings file is not valid JSON, so none of its hooks are \
                             configured: {error}"
                        )))
                        .at(Location::file(at.to_owned())),
                ];
            }
        };

        let mut findings = Vec::new();
        for (event, matchers) in &settings.hooks {
            for hook in matchers.iter().flat_map(|matcher| &matcher.hooks) {
                if hook.kind != "command" {
                    findings.push(
                        self.id
                            .broken(Summary::lossy(&format!(
                                "a {event} hook has type {:?}, which names no program",
                                hook.kind
                            )))
                            .at(Location::file(at.to_owned())),
                    );
                    continue;
                }
                let Some(command) = hook.command.as_deref() else {
                    findings.push(
                        self.id
                            .broken(Summary::lossy(&format!(
                                "a {event} command hook has no command"
                            )))
                            .at(Location::file(at.to_owned())),
                    );
                    continue;
                };
                findings.extend(self.runnable(cx, at, event, command));
            }
        }
        findings
    }

    /// Whether one hook command names something that can run.
    fn runnable(&self, cx: &Context, at: &Utf8Path, event: &str, command: &str) -> Vec<Finding> {
        let words = words(command);
        let Some(program) = words.first() else {
            return vec![
                self.id
                    .broken(Summary::lossy(&format!(
                        "a {event} hook's command is empty"
                    )))
                    .at(Location::file(at.to_owned())),
            ];
        };

        let mut findings = Vec::new();
        if resolve(cx, program).is_none() {
            findings.push(
                self.id
                    .broken(Summary::lossy(&format!(
                        "a {event} hook runs {program}, which is not on this machine"
                    )))
                    .at(Location::file(at.to_owned()))
                    .detailed_with(Detail::new(
                        "A hook that cannot start is indistinguishable from one that was never \
                         configured — and a command ending in `|| true` cannot even fail.",
                    ))
                    .fixed_by(FixHint::lossy(
                        "install it, or drop the hook from the settings file",
                    )),
            );
            return findings;
        }

        // An interpreter resolves whatever it is handed, so the script is the
        // half that actually decides whether anything runs.
        let is_interpreter = Utf8Path::new(program)
            .file_name()
            .is_some_and(|name| INTERPRETERS.contains(&name));
        if !is_interpreter {
            return findings;
        }
        let Some(script) = words.get(1).filter(|word| !word.starts_with('-')) else {
            return findings;
        };
        let at_script = expand(cx, script);
        if !at_script.is_file() {
            findings.push(
                self.id
                    .broken(Summary::lossy(&format!(
                        "a {event} hook runs a script that is not on this machine"
                    )))
                    .at(Location::file(at_script))
                    .detailed_with(Detail::new(
                        "The interpreter resolves, so the hook starts and does nothing. That is \
                         the same outcome as no hook at all, reported by nobody.",
                    )),
            );
        }
        findings
    }

    /// Every yadm hook file, and whether git would run it.
    fn yadm(&self, dir: &Utf8Path) -> Vec<Finding> {
        let Ok(entries) = dir.read_dir_utf8() else {
            return vec![
                self.id
                    .broken(Summary::lossy("the yadm hook directory could not be read"))
                    .at(Location::file(dir.to_owned())),
            ];
        };
        let mut hooks: Vec<Utf8PathBuf> = entries
            .flatten()
            .map(|entry| entry.path().to_owned())
            .filter(|path| {
                path.is_file()
                    && !path
                        .file_name()
                        .is_some_and(|name| name.starts_with('.') || NOT_HOOKS.contains(&name))
            })
            .collect();
        hooks.sort();

        hooks
            .into_iter()
            .filter(|hook| !executable(hook))
            .map(|hook| {
                let name = hook.file_name().unwrap_or("?").to_owned();
                self.id
                    .broken(Summary::lossy(&format!(
                        "the yadm {name} hook is not executable, so it does not run"
                    )))
                    .at(Location::file(hook))
                    .detailed_with(Detail::new(
                        "git skips a hook without the bit and says nothing. A guard that does \
                         not run is a guard that is not there, and nothing reports it.",
                    ))
                    .fixed_by(FixHint::lossy(&format!("chmod +x ~/{YADM_HOOKS}/{name}")))
            })
            .collect()
    }
}

/// A command split the way a shell would split its first few words.
///
/// Not a shell parser, and it does not need to be: what is wanted is the program
/// and, at most, its first argument. Double and single quotes are honoured
/// because a `$HOME`-bearing path is always quoted, and everything from the
/// first operator onwards is dropped — `|| true` is not an argument.
fn words(command: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut quote: Option<char> = None;
    for c in command.chars() {
        if let Some(open) = quote {
            if c == open {
                quote = None;
            } else {
                current.push(c);
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '|' | '&' | ';' | '>' | '<' => {
                if !current.is_empty() {
                    words.push(current);
                }
                return words;
            }
            c if c.is_whitespace() => {
                if !current.is_empty() {
                    words.push(std::mem::take(&mut current));
                }
            }
            c => current.push(c),
        }
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

/// A word with `$HOME` and `~` resolved against the home being checked.
fn expand(cx: &Context, word: &str) -> Utf8PathBuf {
    let expanded = word
        .replace("${HOME}", cx.home().as_str())
        .replace("$HOME", cx.home().as_str());
    match expanded.strip_prefix("~/") {
        Some(rest) => cx.at(rest),
        None => Utf8PathBuf::from(expanded),
    }
}

/// Where a program word resolves, or nothing.
fn resolve(cx: &Context, program: &str) -> Option<Utf8PathBuf> {
    let expanded = expand(cx, program);
    if program.contains('/') || program.starts_with('$') || program.starts_with('~') {
        return (expanded.is_file() && executable(&expanded)).then_some(expanded);
    }
    cx.path()
        .iter()
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file() && executable(candidate))
}

/// Whether the owner may run it.
#[cfg(unix)]
fn executable(path: &Utf8Path) -> bool {
    use std::os::unix::fs::PermissionsExt as _;

    fs_err::metadata(path).is_ok_and(|meta| meta.permissions().mode() & 0o111 != 0)
}

/// Nothing to say where the permission bits do not exist.
#[cfg(not(unix))]
fn executable(_path: &Utf8Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use relic_core::finding::Severity;

    use super::*;

    /// A machine with a settings file, a hook directory, and a search path.
    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
        bin: Utf8PathBuf,
    }

    impl Machine {
        /// Everything wired: one interpreted hook and one direct one, plus an
        /// executable yadm `pre_commit`.
        fn sound() -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let home =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            let bin = home.join("bin");
            fs_err::create_dir_all(&bin).expect("a bin dir");
            let machine = Self {
                _dir: dir,
                home,
                bin,
            };
            Self::write(&machine.bin.join("python3"), "", 0o755);
            Self::write(&machine.home.join(".claude/hooks/modes.py"), "", 0o644);
            Self::write(&machine.home.join(".local/bin/docket"), "", 0o755);
            machine.yadm_hook("pre_commit", 0o700);
            machine.settings(
                r#"{"hooks":{
                     "UserPromptSubmit":[{"hooks":[
                       {"type":"command","command":"python3 \"$HOME/.claude/hooks/modes.py\""}]}],
                     "SessionStart":[{"hooks":[
                       {"type":"command","command":"\"$HOME/.local/bin/docket\" announce --hook || true"}]}]
                   }}"#,
            );
            machine
        }

        fn write(at: &Utf8Path, body: &str, mode: u32) {
            if let Some(parent) = at.parent() {
                fs_err::create_dir_all(parent).expect("a parent");
            }
            fs_err::write(at, body).expect("written");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs_err::set_permissions(at, std::fs::Permissions::from_mode(mode)).expect("a mode");
            }
        }

        fn settings(&self, json: &str) -> &Self {
            Self::write(&self.home.join(SETTINGS[0]), json, 0o644);
            self
        }

        fn yadm_hook(&self, name: &str, mode: u32) -> &Self {
            Self::write(&self.home.join(YADM_HOOKS).join(name), "#!/bin/sh\n", mode);
            self
        }

        fn outcome(&self) -> Outcome {
            HookWiring::default()
                .check(&Context::new(self.home.clone(), vec![self.bin.clone()]))
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
    fn hooks_that_can_all_run_have_nothing_to_say() {
        assert!(Machine::sound().findings().is_empty());
    }

    #[test]
    fn a_hook_whose_program_is_gone_is_broken_even_though_it_cannot_fail() {
        let machine = Machine::sound();
        fs_err::remove_file(machine.home.join(".local/bin/docket")).expect("removed");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("not on this machine"),
            "{finding:?}"
        );
        assert!(
            finding
                .detail
                .as_ref()
                .is_some_and(|d| d.as_str().contains("|| true")),
            "the reason it is invisible is the point of the finding"
        );
    }

    #[test]
    fn an_interpreter_that_resolves_does_not_make_its_script_exist() {
        let machine = Machine::sound();
        fs_err::remove_file(machine.home.join(".claude/hooks/modes.py")).expect("removed");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("script that is not"),
            "python3 resolves perfectly and runs nothing: {finding:?}"
        );
    }

    #[test]
    fn a_program_that_is_present_and_not_executable_does_not_resolve() {
        let machine = Machine::sound();
        Machine::write(&machine.home.join(".local/bin/docket"), "", 0o644);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
    }

    #[test]
    fn a_yadm_hook_without_its_execute_bit_is_a_guard_that_is_not_there() {
        let machine = Machine::sound();
        machine.yadm_hook("pre_commit", 0o600);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding
                .summary
                .as_str()
                .contains("pre_commit hook is not executable"),
            "{finding:?}"
        );
    }

    #[test]
    fn the_guards_data_and_its_reader_are_not_hooks_and_want_no_execute_bit() {
        let machine = Machine::sound();
        for name in NOT_HOOKS {
            Machine::write(&machine.home.join(YADM_HOOKS).join(name), "", 0o600);
        }
        assert!(
            machine.findings().is_empty(),
            "yadm runs a hook named after one of its commands, not everything in the directory"
        );
    }

    #[test]
    fn settings_that_are_not_json_lose_every_hook_and_say_so() {
        let machine = Machine::sound();
        machine.settings("{ not json");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("not valid JSON"));
    }

    #[test]
    fn a_hook_of_a_kind_that_names_no_program_is_reported_rather_than_skipped() {
        let machine = Machine::sound();
        machine.settings(r#"{"hooks":{"SessionStart":[{"hooks":[{"type":"magic"}]}]}}"#);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("names no program"));
    }

    #[test]
    fn a_machine_with_no_hooks_at_all_is_skipped_rather_than_graded_clean() {
        let dir = tempfile::tempdir().expect("a scratch dir");
        let home = Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("utf-8");
        let Outcome::Skipped(reason) = HookWiring::default()
            .check(&Context::new(home, Vec::new()))
            .expect("the station ran")
        else {
            panic!("nothing configured is a fact, not a pass");
        };
        assert!(reason.as_str().contains("no hooks"), "{reason}");
    }

    #[test]
    fn an_operator_ends_the_command_and_is_never_read_as_an_argument() {
        assert_eq!(
            words("\"$HOME/.local/bin/docket\" announce --hook || true"),
            vec!["$HOME/.local/bin/docket", "announce", "--hook"]
        );
    }

    #[test]
    fn a_quoted_path_is_one_word_however_many_spaces_it_holds() {
        assert_eq!(
            words("python3 \"$HOME/a b/hook.py\""),
            vec!["python3", "$HOME/a b/hook.py"]
        );
    }

    #[test]
    fn a_flag_is_not_the_script_an_interpreter_was_handed() {
        let machine = Machine::sound();
        machine.settings(
            r#"{"hooks":{"X":[{"hooks":[{"type":"command","command":"python3 -c 'pass'"}]}]}}"#,
        );
        assert!(
            machine.findings().is_empty(),
            "`python3 -c` names no script, so there is no script to miss"
        );
    }
}
