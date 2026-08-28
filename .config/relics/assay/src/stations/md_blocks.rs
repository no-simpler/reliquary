//! Auto-executed shell blocks in Claude Code markdown.
//!
//! A ```` ```! ```` fence, and the inline ``!`cmd` `` form, runs when the file
//! loads. There is no prompt path: the command is analysed statically and must
//! come back `allow` against the file's `allowed-tools`, or the whole invocation
//! throws. An inline program can therefore never run — it is denied for being
//! unanalysable, long before anything considers whether it is safe.
//!
//! Three ways a block gets denied, all checked here:
//!
//! 1. Claude Code's own `too-complex` prefilter. The rule ported below is the
//!    brace-with-quote one, applied after every `{` inside quotes is blanked,
//!    because it is the one an ordinary shell program trips without looking
//!    wrong: an unquoted `{` reaching a quote before its `}` is usually a shell
//!    function definition.
//! 2. Compound shape. The real analyser decomposes a compound command and
//!    requires every leaf to be allowed, which no multi-statement block
//!    survives. This is deliberately stricter — a single simple command, full
//!    stop. The question is "will this be denied", and a false positive costs
//!    one edit.
//! 3. Coverage. The head command must be granted by the file's `allowed-tools`
//!    frontmatter or by `~/.claude/settings.json`.

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use regex::Regex;
use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};

use crate::station::{Context, Station};

/// Where markdown whose `!` blocks Claude Code executes lives.
///
/// An explicit list rather than discovery: most markdown on this machine is
/// prose, and globbing all of it would be noise.
const ROOTS: &[&str] = &[".claude/skills", ".claude/commands", ".config/.claude"];

/// The settings file whose `permissions.allow` grants apply to every file.
const SETTINGS: &str = ".claude/settings.json";

/// Metacharacters that make a block more than one simple command.
const COMPOUND: &str = ";|&<>(){}`";

/// The patterns, compiled once per run.
///
/// Built fallibly rather than through a `LazyLock` that has to unwrap: these
/// are literals in this file, so a failure is impossible — and a construction
/// that cannot fail is worth spelling as one that returns `Result` when the
/// alternative is a suppression.
struct Patterns {
    fence: Regex,
    inline: Regex,
    brace_quote: Regex,
    rule: Regex,
    allowed_tools: Regex,
}

impl Patterns {
    fn new() -> Result<Self> {
        Ok(Self {
            fence: Regex::new(r"(?ms)^```!\s*$(.*?)^```\s*$")?,
            inline: Regex::new(r"!`([^`\n]+)`")?,
            brace_quote: Regex::new(r#"\{[^}]*['"]"#)?,
            rule: Regex::new(r"Bash\(([^)]*)\)")?,
            allowed_tools: Regex::new(r"(?m)^allowed-tools:\s*(.+)$")?,
        })
    }
}

/// The station.
pub struct MdBlocks {
    id: StationId,
}

impl Default for MdBlocks {
    fn default() -> Self {
        Self {
            id: StationId::from_static("md-shell-blocks"),
        }
    }
}

impl Station for MdBlocks {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "auto-executed ! blocks in Claude Code skills and commands would run"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let roots: Vec<Utf8PathBuf> = ROOTS
            .iter()
            .map(|root| cx.at(root))
            .filter(|root| root.is_dir())
            .collect();
        if roots.is_empty() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "no directory on this machine holds Claude Code markdown",
            )));
        }

        let patterns = Patterns::new()?;
        let mut findings = Vec::new();
        let settings = settings_grants(&patterns, &cx.at(SETTINGS), &self.id, &mut findings);
        for root in &roots {
            for path in markdown(root) {
                findings.extend(examine(&patterns, &self.id, cx.home(), &path, &settings));
            }
        }
        Ok(Outcome::Ran(findings))
    }
}

/// Every `.md` under a root, in a stable order.
fn markdown(root: &Utf8Path) -> Vec<Utf8PathBuf> {
    ignore::WalkBuilder::new(root)
        .standard_filters(false)
        .sort_by_file_name(std::ffi::OsStr::cmp)
        .build()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_some_and(|kind| kind.is_file()))
        .filter_map(|entry| Utf8PathBuf::from_path_buf(entry.into_path()).ok())
        .filter(|path| path.extension() == Some("md"))
        .collect()
}

/// One permission grant, as the matcher it becomes.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Grant {
    /// The command must be exactly this.
    Exact(String),
    /// The command must be this, or start with this and a space. An empty
    /// prefix is the bare `*`, which grants everything.
    Prefix(String),
}

impl Grant {
    fn covers(&self, command: &str) -> bool {
        match self {
            Self::Exact(text) => command == text,
            Self::Prefix(text) => {
                text.is_empty() || command == text || command.starts_with(&format!("{text} "))
            }
        }
    }
}

/// Rule bodies, normalised. `X:*` and the legacy `X *` are prefixes; a bare `X`
/// is an exact command.
fn grants<'a>(rules: impl IntoIterator<Item = &'a str>) -> Vec<Grant> {
    rules
        .into_iter()
        .map(str::trim)
        .filter(|rule| !rule.is_empty())
        .map(|rule| {
            if let Some(head) = rule.strip_suffix(":*").or_else(|| rule.strip_suffix(" *")) {
                Grant::Prefix(head.trim().to_owned())
            } else if rule == "*" {
                Grant::Prefix(String::new())
            } else {
                Grant::Exact(rule.to_owned())
            }
        })
        .collect()
}

/// The `Bash(...)` bodies inside a list of permission rules.
fn bash_rules<'a>(patterns: &Patterns, text: &'a str) -> Vec<&'a str> {
    patterns
        .rule
        .captures_iter(text)
        .filter_map(|caps| caps.get(1).map(|body| body.as_str()))
        .collect()
}

/// Grants from `~/.claude/settings.json`.
///
/// An unreadable settings file is a `Soft` finding rather than a silent empty
/// list: every grant it holds would otherwise vanish and every block would read
/// as ungranted, which is a wrong answer dressed as a right one.
fn settings_grants(
    patterns: &Patterns,
    path: &Utf8Path,
    station: &StationId,
    findings: &mut Vec<Finding>,
) -> Vec<Grant> {
    if !path.is_file() {
        return Vec::new();
    }
    let text = match fs_err::read_to_string(path) {
        Ok(text) => text,
        Err(error) => {
            findings.push(
                station
                    .soft(Summary::lossy(&format!(
                        "the settings file is unreadable, so its grants are ignored: {error}"
                    )))
                    .at(Location::file(path)),
            );
            return Vec::new();
        }
    };
    let Ok(data) = serde_json::from_str::<serde_json::Value>(&text) else {
        findings.push(
            station
                .soft(Summary::lossy(
                    "the settings file is not valid JSON, so its grants are ignored",
                ))
                .at(Location::file(path)),
        );
        return Vec::new();
    };
    let rules = data
        .get("permissions")
        .and_then(|permissions| permissions.get("allow"))
        .and_then(serde_json::Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or_default();
    grants(
        rules
            .iter()
            .filter_map(serde_json::Value::as_str)
            .flat_map(|rule| bash_rules(patterns, rule)),
    )
}

/// The `allowed-tools` grants a file declares, or `None` when it declares none.
fn frontmatter_grants(patterns: &Patterns, text: &str) -> Option<Vec<Grant>> {
    let body = text.strip_prefix("---")?;
    let end = body.find("\n---")?;
    let front = body.get(..end)?;
    let declared = patterns.allowed_tools.captures(front)?.get(1)?.as_str();
    Some(grants(bash_rules(patterns, declared)))
}

/// What a `!` block is written as.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Shape {
    Fence,
    Inline,
}

impl std::fmt::Display for Shape {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Fence => "fence",
            Self::Inline => "inline",
        })
    }
}

/// Every `!` block in one file, with the line it starts on.
fn blocks(patterns: &Patterns, text: &str) -> Vec<(usize, String, Shape)> {
    let mut found = Vec::new();
    let line_of = |offset: usize| {
        text.get(..offset)
            .map_or(1, |before| before.matches('\n').count() + 1)
    };
    for caps in patterns.fence.captures_iter(text) {
        if let (Some(whole), Some(body)) = (caps.get(0), caps.get(1)) {
            found.push((
                line_of(whole.start()),
                body.as_str().trim().to_owned(),
                Shape::Fence,
            ));
        }
    }
    for caps in patterns.inline.captures_iter(text) {
        if let (Some(whole), Some(body)) = (caps.get(0), caps.get(1)) {
            found.push((
                line_of(whole.start()),
                body.as_str().trim().to_owned(),
                Shape::Inline,
            ));
        }
    }
    found.sort_by_key(|(line, _, _)| *line);
    found
}

/// One file's findings.
fn examine(
    patterns: &Patterns,
    station: &StationId,
    home: &Utf8Path,
    path: &Utf8Path,
    settings: &[Grant],
) -> Vec<Finding> {
    let Ok(text) = fs_err::read_to_string(path) else {
        // Not a finding about the machine: a file that cannot be read has not
        // been checked, and saying so is the whole point of the station.
        return vec![
            station
                .soft(Summary::lossy(
                    "this file could not be read, so it was not checked",
                ))
                .at(Location::file(shorten(home, path))),
        ];
    };
    let found = blocks(patterns, &text);
    if found.is_empty() {
        return Vec::new();
    }

    let mut findings = Vec::new();
    let declared = frontmatter_grants(patterns, &text);
    if declared.is_none() {
        findings.push(
            station
                .soft(Summary::lossy(
                    "this file has ! blocks and declares no allowed-tools",
                ))
                .at(Location::file(shorten(home, path))),
        );
    }
    let mut allow = declared.unwrap_or_default();
    allow.extend_from_slice(settings);

    for (line, command, shape) in found {
        let at = Location::file(shorten(home, path)).at_line(line);
        let evidence = || {
            Detail::new(
                command
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(100)
                    .collect::<String>(),
            )
        };

        if command.is_empty() {
            findings.push(
                station
                    .broken(Summary::lossy(&format!("an empty ! {shape}")))
                    .at(at),
            );
        } else if patterns
            .brace_quote
            .is_match(&blank_quoted_braces(&command))
        {
            let mut finding = station
                .broken(Summary::lossy(
                    "a brace reaching a quote reads as expansion obfuscation",
                ))
                .at(at);
            if let Some(detail) = evidence() {
                finding = finding.detailed(detail);
            }
            findings.push(finding);
        } else if is_compound(&command) {
            let mut finding = station
                .broken(Summary::lossy("not a single simple command"))
                .at(at)
                .fixed_by(FixHint::lossy("extract it to a script on PATH"));
            if let Some(detail) = evidence() {
                finding = finding.detailed(detail);
            }
            findings.push(finding);
        } else if !allow.iter().any(|grant| grant.covers(&command)) {
            findings.push(
                station
                    .broken(Summary::lossy(&format!(
                        "`{command}` is granted by neither allowed-tools nor settings.json"
                    )))
                    .at(at),
            );
        }
    }
    findings
}

/// More than one simple command.
fn is_compound(command: &str) -> bool {
    command.contains('\n') || command.contains(COMPOUND_ANY) || command.contains("$(")
}

/// The predicate `COMPOUND` becomes.
const COMPOUND_ANY: fn(char) -> bool = |c| COMPOUND.contains(c);

/// A path as `~/…`, for a finding a person reads.
fn shorten(home: &Utf8Path, path: &Utf8Path) -> Utf8PathBuf {
    path.strip_prefix(home)
        .map_or_else(|_| path.to_owned(), |rest| Utf8Path::new("~").join(rest))
}

/// Claude Code's brace collapser, ported.
///
/// Every `{` inside single quotes, double quotes or backticks becomes a space,
/// so only unquoted braces survive into the brace-with-quote test. Comments pass
/// through untouched, as they do there.
fn blank_quoted_braces(text: &str) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::with_capacity(text.len());
    let (mut in_single, mut in_double, mut in_back) = (false, false, false);
    let mut at_cmd_start = true;
    let mut i = 0;

    let peek = |i: usize| chars.get(i).copied();

    while let Some(c) = peek(i) {
        if in_back {
            if c == '\\' && peek(i + 1).is_some_and(|next| "`\\$".contains(next)) {
                out.push(c);
                out.extend(peek(i + 1));
                i += 2;
                continue;
            }
            if c == '`' {
                in_back = false;
            }
            out.push(if c == '{' { ' ' } else { c });
            i += 1;
        } else if in_single {
            if c == '\'' {
                in_single = false;
            }
            out.push(if c == '{' { ' ' } else { c });
            i += 1;
        } else if in_double {
            if c == '\\' && peek(i + 1).is_some_and(|next| "\"\\`".contains(next)) {
                out.push(c);
                out.extend(peek(i + 1));
                i += 2;
                continue;
            }
            if c == '`' {
                in_back = true;
                out.push(c);
                i += 1;
                continue;
            }
            if c == '"' {
                in_double = false;
            }
            out.push(if c == '{' { ' ' } else { c });
            i += 1;
        } else if c == '\\' && peek(i + 1).is_some() {
            out.push(c);
            out.extend(peek(i + 1));
            if peek(i + 1) != Some('\n') {
                at_cmd_start = false;
            }
            i += 2;
        } else if c == '#' && at_cmd_start {
            while let Some(inside) = peek(i) {
                if inside == '\n' {
                    break;
                }
                out.push(inside);
                i += 1;
            }
            at_cmd_start = true;
        } else if c == '`' {
            in_back = true;
            at_cmd_start = false;
            out.push(c);
            i += 1;
        } else {
            if c == '\'' {
                in_single = true;
            } else if c == '"' {
                in_double = true;
            }
            at_cmd_start = " \t\n;|&()<>".contains(c);
            out.push(c);
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use relic_core::finding::Severity;

    use super::*;

    struct Home {
        _keep: tempfile::TempDir,
        root: Utf8PathBuf,
    }

    impl Home {
        fn new() -> Self {
            let keep = tempfile::tempdir().expect("a scratch dir");
            let root = Utf8PathBuf::from_path_buf(keep.path().to_path_buf()).expect("utf-8");
            Self { _keep: keep, root }
        }

        fn write(&self, relative: &str, text: &str) -> &Self {
            let path = self.root.join(relative);
            fs_err::create_dir_all(path.parent().expect("a parent")).expect("created");
            fs_err::write(&path, text).expect("written");
            self
        }

        fn outcome(&self) -> Outcome {
            MdBlocks::default()
                .check(&Context::new(self.root.clone(), Vec::new()))
                .expect("the station ran")
        }

        fn findings(&self) -> Vec<Finding> {
            match self.outcome() {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("skipped: {reason}"),
            }
        }

        fn summaries(&self) -> Vec<String> {
            self.findings()
                .iter()
                .map(|finding| finding.summary.to_string())
                .collect()
        }
    }

    fn severities(findings: &[Finding]) -> (usize, usize) {
        (
            findings
                .iter()
                .filter(|f| f.severity == Severity::Broken)
                .count(),
            findings
                .iter()
                .filter(|f| f.severity == Severity::Soft)
                .count(),
        )
    }

    #[test]
    fn a_machine_with_no_claude_markdown_is_skipped_not_passed() {
        let home = Home::new();
        assert!(matches!(home.outcome(), Outcome::Skipped(_)));
    }

    #[test]
    fn a_granted_simple_command_is_nothing_to_report() {
        let home = Home::new();
        home.write(
            ".claude/commands/ok.md",
            "---\nallowed-tools: Bash(pb)\n---\n!`pb`\n",
        );
        assert_eq!(home.summaries(), Vec::<String>::new());
    }

    #[test]
    fn a_file_with_blocks_and_no_declaration_is_soft() {
        let home = Home::new();
        home.write(".claude/commands/bare.md", "prose\n\n!`pb`\n");
        let findings = home.findings();
        // Soft for the missing declaration, broken for the block nothing grants.
        assert_eq!(severities(&findings), (1, 1));
        assert!(
            home.summaries()
                .iter()
                .any(|s| s.contains("declares no allowed-tools"))
        );
    }

    #[test]
    fn an_ungranted_command_is_broken_and_names_itself() {
        let home = Home::new();
        home.write(
            ".claude/commands/x.md",
            "---\nallowed-tools: Bash(pb)\n---\n!`relic list`\n",
        );
        let findings = home.findings();
        assert_eq!(severities(&findings), (1, 0));
        assert!(
            home.summaries()
                .first()
                .is_some_and(|s| s.contains("`relic list`")),
            "{:?}",
            home.summaries()
        );
    }

    #[test]
    fn a_compound_block_is_broken_however_it_is_spelled() {
        for body in [
            "echo one; echo two",
            "pb | head",
            "echo $(date)",
            "pb && pb",
            "pb\npb",
        ] {
            let home = Home::new();
            home.write(
                ".claude/commands/x.md",
                &format!("---\nallowed-tools: Bash(*)\n---\n```!\n{body}\n```\n"),
            );
            let findings = home.findings();
            assert_eq!(severities(&findings), (1, 0), "{body:?}");
        }
    }

    #[test]
    fn a_brace_reaching_a_quote_is_broken_and_a_quoted_one_is_not() {
        let home = Home::new();
        home.write(
            ".claude/commands/x.md",
            "---\nallowed-tools: Bash(*)\n---\n!`f() { echo \"hi\"; }`\n",
        );
        assert_eq!(severities(&home.findings()), (1, 0));

        // The brace is inside quotes, so the prefilter blanks it. What is left
        // is an ordinary command, and only the compound rule may still object.
        let collapsed = blank_quoted_braces("echo '{ literal }'");
        let patterns = Patterns::new().expect("valid patterns");
        assert!(!patterns.brace_quote.is_match(&collapsed), "{collapsed}");
    }

    /// The collapser is the ported half, and the half a reader cannot check by
    /// eye. Each case is one state the machine has to leave correctly.
    #[test]
    fn the_brace_collapser_blanks_only_quoted_braces() {
        let cases = [
            // Unquoted braces survive, so the brace-with-quote rule can see them.
            ("f() { echo x; }", "f() { echo x; }"),
            // Quoted ones do not.
            ("echo '{ literal }'", "echo '  literal }'"),
            ("echo \"{ literal }\"", "echo \"  literal }\""),
            ("echo `{ sub }`", "echo `  sub }`"),
            // An escape inside a quote consumes its partner rather than ending
            // the quote, so what follows is still quoted.
            ("echo \"a\\\"{ b }\"", "echo \"a\\\"  b }\""),
            ("echo `a\\`{ b }`", "echo `a\\`  b }`"),
            // A backtick inside double quotes opens a substitution, not a string.
            ("echo \"`{ x }`\"", "echo \"`  x }`\""),
            // A comment at the start of a command passes through untouched.
            ("# { a comment }\nls", "# { a comment }\nls"),
            // A hash that is not at the start of a command is not a comment.
            ("echo a#b", "echo a#b"),
            // An escape outside quotes keeps its partner.
            ("echo \\{ x", "echo \\{ x"),
        ];
        for (input, want) in cases {
            assert_eq!(blank_quoted_braces(input), want, "{input:?}");
        }
    }

    #[test]
    fn a_path_outside_home_keeps_its_own_spelling() {
        let home = Utf8Path::new("/Users/example");
        assert_eq!(
            shorten(home, Utf8Path::new("/Users/example/.claude/x.md")),
            Utf8PathBuf::from("~/.claude/x.md")
        );
        assert_eq!(
            shorten(home, Utf8Path::new("/opt/elsewhere/x.md")),
            Utf8PathBuf::from("/opt/elsewhere/x.md")
        );
    }

    #[test]
    fn a_file_with_no_blocks_at_all_says_nothing() {
        let home = Home::new();
        home.write(".claude/commands/prose.md", "just prose, no blocks
");
        assert_eq!(home.summaries(), Vec::<String>::new());
    }

    #[test]
    fn frontmatter_without_allowed_tools_reads_as_no_declaration() {
        let home = Home::new();
        home.write(
            ".claude/commands/x.md",
            "---\ndescription: something\n---\n!`pb`\n",
        );
        assert!(
            home.summaries()
                .iter()
                .any(|s| s.contains("declares no allowed-tools")),
            "{:?}",
            home.summaries()
        );
    }

    #[test]
    fn settings_that_declare_no_permissions_grant_nothing_and_say_nothing() {
        let home = Home::new();
        home.write(".claude/settings.json", "{}");
        home.write(
            ".claude/commands/x.md",
            "---\nallowed-tools: Bash(pb)\n---\n!`pb`\n",
        );
        assert_eq!(home.summaries(), Vec::<String>::new());
    }

    #[test]
    fn a_settings_grant_reaches_every_file() {
        let home = Home::new();
        home.write(
            ".claude/settings.json",
            r#"{"permissions":{"allow":["Bash(git status:*)"]}}"#,
        );
        home.write(
            ".claude/commands/x.md",
            "---\nallowed-tools: Bash(pb)\n---\n!`git status --short`\n",
        );
        assert_eq!(home.summaries(), Vec::<String>::new());
    }

    #[test]
    fn unreadable_settings_are_reported_rather_than_treated_as_empty() {
        let home = Home::new();
        home.write(".claude/settings.json", "{ not json");
        home.write(
            ".claude/commands/x.md",
            "---\nallowed-tools: Bash(pb)\n---\n!`pb`\n",
        );
        let findings = home.findings();
        assert_eq!(severities(&findings), (0, 1));
        assert!(
            home.summaries()
                .first()
                .is_some_and(|s| s.contains("not valid JSON")),
            "{:?}",
            home.summaries()
        );
    }

    #[test]
    fn a_grant_covers_what_its_form_says_and_no_more() {
        let prefix = grants(["docket announce:*"]);
        assert!(prefix.first().is_some_and(|g| g.covers("docket announce")));
        assert!(
            prefix
                .first()
                .is_some_and(|g| g.covers("docket announce --hook"))
        );
        assert!(!prefix.first().is_some_and(|g| g.covers("docket announced")));

        let legacy = grants(["git log *"]);
        assert!(
            legacy
                .first()
                .is_some_and(|g| g.covers("git log --oneline"))
        );

        let everything = grants(["*"]);
        assert!(
            everything
                .first()
                .is_some_and(|g| g.covers("anything at all"))
        );

        let exact = grants(["pb"]);
        assert!(exact.first().is_some_and(|g| g.covers("pb")));
        assert!(!exact.first().is_some_and(|g| g.covers("pb --all")));
    }

    #[test]
    fn an_empty_fence_is_broken() {
        let home = Home::new();
        home.write(
            ".claude/commands/x.md",
            "---\nallowed-tools: Bash(*)\n---\n```!\n\n```\n",
        );
        let findings = home.findings();
        assert_eq!(severities(&findings), (1, 0));
        assert!(
            home.summaries()
                .first()
                .is_some_and(|s| s.contains("empty")),
            "{:?}",
            home.summaries()
        );
    }

    #[test]
    fn a_finding_says_which_line_it_is_on() {
        let home = Home::new();
        home.write(
            ".claude/commands/x.md",
            "---\nallowed-tools: Bash(pb)\n---\nprose\n\n!`relic list`\n",
        );
        let location = home
            .findings()
            .first()
            .and_then(|finding| finding.location.clone())
            .expect("a location");
        assert_eq!(location.line, Some(6));
        assert!(
            location.path.as_str().starts_with("~/"),
            "{}",
            location.path
        );
    }
}
