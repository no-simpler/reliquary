//! Whether the harness will honour the permission rules this machine carries.
//!
//! A rule the harness cannot use does not fail loudly. It is accepted into a
//! settings file, mentioned once in a warning above the first prompt, and then
//! matches nothing for the rest of the session. What is left reads as protection
//! that is not there — and the warning is addressed to a person, at session
//! start, in whichever project happened to load the file, so a rule authored in
//! one project surfaces days later in an unrelated one and **no agent ever sees
//! it at all**.
//!
//! **File permission checks consult exactly one gate per operation**: `Edit(path)`
//! for every file-writing tool, `Read(path)` for every file-reading tool. So
//! `Write(~/somewhere/**)` looks like write access and grants none. The harness
//! warns about `Write`, `MultiEdit`, `NotebookEdit` and `Glob`; **`Grep(path)` it
//! does not warn about at all**, which makes that one strictly worse — it looks
//! live.
//!
//! ## Provenance
//!
//! The tables and the check ordering are transcribed from the Claude Code bundle
//! — the settings validator itself, not documentation about it — and verified
//! against **2.1.251**. [`Station::derived_from`] carries that version and the
//! recipe, so the runner says so when the installed harness has moved on.
//!
//! The port took its shape from `halo/alfred/scripts/harness/settings_lint.py`,
//! whose own transcription was read against 2.1.226. Re-deriving against 2.1.251
//! confirmed every table unchanged and found **two things that version predates**,
//! both carried here: the warned-gate check is suppressed when the pattern holds
//! `:*`, and an allow rule for `Bash` whose wildcard sits *before* the rest of
//! the command is warned about by the harness itself.
//!
//! ## Scope
//!
//! Machine-wide, which is the whole point: the class this closes is a rule
//! written in one tree and never seen again. `halo`'s lint is a commit gate over
//! one repository; this is the standing audit over every settings file the
//! machine carries.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};

use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};

use crate::station::{Context, Derivation, Station};

/// Where settings files are looked for, `$HOME`-relative.
///
/// Three roots, because that is where this machine keeps trees. `~/.claude` is
/// the user level; the other two carry per-project `.claude/` directories.
const ROOTS: &[&str] = &[".claude", ".config", "Developer"];

/// How deep a `.claude` directory may sit under a root.
///
/// `~/Developer/<org>/<repo>/.claude` is the deepest real shape. A cap keeps the
/// walk bounded on a tree holding vendored dependencies.
const MAX_DEPTH: usize = 5;

/// Directories a settings file is never inside, and that are expensive to enter.
const SKIP: &[&str] = &["node_modules", "target", "vendor", ".git", "dist", "build"];

/// Where the harness keeps the versions it has installed, `$HOME`-relative.
const VERSIONS: &str = ".local/share/claude/versions";

/// The harness release the tables below were read against.
const DERIVED_AGAINST: &str = "2.1.251";

/// How to read them again.
const RECIPE: &str = "\
v=~/.local/share/claude/versions/<version>
LC_ALL=C grep -a -b -o -F 'is not matched by file permission checks' \"$v\"
dd if=\"$v\" bs=1 skip=$((<offset> - 6500)) count=8000 | tr -c '[:print:]\\n' '\\n'

The validator is one function ending in that warning; the tool tables it reads
sit a few hundred bytes ahead of it in the same window.";

/// Renamed tools, mapped before anything else looks at the name.
const ALIASES: &[(&str, &str)] = &[
    ("Task", "Agent"),
    ("KillShell", "TaskStop"),
    ("KillBash", "TaskStop"),
    ("AgentOutputTool", "TaskOutput"),
    ("BashOutputTool", "TaskOutput"),
    ("AgentOutput", "TaskOutput"),
    ("BashOutput", "TaskOutput"),
    ("ListPeers", "ListAgents"),
    ("Brief", "SendUserMessage"),
    ("ListMcpResources", "ListMcpResourcesTool"),
    ("ReadMcpResource", "ReadMcpResourceTool"),
    ("ReadMcpResourceDir", "ReadMcpResourceDirTool"),
];

/// Tools whose rule content is a file glob, so `:*` in it is an error.
const FILE_PATTERN_TOOLS: &[&str] = &[
    "Read",
    "Write",
    "Edit",
    "Glob",
    "NotebookRead",
    "NotebookEdit",
    "Cd",
];

/// Tools whose rule content is a command prefix.
const BASH_PREFIX_TOOLS: &[&str] = &["Bash"];

/// A path rule on the key is never consulted; the value is the rule that is.
///
/// The harness warns about each of these once per session. One `Edit` rule
/// covers every file-editing tool and one `Read` rule every file-reading tool,
/// so the remedy is always a rewrite and never an addition.
const WARNED_GATE: &[(&str, &str)] = &[
    ("Write", "Edit"),
    ("NotebookEdit", "Edit"),
    ("MultiEdit", "Edit"),
    ("Glob", "Read"),
];

/// The same failure with no warning attached.
///
/// `Grep` is absent from the harness's file-tool tables entirely, so nothing
/// checks a `Grep(path)` rule and nothing ever matches it. Strictly worse than a
/// warned one: it looks live.
const SILENT_GATE: &[(&str, &str)] = &[("Grep", "Read")];

/// The lists a settings file may carry, in reporting order.
const RULE_LISTS: &[&str] = &["allow", "deny", "ask"];

/// A rule as the harness reads it.
#[derive(Debug, PartialEq, Eq)]
struct Rule {
    /// The tool, canonicalised through [`ALIASES`].
    tool: String,
    /// The pattern, when the rule carries one. An empty pattern and a bare `*`
    /// both mean "the tool, unrestricted", and neither is content.
    content: Option<String>,
}

/// What is wrong with one rule.
#[derive(Debug, PartialEq, Eq)]
struct Problem {
    /// Which class it belongs to, for the reader who is scanning.
    kind: &'static str,
    /// What is wrong. One line: the summary is capped, and a remedy appended to
    /// it is a remedy that gets cut off exactly when it is longest.
    message: String,
    /// What to write instead, when there is a single answer.
    fix: Option<String>,
}

/// The station.
pub struct PermissionRules {
    id: StationId,
}

impl Default for PermissionRules {
    fn default() -> Self {
        Self {
            id: StationId::from_static("permission-rules"),
        }
    }
}

impl Station for PermissionRules {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "every permission rule on this machine is one the harness will honour"
    }

    fn derived_from(&self) -> Option<Derivation> {
        Some(Derivation {
            artefact: "the Claude Code settings validator, read from the bundle at",
            version: DERIVED_AGAINST,
            recipe: RECIPE,
            installed: newest_harness,
        })
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let files = discover(cx);
        if files.is_empty() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "this machine carries no settings files",
            )));
        }
        Ok(Outcome::Ran(
            files.iter().flat_map(|file| self.file(file)).collect(),
        ))
    }
}

impl PermissionRules {
    /// Every finding in one settings file.
    fn file(&self, at: &Utf8Path) -> Vec<Finding> {
        let broken = |kind: &'static str, message: String| {
            vec![
                self.id
                    .broken(Summary::lossy(&message))
                    .at(Location::file(at.to_owned()))
                    .detailed_with(Detail::new(format!("kind: {kind}"))),
            ]
        };

        let text = match fs_err::read_to_string(at) {
            Ok(text) => text,
            Err(error) => return broken("file", format!("a settings file is unreadable: {error}")),
        };
        let parsed: serde_json::Value = match serde_json::from_str(&text) {
            Ok(parsed) => parsed,
            Err(error) => {
                return broken(
                    "file",
                    format!(
                        "a settings file is not valid JSON ({error}) — the harness discards the \
                         whole file"
                    ),
                );
            }
        };
        let Some(object) = parsed.as_object() else {
            return broken("file", "a settings file is not a JSON object".to_owned());
        };
        let Some(permissions) = object.get("permissions") else {
            return Vec::new();
        };
        let Some(permissions) = permissions.as_object() else {
            return broken("file", "\"permissions\" is not an object".to_owned());
        };

        let mut findings = Vec::new();
        let mut listed: BTreeMap<&str, Vec<String>> = BTreeMap::new();
        for listing in RULE_LISTS {
            let Some(rules) = permissions.get(*listing) else {
                continue;
            };
            let Some(rules) = rules.as_array() else {
                findings.extend(broken(
                    "file",
                    format!("\"permissions.{listing}\" is not a list"),
                ));
                continue;
            };
            let mut raws = Vec::with_capacity(rules.len());
            for (position, raw) in rules.iter().enumerate() {
                match raw.as_str() {
                    Some(raw) => raws.push(raw.to_owned()),
                    None => findings.extend(broken(
                        "file",
                        format!("permissions.{listing}[{position}] is not a string"),
                    )),
                }
            }
            listed.insert(listing, raws);
        }
        findings.extend(self.lists(at, &listed));
        findings
    }

    /// Every finding across one file's rule lists.
    fn lists(&self, at: &Utf8Path, listed: &BTreeMap<&str, Vec<String>>) -> Vec<Finding> {
        let denied: BTreeSet<&str> = listed
            .get("deny")
            .map(|raws| raws.iter().map(String::as_str).collect())
            .unwrap_or_default();

        let mut findings = Vec::new();
        for listing in RULE_LISTS {
            let Some(raws) = listed.get(listing) else {
                continue;
            };
            let mut seen: BTreeSet<&str> = BTreeSet::new();
            for (position, raw) in raws.iter().enumerate() {
                let where_ = format!("{listing}[{position}] {raw}");
                if let Some(problem) = validate(raw, listing) {
                    let fix = if redundant(raw, raws) {
                        // A rewrite that is already written is a deletion, and
                        // that is the whole edit.
                        Some(
                            "delete this line — the rule that works is already there, so it is \
                             only noise"
                                .to_owned(),
                        )
                    } else {
                        problem.fix.clone()
                    };
                    findings.push(self.finding(
                        at,
                        problem.kind,
                        &format!("{where_}: {}", problem.message),
                        fix,
                    ));
                }
                if !seen.insert(raw) {
                    findings.push(self.finding(
                        at,
                        "duplicate",
                        &format!("{where_}: already in {listing} above"),
                        Some("delete one of them".to_owned()),
                    ));
                }
                if *listing == "allow" && denied.contains(raw.as_str()) {
                    findings.push(self.finding(
                        at,
                        "shadowed",
                        &format!("{where_}: also denied, and deny wins — this grants nothing"),
                        Some("delete the allow rule, or narrow the deny rule".to_owned()),
                    ));
                }
            }
        }
        findings
    }

    /// One finding, graded by what its class means for the machine.
    fn finding(&self, at: &Utf8Path, kind: &str, message: &str, fix: Option<String>) -> Finding {
        // `duplicate` and `shadowed` are untidy: the rule set still means what
        // it reads as. `invalid` takes the whole file's rules down with it,
        // `ineffective` is protection that is not there, and `overbroad`
        // approves more than it says — all three are a guard disarmed.
        let finding = if matches!(kind, "duplicate" | "shadowed") {
            self.id.soft(Summary::lossy(message))
        } else {
            self.id.broken(Summary::lossy(message))
        };
        let finding = finding
            .at(Location::file(at.to_owned()))
            .detailed_with(Detail::new(format!("kind: {kind}")));
        match fix {
            Some(fix) => finding.fixed_by(FixHint::lossy(&fix)),
            None => finding,
        }
    }
}

/// Every settings file the machine carries, in a stable order.
fn discover(cx: &Context) -> Vec<Utf8PathBuf> {
    let mut found: BTreeSet<Utf8PathBuf> = BTreeSet::new();
    for root in ROOTS {
        let at = cx.at(root);
        if !at.is_dir() {
            continue;
        }
        // `~/.claude` is itself the settings directory; the others carry
        // `.claude/` subdirectories.
        if *root == ".claude" {
            found.extend(settings_in(&at));
            continue;
        }
        walk(&at, 0, &mut found);
    }
    found.into_iter().collect()
}

/// Descend looking for `.claude` directories, and never into one.
fn walk(at: &Utf8Path, depth: usize, found: &mut BTreeSet<Utf8PathBuf>) {
    if depth > MAX_DEPTH {
        return;
    }
    let Ok(entries) = at.read_dir_utf8() else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !entry.file_type().is_ok_and(|kind| kind.is_dir()) {
            continue;
        }
        let Some(name) = path.file_name() else {
            continue;
        };
        if name == ".claude" {
            found.extend(settings_in(path));
            continue;
        }
        if SKIP.contains(&name) || (name.starts_with('.') && name != ".config") {
            continue;
        }
        walk(path, depth + 1, found);
    }
}

/// The settings files in one `.claude` directory.
fn settings_in(at: &Utf8Path) -> Vec<Utf8PathBuf> {
    let Ok(entries) = at.read_dir_utf8() else {
        return Vec::new();
    };
    entries
        .flatten()
        .map(|entry| entry.path().to_owned())
        .filter(|path| {
            path.is_file()
                && path.extension() == Some("json")
                && path
                    .file_name()
                    .is_some_and(|name| name.starts_with("settings"))
        })
        .collect()
}

/// The newest harness version installed, by the directory names it keeps.
///
/// Read from the filesystem rather than by running `claude --version`: the
/// station is detect-only, and starting the harness to ask it its version inside
/// a health check the harness is running is not free.
fn newest_harness(cx: &Context) -> Option<String> {
    let at = cx.at(VERSIONS);
    let mut versions: Vec<Vec<u64>> = at
        .read_dir_utf8()
        .ok()?
        .flatten()
        .filter_map(|entry| {
            entry.path().file_name().map(|name| {
                name.split('.')
                    .filter_map(|part| part.parse().ok())
                    .collect()
            })
        })
        .filter(|parts: &Vec<u64>| !parts.is_empty())
        .collect();
    versions.sort();
    versions.pop().map(|parts| {
        parts
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>()
            .join(".")
    })
}

/// Whether the rule that would actually work is already in the same list.
///
/// A rewrite becomes a deletion, and that is the whole edit.
fn redundant(raw: &str, raws: &[String]) -> bool {
    let rule = parse(raw);
    let Some(gate) = gate_for(&rule.tool) else {
        return false;
    };
    let Some(content) = rule.content else {
        return false;
    };
    raws.iter()
        .any(|other| *other == format!("{gate}({content})"))
}

/// The gate a path rule on this tool should have been written against.
fn gate_for(tool: &str) -> Option<&'static str> {
    WARNED_GATE
        .iter()
        .chain(SILENT_GATE)
        .find(|(key, _)| *key == tool)
        .map(|(_, gate)| *gate)
}

/// Whether `text[index]` is preceded by an odd run of backslashes.
fn escaped(text: &[char], index: usize) -> bool {
    let mut run = 0;
    let mut cursor = index;
    while cursor > 0 && text.get(cursor - 1) == Some(&'\\') {
        run += 1;
        cursor -= 1;
    }
    run % 2 != 0
}

/// How many unescaped `needle`s the text holds.
fn count_unescaped(text: &str, needle: char) -> usize {
    let chars: Vec<char> = text.chars().collect();
    chars
        .iter()
        .enumerate()
        .filter(|(index, c)| **c == needle && !escaped(&chars, *index))
        .count()
}

/// The first unescaped `needle`, as a char index.
fn first_unescaped(text: &str, needle: char) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len()).find(|index| chars.get(*index) == Some(&needle) && !escaped(&chars, *index))
}

/// The last unescaped `needle`, as a char index.
fn last_unescaped(text: &str, needle: char) -> Option<usize> {
    let chars: Vec<char> = text.chars().collect();
    (0..chars.len())
        .rev()
        .find(|index| chars.get(*index) == Some(&needle) && !escaped(&chars, *index))
}

/// Whether the text holds an unescaped `*`.
fn has_wildcard(text: &str) -> bool {
    count_unescaped(text, '*') > 0
}

/// Split `Tool(pattern)` the way the harness does.
///
/// Every malformed shape degrades to "the whole string is the tool name" rather
/// than to an error — which is why a rule with a trailing space silently becomes
/// a tool nothing is ever called.
fn parse(raw: &str) -> Rule {
    let chars: Vec<char> = raw.chars().collect();
    let canonical = |tool: &str| -> String {
        ALIASES
            .iter()
            .find(|(from, _)| *from == tool)
            .map_or_else(|| tool.to_owned(), |(_, to)| (*to).to_owned())
    };

    let Some(open) = first_unescaped(raw, '(') else {
        return Rule {
            tool: canonical(raw),
            content: None,
        };
    };
    let close = last_unescaped(raw, ')');
    let whole = || Rule {
        tool: canonical(raw),
        content: None,
    };
    let Some(close) = close else { return whole() };
    if close <= open || close != chars.len() - 1 {
        return whole();
    }
    let name: String = chars.get(..open).unwrap_or_default().iter().collect();
    if name.is_empty() {
        return whole();
    }
    let content: String = chars
        .get(open + 1..close)
        .unwrap_or_default()
        .iter()
        .collect();
    if content.is_empty() || content == "*" {
        return Rule {
            tool: canonical(&name),
            content: None,
        };
    }
    Rule {
        tool: canonical(&name),
        content: Some(
            content
                .replace("\\(", "(")
                .replace("\\)", ")")
                .replace("\\\\", "\\"),
        ),
    }
}

/// `mcp__<server>__<tool>` split, or nothing when this is not an MCP rule.
fn parse_mcp(tool: &str) -> Option<(String, Option<String>)> {
    let rest = tool.strip_prefix("mcp__")?;
    if rest.is_empty() {
        return None;
    }
    match rest.split_once("__") {
        Some((server, name)) if !server.is_empty() => {
            Some((server.to_owned(), Some(name.to_owned())))
        }
        Some(_) => None,
        None => Some((rest.to_owned(), None)),
    }
}

/// An allow rule must name the scope it widens.
fn wildcard_in_allow(tool: &str) -> Option<Problem> {
    if !has_wildcard(tool) {
        return None;
    }
    if parse_mcp(tool).is_some_and(|(server, _)| !has_wildcard(&server)) {
        return None;
    }
    Some(Problem {
        kind: "invalid",
        message: format!(
            "wildcard tool name \"{tool}\" is not supported in allow rules. Name the scope, or \
             use a literal mcp__<server>__ prefix"
        ),
        fix: None,
    })
}

/// The two tools with a validator of their own.
fn web_problem(tool: &str, content: &str) -> Option<Problem> {
    if tool == "WebSearch" && (content.contains('*') || content.contains('?')) {
        return Some(Problem {
            kind: "invalid",
            message: "WebSearch does not support wildcards — use exact search terms".to_owned(),
            fix: None,
        });
    }
    if tool != "WebFetch" {
        return None;
    }
    if content.contains("://") || content.starts_with("http") {
        return Some(Problem {
            kind: "invalid",
            message: "WebFetch permissions use domain format, not URLs — \
                      WebFetch(domain:example.com)"
                .to_owned(),
            fix: None,
        });
    }
    if !content.starts_with("domain:") {
        return Some(Problem {
            kind: "invalid",
            message: "WebFetch permissions must use the \"domain:\" prefix — \
                      WebFetch(domain:example.com)"
                .to_owned(),
            fix: None,
        });
    }
    None
}

/// A `Bash` allow rule whose wildcard sits before the rest of the command.
///
/// Such a rule also matches any options inserted at that position and approves
/// them with no prompt — for `git`, `-c` and `--exec-path` run arbitrary
/// commands. New in the harness since the port this was taken from, and carried
/// because it is the one rule here that widens authority rather than losing it.
fn wildcard_before_command(content: &str) -> Option<&str> {
    if content.ends_with(":*") {
        return None;
    }
    let words: Vec<&str> = content.split_whitespace().collect();
    let first = words.first()?;
    if words.len() < 3 || has_wildcard(first) {
        return None;
    }
    let mut wildcarded = false;
    for word in words.get(1..)? {
        if word.starts_with(['|', '&', ';', '<', '>']) {
            return None;
        }
        if has_wildcard(word) {
            wildcarded = true;
            continue;
        }
        if word.starts_with('-') {
            continue;
        }
        return wildcarded.then_some(*first);
    }
    None
}

/// The harness's validator, in its own order, plus the silent-gate case.
fn validate(raw: &str, listing: &str) -> Option<Problem> {
    let invalid = |message: String| {
        Some(Problem {
            kind: "invalid",
            message,
            fix: None,
        })
    };

    if raw.trim().is_empty() {
        return invalid("a permission rule cannot be empty".to_owned());
    }
    if count_unescaped(raw, '(') != count_unescaped(raw, ')') {
        return invalid("mismatched parentheses".to_owned());
    }
    let chars: Vec<char> = raw.chars().collect();
    let empty_parens = (0..chars.len().saturating_sub(1)).any(|index| {
        chars.get(index) == Some(&'(')
            && chars.get(index + 1) == Some(&')')
            && !escaped(&chars, index)
    });
    if empty_parens {
        let name = raw.split('(').next().unwrap_or_default();
        return if name.is_empty() {
            invalid("empty parentheses with no tool name".to_owned())
        } else {
            invalid(format!(
                "empty parentheses — write a pattern, or just \"{name}\""
            ))
        };
    }

    let rule = parse(raw);
    if let Some((server, _)) = parse_mcp(&rule.tool) {
        if rule.content.is_some() || count_unescaped(raw, '(') > 0 {
            return invalid(format!(
                "MCP rules take no pattern in parentheses — use \"{}\", or \"mcp__{server}__*\" \
                 for every tool on that server",
                rule.tool
            ));
        }
        return (listing == "allow")
            .then(|| wildcard_in_allow(&rule.tool))
            .flatten();
    }
    if rule.tool.is_empty() {
        return invalid("tool name cannot be empty".to_owned());
    }

    // Not the harness's check, and deliberately ahead of it. Every malformed
    // shape parses as a bare tool name, so `Bash(git log:*) ` with a trailing
    // space is a live rule for a tool that does not exist and reads exactly like
    // the rule it was meant to be. The harness rejects some of these further
    // down for an incidental reason and says so in terms that name a different
    // defect.
    if rule.tool.contains(['(', ')', ' ', '\t', '\n']) {
        return invalid(format!(
            "does not parse as Tool(pattern) — the whole string reads as one tool name, \
             \"{}\". A closing parenthesis must be the last character",
            rule.tool
        ));
    }

    if listing == "allow"
        && let Some(problem) = wildcard_in_allow(&rule.tool)
    {
        return Some(problem);
    }
    if !rule.tool.contains('_')
        && rule
            .tool
            .chars()
            .next()
            .is_some_and(|first| !first.is_uppercase())
    {
        return invalid("tool names start uppercase".to_owned());
    }

    let content = rule.content.as_deref()?;
    tool_problem(&rule, content, listing)
}

/// The checks that depend on which tool the rule names.
///
/// Split out of [`validate`] rather than inlined: the shape checks above answer
/// "did this parse", and these answer "does the harness consult it". Two
/// questions, and the second is the one that moves when the harness does.
fn tool_problem(rule: &Rule, content: &str, listing: &str) -> Option<Problem> {
    let invalid = |message: String| {
        Some(Problem {
            kind: "invalid",
            message,
            fix: None,
        })
    };

    if let Some(problem) = web_problem(&rule.tool, content) {
        return Some(problem);
    }

    if BASH_PREFIX_TOOLS.contains(&rule.tool.as_str()) {
        if content == ":*" {
            return invalid("prefix cannot be empty before :*".to_owned());
        }
        if content.contains(":*") && !content.ends_with(":*") {
            return invalid("the :* prefix pattern must sit at the end".to_owned());
        }
        if listing == "allow"
            && let Some(first) = wildcard_before_command(content)
        {
            {
                let extra = if first == "git" {
                    " For git, options such as -c and --exec-path run arbitrary commands."
                } else {
                    ""
                };
                return Some(Problem {
                    kind: "overbroad",
                    message: format!(
                        "a wildcard sits before the rest of the command, so it also matches \
                         options inserted there and approves them with no prompt.{extra}"
                    ),
                    fix: Some(
                        "replace that * with the value you mean, or use * only after the \
                         subcommand"
                            .to_owned(),
                    ),
                });
            }
        }
    }

    if FILE_PATTERN_TOOLS.contains(&rule.tool.as_str()) && content.contains(":*") {
        return invalid(
            "the \":*\" syntax is only for Bash prefix rules — use a glob (*, **) for file \
             matching"
                .to_owned(),
        );
    }

    // The harness suppresses its own warning when the pattern holds `:*`; a
    // `MultiEdit(a:*)` rule reaches here because MultiEdit is not a file-pattern
    // tool, and the harness says nothing about it.
    if content.contains(":*") {
        return None;
    }
    if let Some(gate) = WARNED_GATE
        .iter()
        .find(|(tool, _)| *tool == rule.tool)
        .map(|(_, gate)| *gate)
    {
        return Some(Problem {
            kind: "ineffective",
            message: "not matched by file permission checks — the harness warns about this \
                      once per session, to nobody in particular"
                .to_owned(),
            fix: Some(format!(
                "write {gate}({content}) instead — one {gate} rule covers every file-{} tool",
                verb(gate)
            )),
        });
    }
    if let Some(gate) = SILENT_GATE
        .iter()
        .find(|(tool, _)| *tool == rule.tool)
        .map(|(_, gate)| *gate)
    {
        return Some(Problem {
            kind: "ineffective",
            message: format!(
                "{} is not a file-permission tool and nothing warns about it, so the rule is \
                 silently dead and looks live",
                rule.tool
            ),
            fix: Some(format!(
                "write {gate}({content}) instead — one {gate} rule covers every file-{} tool",
                verb(gate)
            )),
        });
    }
    None
}

/// What a gate's rules cover, in the harness's own words.
fn verb(gate: &str) -> &'static str {
    if gate == "Edit" { "editing" } else { "reading" }
}

#[cfg(test)]
mod tests {
    use relic_core::finding::Severity;

    use super::*;

    /// A machine carrying settings files a test composes.
    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
    }

    impl Machine {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let home =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            Self { _dir: dir, home }
        }

        fn write(&self, relative: &str, body: &str) -> Utf8PathBuf {
            let at = self.home.join(relative);
            fs_err::create_dir_all(at.parent().expect("a parent")).expect("a parent");
            fs_err::write(&at, body).expect("written");
            at
        }

        /// One user-level settings file holding these permission lists.
        fn permissions(&self, json: &str) -> &Self {
            self.write(
                ".claude/settings.json",
                &format!("{{\"permissions\":{json}}}"),
            );
            self
        }

        fn outcome(&self) -> Outcome {
            PermissionRules::default()
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

    /// One rule in one list, as a settings body.
    fn one(listing: &str, rule: &str) -> String {
        format!("{{\"{listing}\":[\"{rule}\"]}}")
    }

    // --- the validator, against the harness's own tables --------------------

    #[test]
    fn a_live_rule_has_nothing_to_say() {
        assert_eq!(validate("Bash(git status:*)", "allow"), None);
        assert_eq!(validate("Edit(~/x/**)", "allow"), None);
        assert_eq!(validate("Read(**/*.rs)", "allow"), None);
        assert_eq!(validate("mcp__slack__users_search", "allow"), None);
        assert_eq!(validate("mcp__slack__*", "allow"), None);
    }

    #[test]
    fn a_path_rule_on_a_warned_tool_is_protection_that_is_not_there() {
        let problem = validate("Write(~/x/**)", "allow").expect("a problem");
        assert_eq!(problem.kind, "ineffective");
        let fix = problem.fix.as_deref().expect("one answer");
        assert!(fix.contains("Edit(~/x/**)"), "{problem:?}");
        assert!(fix.contains("file-editing"), "{problem:?}");
    }

    #[test]
    fn a_glob_rule_points_at_read_rather_than_edit() {
        let problem = validate("Glob(src/**)", "allow").expect("a problem");
        let fix = problem.fix.as_deref().expect("one answer");
        assert!(fix.contains("Read(src/**)"), "{problem:?}");
        assert!(fix.contains("file-reading"), "{problem:?}");
    }

    #[test]
    fn the_silent_gate_says_that_nothing_warns_about_it() {
        let problem = validate("Grep(src/**)", "deny").expect("a problem");
        assert_eq!(problem.kind, "ineffective");
        assert!(
            problem.message.contains("nothing warns about it"),
            "a rule that looks live is strictly worse than one the harness complains about: \
             {problem:?}"
        );
    }

    #[test]
    fn a_bare_tool_name_is_the_tool_unrestricted_and_is_never_ineffective() {
        assert_eq!(validate("Write", "allow"), None);
        assert_eq!(
            validate("Write(*)", "allow"),
            None,
            "a bare * is not content"
        );
        assert_eq!(
            validate("Write()", "allow").map(|p| p.kind),
            Some("invalid")
        );
    }

    #[test]
    fn a_trailing_space_makes_a_rule_a_tool_nothing_is_ever_called() {
        let problem = validate("Bash(git log:*) ", "allow").expect("a problem");
        assert_eq!(problem.kind, "invalid");
        assert!(
            problem.message.contains("does not parse"),
            "the defect is the shape, not an incidental complaint about the name: {problem:?}"
        );
    }

    #[test]
    fn an_escaped_parenthesis_is_content_and_not_structure() {
        assert_eq!(
            parse(r"Bash(echo \(hi\))"),
            Rule {
                tool: "Bash".to_owned(),
                content: Some("echo (hi)".to_owned()),
            }
        );
    }

    #[test]
    fn a_renamed_tool_is_honoured_under_its_new_name() {
        assert_eq!(parse("Task").tool, "Agent");
        assert_eq!(parse("KillBash(x)").tool, "TaskStop");
    }

    #[test]
    fn a_wildcard_tool_name_is_refused_in_allow_and_accepted_elsewhere() {
        assert_eq!(validate("Bash*", "allow").map(|p| p.kind), Some("invalid"));
        assert_eq!(validate("Bash*", "deny"), None);
        assert_eq!(
            validate("mcp__slack__get_*", "allow"),
            None,
            "a literal server prefix is the one place a glob is allowed"
        );
        assert_eq!(
            validate("mcp__*__x", "allow").map(|p| p.kind),
            Some("invalid")
        );
    }

    #[test]
    fn an_mcp_rule_takes_no_pattern() {
        let problem = validate("mcp__slack__users(x)", "allow").expect("a problem");
        assert!(problem.message.contains("no pattern"), "{problem:?}");
    }

    #[test]
    fn the_two_tools_with_their_own_validator_keep_it() {
        assert!(validate("WebSearch(a*)", "allow").is_some());
        assert!(validate("WebFetch(https://x.test)", "allow").is_some());
        assert!(validate("WebFetch(x.test)", "allow").is_some());
        assert_eq!(validate("WebFetch(domain:x.test)", "allow"), None);
    }

    #[test]
    fn the_bash_prefix_syntax_belongs_at_the_end_and_only_to_bash() {
        assert!(validate("Bash(:*)", "allow").is_some());
        assert!(validate("Bash(git:* log)", "allow").is_some());
        assert_eq!(
            validate("Read(src:*)", "allow").map(|p| p.kind),
            Some("invalid"),
            "a file-pattern tool takes a glob, not a prefix"
        );
    }

    #[test]
    fn a_wildcard_before_the_rest_of_the_command_approves_inserted_options() {
        let problem = validate("Bash(git * status)", "allow").expect("a problem");
        assert_eq!(problem.kind, "overbroad");
        assert!(
            problem.message.contains("--exec-path"),
            "git's own escape hatch is the reason it is worth naming: {problem:?}"
        );
    }

    #[test]
    fn a_wildcard_after_the_subcommand_is_the_arrangement_and_says_nothing() {
        assert_eq!(validate("Bash(git status *)", "allow"), None);
        assert_eq!(validate("Bash(git log:*)", "allow"), None);
        assert_eq!(
            validate("Bash(git * status)", "deny"),
            None,
            "widening only matters where a rule grants"
        );
    }

    #[test]
    fn a_pattern_holding_the_prefix_syntax_suppresses_the_warned_gate() {
        assert_eq!(
            validate("MultiEdit(a:*)", "allow"),
            None,
            "MultiEdit is not a file-pattern tool, so it reaches the gate check — and the \
             harness suppresses its own warning when the pattern holds `:*`"
        );
    }

    // --- the file, and the machine -----------------------------------------

    #[test]
    fn a_sound_settings_file_has_nothing_to_say() {
        let machine = Machine::new();
        machine.permissions(&one("allow", "Edit(~/x/**)"));
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn an_ineffective_rule_whose_remedy_is_already_present_is_only_noise() {
        let machine = Machine::new();
        machine.permissions(r#"{"allow":["Write(~/x/**)","Edit(~/x/**)"]}"#);
        let finding = machine.only();
        assert!(
            finding
                .fix
                .as_ref()
                .is_some_and(|fix| fix.as_str().contains("delete this line")),
            "a rewrite that is already written is a deletion, and that is the whole edit: \
             {finding:?}"
        );
    }

    #[test]
    fn the_same_rule_twice_is_soft_because_the_set_still_means_what_it_reads_as() {
        let machine = Machine::new();
        machine.permissions(r#"{"allow":["Edit(~/x/**)","Edit(~/x/**)"]}"#);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Soft);
        assert!(finding.summary.as_str().contains("already in allow above"));
    }

    #[test]
    fn an_allow_rule_that_is_also_denied_grants_nothing() {
        let machine = Machine::new();
        machine.permissions(r#"{"allow":["Edit(~/x/**)"],"deny":["Edit(~/x/**)"]}"#);
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Soft);
        assert!(finding.summary.as_str().contains("deny wins"));
    }

    #[test]
    fn an_invalid_rule_is_broken_because_it_takes_the_whole_file_down() {
        let machine = Machine::new();
        machine.permissions(&one("allow", "Write()"));
        assert_eq!(machine.only().severity, Severity::Broken);
    }

    #[test]
    fn settings_that_are_not_json_lose_every_rule_and_say_so() {
        let machine = Machine::new();
        machine.write(".claude/settings.json", "{ not json");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("discards the whole file"));
    }

    #[test]
    fn a_permission_list_that_is_not_a_list_is_reported_and_the_rest_still_read() {
        let machine = Machine::new();
        machine.permissions(r#"{"allow":"Edit(~/x/**)","deny":["Write(~/y/**)"]}"#);
        let findings = machine.findings();
        assert_eq!(findings.len(), 2, "{findings:?}");
        assert!(
            findings
                .iter()
                .any(|f| f.summary.as_str().contains("not a list"))
        );
        assert!(
            findings.iter().any(|f| f
                .fix
                .as_ref()
                .is_some_and(|fix| fix.as_str().contains("Edit(~/y/**)"))),
            "one bad list must not hide the rules in the others: {findings:?}"
        );
    }

    #[test]
    fn a_file_with_no_permissions_is_not_a_finding() {
        let machine = Machine::new();
        machine.write(".claude/settings.json", r#"{"model":"opus"}"#);
        assert!(machine.findings().is_empty());
    }

    #[test]
    fn a_machine_with_no_settings_at_all_is_skipped_rather_than_graded_clean() {
        let machine = Machine::new();
        let Outcome::Skipped(reason) = machine.outcome() else {
            panic!("nothing to read is a fact, not a pass");
        };
        assert!(reason.as_str().contains("no settings files"), "{reason}");
    }

    // --- discovery ----------------------------------------------------------

    #[test]
    fn a_rule_written_in_any_tree_is_found_which_is_the_whole_point() {
        let machine = Machine::new();
        machine.write(
            "Developer/org/repo/.claude/settings.local.json",
            &format!("{{\"permissions\":{}}}", one("allow", "Grep(src/**)")),
        );
        let finding = machine.only();
        assert!(
            finding.summary.as_str().contains("Grep"),
            "the class this closes is a rule authored in one tree and never seen again: \
             {finding:?}"
        );
    }

    #[test]
    fn an_expensive_directory_is_never_entered() {
        let machine = Machine::new();
        machine.write(
            "Developer/repo/node_modules/pkg/.claude/settings.json",
            &format!("{{\"permissions\":{}}}", one("allow", "Write(x)")),
        );
        assert!(
            matches!(machine.outcome(), Outcome::Skipped(_)),
            "a vendored tree's settings are not this machine's rules"
        );
    }

    #[test]
    fn discovery_stops_at_the_depth_cap_rather_than_walking_forever() {
        let machine = Machine::new();
        let deep = "Developer/a/b/c/d/e/f/.claude/settings.json";
        machine.write(deep, r#"{"permissions":{"allow":["Write(x)"]}}"#);
        assert!(matches!(machine.outcome(), Outcome::Skipped(_)));
    }

    #[test]
    fn only_a_settings_file_is_read_and_not_everything_beside_it() {
        let machine = Machine::new();
        machine.write(".claude/notes.json", "{ not json");
        machine.permissions(&one("allow", "Edit(~/x/**)"));
        assert!(machine.findings().is_empty());
    }

    // --- the derivation, and the runner-level staleness facility ------------

    #[test]
    fn the_newest_installed_harness_is_read_by_version_and_not_by_name() {
        let machine = Machine::new();
        for version in ["2.1.9", "2.1.10", "2.0.99"] {
            fs_err::create_dir_all(machine.home.join(VERSIONS).join(version)).expect("a version");
        }
        assert_eq!(
            newest_harness(&Context::new(machine.home.clone(), Vec::new())),
            Some("2.1.10".to_owned()),
            "sorted as numbers: a string sort would call 2.1.9 the newest"
        );
    }

    #[test]
    fn a_machine_with_no_installed_harness_offers_no_version_to_compare() {
        let machine = Machine::new();
        assert_eq!(
            newest_harness(&Context::new(machine.home.clone(), Vec::new())),
            None
        );
    }

    #[test]
    fn the_station_declares_what_its_tables_were_read_out_of() {
        let derivation = PermissionRules::default()
            .derived_from()
            .expect("a transcribing station must declare its source");
        assert_eq!(derivation.version, DERIVED_AGAINST);
        assert!(
            derivation.recipe.contains("grep"),
            "the recipe has to be runnable, not a description of one"
        );
    }

    #[test]
    fn a_harness_that_has_moved_on_is_noted_by_the_runner_and_never_graded() {
        let machine = Machine::new();
        machine.permissions(&one("allow", "Edit(~/x/**)"));
        fs_err::create_dir_all(machine.home.join(VERSIONS).join("99.0.0")).expect("a version");
        let reports = crate::run::run(
            &[Box::new(PermissionRules::default())],
            &Context::new(machine.home.clone(), Vec::new()),
        );
        let finding = reports
            .first()
            .and_then(|report| report.findings().first())
            .expect("the drift note");
        assert_eq!(finding.severity, Severity::Note);
        assert!(finding.summary.as_str().contains("99.0.0"), "{finding:?}");
        assert_eq!(
            crate::run::grade(&reports),
            relic_core::finding::Grade::Ok,
            "a checker written against an older release is not a broken machine"
        );
    }

    #[test]
    fn a_matching_harness_produces_no_note_at_all() {
        let machine = Machine::new();
        machine.permissions(&one("allow", "Edit(~/x/**)"));
        fs_err::create_dir_all(machine.home.join(VERSIONS).join(DERIVED_AGAINST))
            .expect("a version");
        let reports = crate::run::run(
            &[Box::new(PermissionRules::default())],
            &Context::new(machine.home.clone(), Vec::new()),
        );
        assert!(
            reports
                .first()
                .is_some_and(|report| report.findings().is_empty()),
            "the note exists to ask a question, not to be permanent furniture"
        );
    }
}
