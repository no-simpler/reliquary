//! Drift between the paired POSIX and fish shell configuration.
//!
//! Aliases, abbreviations and functions are hand-maintained twice — once under
//! `shell/` for bash and zsh, once under `fish/conf.d/` — and there is nothing
//! that makes the two agree. This compares them.
//!
//! **Rebuilt, not ported.** Three things the shell checker did differently:
//!
//! - It scanned every field of every line for the token `alias`, so the prose
//!   "…so no alias is needed…" defined a phantom alias named `is`. That phantom
//!   is live in the tracked files today, and the check passed only because the
//!   fish twin carries the same sentence and it cancelled on both sides. That
//!   sentence is this station's first regression test.
//! - Its pair list was six hardcoded lines, so a new paired file was covered
//!   only if someone remembered. Pairs are discovered by stem here, and a file
//!   that defines names with no counterpart is a finding unless `parity.toml`
//!   says it is deliberate — the shape `yadm/unmanaged` and `brew/undeclared`
//!   already use for a decision worth recording once.
//! - It compared names only, and its own header conceded that values "cannot be
//!   compared across shells". They can be compared after normalising away the
//!   dialects, which is where `gu` meaning two different things gets caught.
//!
//! It also hard-failed before the first `yadm decrypt`, because its pair list
//! named an encrypt-lane file. An absent side of a pair is a skip here.

use std::collections::BTreeMap;

use anyhow::Result;
use camino::{Utf8Path, Utf8PathBuf};
use relic_core::finding::{Detail, Finding, Location, Outcome, StationId, Summary};
use serde::Deserialize;

use crate::station::{Context, Station};

/// Directories holding POSIX shell configuration, in load order.
const POSIX_DIRS: &[&str] = &[".config/shell/env.d", ".config/shell/interactive.d"];

/// Where fish keeps its half.
const FISH_DIR: &str = ".config/fish/conf.d";

/// The decisions this check is not allowed to make for itself.
const POLICY: &str = ".config/shell/parity.toml";

/// The station.
pub struct ShellParity {
    id: StationId,
}

impl Default for ShellParity {
    fn default() -> Self {
        Self {
            id: StationId::from_static("shell-parity"),
        }
    }
}

impl Station for ShellParity {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "alias, abbreviation and function names agree across POSIX and fish"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let fish_dir = cx.at(FISH_DIR);
        let posix_dirs: Vec<Utf8PathBuf> = POSIX_DIRS
            .iter()
            .map(|dir| cx.at(dir))
            .filter(|dir| dir.is_dir())
            .collect();
        if !fish_dir.is_dir() || posix_dirs.is_empty() {
            return Ok(Outcome::Skipped(Summary::lossy(
                "this machine carries only one of the two shell dialects",
            )));
        }

        let policy = Policy::read(&cx.at(POLICY))?;
        Ok(Outcome::Ran(compare(
            &self.id,
            cx.home(),
            &posix_dirs,
            &fish_dir,
            &policy,
        )))
    }
}

/// What the check is told rather than left to guess.
#[derive(Deserialize, Debug, Default)]
#[serde(deny_unknown_fields)]
struct Policy {
    /// Names defined on one side only, on purpose, keyed by the pair's stem.
    #[serde(default)]
    allow: BTreeMap<String, Vec<String>>,
    /// Bodies that differ on purpose, keyed by the pair's stem.
    #[serde(default)]
    diverge: BTreeMap<String, Vec<String>>,
    /// Files with no counterpart in the other dialect, and why. The value is
    /// the reason, so a decision is recorded rather than merely silenced.
    #[serde(default)]
    unpaired: BTreeMap<String, String>,
}

impl Policy {
    fn read(path: &Utf8Path) -> Result<Self> {
        match fs_err::read_to_string(path) {
            Ok(text) => Ok(toml::from_str(&text)?),
            // An absent policy grants nothing, which is the loud direction.
            Err(_) => Ok(Self::default()),
        }
    }

    fn allowed(&self, stem: &str, name: &str) -> bool {
        self.allow
            .get(stem)
            .is_some_and(|names| names.iter().any(|allowed| allowed == name))
    }

    fn may_diverge(&self, stem: &str, name: &str) -> bool {
        self.diverge
            .get(stem)
            .is_some_and(|names| names.iter().any(|allowed| allowed == name))
    }
}

/// What a name is bound to, as far as reading the file can tell.
#[derive(Clone, PartialEq, Eq, Debug)]
enum Body {
    /// A function's block. No single spelling the two dialects could share.
    Block,
    /// One definition, normalised into a dialect-neutral form.
    Text(String),
    /// Defined more than once, behind conditions a static reader cannot
    /// resolve — `ls` has three POSIX definitions, one per platform. Which one
    /// is live depends on the machine, so no comparison is honest.
    Conditional,
}

/// Everything one file defines.
type Definitions = BTreeMap<String, Body>;

/// Records a definition, noticing when a name already had a different one.
fn define(found: &mut Definitions, name: &str, body: Body) {
    match found.get(name) {
        Some(existing) if *existing == body => {}
        Some(_) => {
            found.insert(name.to_owned(), Body::Conditional);
        }
        None => {
            found.insert(name.to_owned(), body);
        }
    }
}

fn compare(
    station: &StationId,
    home: &Utf8Path,
    posix_dirs: &[Utf8PathBuf],
    fish_dir: &Utf8Path,
    policy: &Policy,
) -> Vec<Finding> {
    let posix: BTreeMap<String, Utf8PathBuf> = posix_dirs
        .iter()
        .flat_map(|dir| files(dir, "sh"))
        .filter_map(|path| stem(&path).map(|stem| (stem, path)))
        .collect();
    let fish: BTreeMap<String, Utf8PathBuf> = files(fish_dir, "fish")
        .into_iter()
        .filter_map(|path| stem(&path).map(|stem| (stem, path)))
        .collect();

    let mut findings = Vec::new();
    for (stem, posix_path) in &posix {
        let Some(fish_path) = fish.get(stem) else {
            findings.extend(unpaired(station, home, policy, stem, posix_path));
            continue;
        };
        findings.extend(pair(station, home, policy, stem, posix_path, fish_path));
    }
    for (stem, fish_path) in &fish {
        if !posix.contains_key(stem) {
            findings.extend(unpaired(station, home, policy, stem, fish_path));
        }
    }
    findings
}

/// A file with no counterpart. Silent when it defines nothing, and silent when
/// `parity.toml` records why it stands alone.
fn unpaired(
    station: &StationId,
    home: &Utf8Path,
    policy: &Policy,
    stem: &str,
    path: &Utf8Path,
) -> Vec<Finding> {
    let relative = relative(home, path);
    if policy.unpaired.contains_key(relative.as_str()) {
        return Vec::new();
    }
    let Ok(text) = fs_err::read_to_string(path) else {
        return Vec::new();
    };
    let defined = if path.extension() == Some("fish") {
        fish_definitions(&text)
    } else {
        posix_definitions(&text)
    };
    if defined.is_empty() {
        return Vec::new();
    }
    let names: Vec<&str> = defined.keys().map(String::as_str).collect();
    vec![with_detail(
        station
            .soft(Summary::lossy(&format!(
                "{stem} defines {} name(s) and has no counterpart in the other dialect",
                names.len()
            )))
            .at(Location::file(&relative)),
        &names.join(" "),
    )]
}

/// Attaches evidence when there is any. `Detail::new` refuses empty text, and a
/// caller that has nothing to show should say nothing rather than unwrap.
fn with_detail(finding: Finding, evidence: &str) -> Finding {
    match Detail::new(evidence) {
        Some(detail) => finding.detailed(detail),
        None => finding,
    }
}

/// One pair, compared both ways.
fn pair(
    station: &StationId,
    home: &Utf8Path,
    policy: &Policy,
    stem: &str,
    posix_path: &Utf8Path,
    fish_path: &Utf8Path,
) -> Vec<Finding> {
    let (Ok(posix_text), Ok(fish_text)) = (
        fs_err::read_to_string(posix_path),
        fs_err::read_to_string(fish_path),
    ) else {
        return vec![station.note(Summary::lossy(&format!(
            "{stem} could not be read on one side, so the pair is unjudged"
        )))];
    };

    let posix = posix_definitions(&posix_text);
    let fish = fish_definitions(&fish_text);
    let mut findings = Vec::new();

    let only = |left: &Definitions, right: &Definitions| -> Vec<String> {
        left.keys()
            .filter(|name| !right.contains_key(*name))
            .filter(|name| !policy.allowed(stem, name))
            .cloned()
            .collect()
    };

    let posix_only = only(&posix, &fish);
    let fish_only = only(&fish, &posix);

    if !posix_only.is_empty() {
        findings.push(with_detail(
            station
                .soft(Summary::lossy(&format!(
                    "{stem}: {} name(s) defined for POSIX and not for fish",
                    posix_only.len()
                )))
                .at(Location::file(relative(home, posix_path))),
            &posix_only.join(" "),
        ));
    }
    if !fish_only.is_empty() {
        findings.push(with_detail(
            station
                .soft(Summary::lossy(&format!(
                    "{stem}: {} name(s) defined for fish and not for POSIX",
                    fish_only.len()
                )))
                .at(Location::file(relative(home, fish_path))),
            &fish_only.join(" "),
        ));
    }

    let mut divergent: Vec<String> = Vec::new();
    for (name, posix_body) in &posix {
        let Some(fish_body) = fish.get(name) else {
            continue;
        };
        if policy.may_diverge(stem, name) {
            continue;
        }
        if let (Body::Text(left), Body::Text(right)) = (posix_body, fish_body)
            && left != right
        {
            divergent.push(format!("{name}: POSIX {left:?} vs fish {right:?}"));
        }
    }
    if !divergent.is_empty() {
        findings.push(with_detail(
            station
                .soft(Summary::lossy(&format!(
                    "{stem}: {} name(s) mean different things in the two dialects",
                    divergent.len()
                )))
                .at(Location::file(relative(home, posix_path))),
            &divergent.join("\n"),
        ));
    }

    findings
}

fn relative(home: &Utf8Path, path: &Utf8Path) -> Utf8PathBuf {
    path.strip_prefix(home)
        .map_or_else(|_| path.to_owned(), Utf8Path::to_owned)
}

/// Files with one extension in a directory, sorted.
fn files(dir: &Utf8Path, extension: &str) -> Vec<Utf8PathBuf> {
    let Ok(entries) = fs_err::read_dir(dir) else {
        return Vec::new();
    };
    let mut found: Vec<Utf8PathBuf> = entries
        .flatten()
        .filter_map(|entry| Utf8PathBuf::from_path_buf(entry.path()).ok())
        .filter(|path| path.is_file() && path.extension() == Some(extension))
        .collect();
    found.sort();
    found
}

/// The stem a pair is keyed on: the filename without its extension.
fn stem(path: &Utf8Path) -> Option<String> {
    path.file_stem().map(str::to_owned)
}

// --- Tokenizing ------------------------------------------------------------

/// The statements of one line, quote-aware.
///
/// Two things the field-scanning checker did not do, and both are defects it
/// carried. A `#` opens a comment only at the start of a word and only outside
/// quotes — otherwise the prose "…so no alias is needed…" defines a name. And a
/// `;`, `|` or `&` separates statements only outside quotes — otherwise
/// `alias gl="glfr | less -R"` is read as an alias whose body is `"glfr`.
///
/// Leading control tokens are dropped, so a definition behind a guard —
/// `[ -d X ] && alias …` — is still found at the head of its statement.
fn statements(line: &str) -> Vec<Vec<&str>> {
    let mut pieces: Vec<&str> = Vec::new();
    let (mut single, mut double, mut escaped) = (false, false, false);
    let mut start = 0usize;
    let mut end = line.len();
    let mut previous = ' ';

    for (at, c) in line.char_indices() {
        if escaped {
            escaped = false;
            previous = c;
            continue;
        }
        match c {
            '\\' if !single => escaped = true,
            '\'' if !double => single = !single,
            '"' if !single => double = !double,
            '#' if !single && !double && (at == 0 || previous.is_whitespace()) => {
                end = at;
                break;
            }
            ';' | '|' | '&' if !single && !double => {
                if let Some(piece) = line.get(start..at) {
                    pieces.push(piece);
                }
                start = at + c.len_utf8();
            }
            _ => {}
        }
        previous = c;
    }
    if start <= end
        && let Some(piece) = line.get(start..end)
    {
        pieces.push(piece);
    }

    pieces
        .into_iter()
        .map(|piece| {
            piece
                .split_whitespace()
                .skip_while(|word| {
                    matches!(*word, "then" | "do" | "else" | "{" | "(" | "and" | "or")
                })
                .collect::<Vec<_>>()
        })
        .filter(|words| !words.is_empty())
        .collect()
}

/// A body reduced to what both dialects can be compared on: quotes removed,
/// whitespace collapsed, a trailing separator dropped.
fn normalise(body: &str) -> String {
    let trimmed = body.trim();
    let unquoted = trimmed
        .strip_prefix('\'')
        .and_then(|rest| rest.strip_suffix('\''))
        .or_else(|| {
            trimmed
                .strip_prefix('"')
                .and_then(|rest| rest.strip_suffix('"'))
        })
        .unwrap_or(trimmed);
    let collapsed = unquoted.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim_end_matches(';');
    // POSIX escapes a name to bypass alias expansion — `\\cd`, `\\type` — and fish
    // has no such need, so the leading backslash is dialect, not meaning.
    trimmed.strip_prefix('\\').unwrap_or(trimmed).to_owned()
}

/// What a POSIX file defines: `alias [-g|-s|--] NAME=BODY`, and `NAME() {`.
fn posix_definitions(text: &str) -> Definitions {
    let mut found = Definitions::new();
    for line in text.lines() {
        if let Some(name) = function_name(line) {
            define(&mut found, &name, Body::Block);
            continue;
        }
        for words in statements(line) {
            let Some((keyword, rest)) = words.split_first() else {
                continue;
            };
            if *keyword != "alias" {
                continue;
            }
            let assignment = rest
                .iter()
                .skip_while(|word| matches!(**word, "-g" | "-s" | "--"))
                .copied()
                .collect::<Vec<_>>()
                .join(" ");
            if let Some((name, body)) = assignment.split_once('=') {
                define(&mut found, name, Body::Text(normalise(body)));
            }
        }
    }
    found
}

/// A POSIX function definition, in all three spellings bash and zsh accept:
/// `NAME() {`, `function NAME {`, and `function NAME() {`.
///
/// The keyword form is not decoration — `080-check.sh` uses it, and a reader
/// that only knew `NAME()` reported its function as fish-only.
fn function_name(line: &str) -> Option<String> {
    let trimmed = line.trim_start();
    let after_keyword = trimmed
        .strip_prefix("function ")
        .or_else(|| trimmed.strip_prefix("function\t"))
        .map(str::trim_start);
    let candidate = after_keyword.unwrap_or(trimmed);

    let name = match candidate.split_once('(') {
        Some((name, after)) if after.trim_start().starts_with(')') => name.trim(),
        // A bare name is a definition only when the keyword introduced it.
        _ => after_keyword?.split_whitespace().next()?,
    };

    (!name.is_empty()
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'))
    .then(|| name.to_owned())
}

/// What a fish file defines: `alias NAME BODY`, `alias NAME=BODY`,
/// `abbr [flags] NAME BODY…`, and `function NAME`.
fn fish_definitions(text: &str) -> Definitions {
    let mut found = Definitions::new();
    for line in text.lines() {
        for words in statements(line) {
            let Some((keyword, rest)) = words.split_first() else {
                continue;
            };
            match *keyword {
                "function" => {
                    if let Some(name) = rest.first() {
                        define(&mut found, name, Body::Block);
                    }
                }
                "alias" => {
                    let rest: Vec<&str> = rest
                        .iter()
                        .skip_while(|word| matches!(**word, "--" | "-s" | "--save"))
                        .copied()
                        .collect();
                    if let Some((name, body)) = rest.split_first() {
                        if let Some((name, inline)) = name.split_once('=') {
                            define(&mut found, name, Body::Text(normalise(inline)));
                        } else {
                            define(&mut found, name, Body::Text(normalise(&body.join(" "))));
                        }
                    }
                }
                "abbr" => {
                    let rest: Vec<&str> = rest
                        .iter()
                        .skip_while(|word| {
                            matches!(**word, "--add" | "-a" | "-g" | "--global" | "--" | "-U")
                        })
                        .copied()
                        .collect();
                    if let Some((name, body)) = rest.split_first() {
                        define(&mut found, name, Body::Text(normalise(&body.join(" "))));
                    }
                }
                _ => {}
            }
        }
    }
    found
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
            for dir in [POSIX_DIRS.last().copied().unwrap_or_default(), FISH_DIR] {
                fs_err::create_dir_all(root.join(dir)).expect("created");
            }
            Self { _keep: keep, root }
        }

        fn posix(&self, name: &str, text: &str) -> &Self {
            self.put(&format!(".config/shell/interactive.d/{name}.sh"), text)
        }

        fn fish(&self, name: &str, text: &str) -> &Self {
            self.put(&format!("{FISH_DIR}/{name}.fish"), text)
        }

        fn put(&self, relative: &str, text: &str) -> &Self {
            let path = self.root.join(relative);
            fs_err::create_dir_all(path.parent().expect("a parent")).expect("created");
            fs_err::write(path, text).expect("written");
            self
        }

        fn findings(&self) -> Vec<Finding> {
            match ShellParity::default()
                .check(&Context::new(self.root.clone(), Vec::new()))
                .expect("the station ran")
            {
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

        fn evidence(&self) -> String {
            self.findings()
                .iter()
                .filter_map(|finding| finding.detail.as_ref().map(ToString::to_string))
                .collect::<Vec<_>>()
                .join("\n")
        }
    }

    /// The defect that made this a rebuild rather than a port. The sentence is
    /// the one in the tracked files, verbatim.
    #[test]
    fn prose_in_a_comment_defines_nothing() {
        let comment =
            "# of Homebrew on $PATH by env.d/040-env.sh — so no alias is needed for the wrapper.\n";
        assert!(posix_definitions(comment).is_empty());
        assert!(fish_definitions(comment).is_empty());

        // And a `#` that is not at the start of a word is not a comment.
        let live = "alias h='git log --format=%h#%s'\n";
        assert_eq!(
            posix_definitions(live).get("h"),
            Some(&Body::Text("git log --format=%h#%s".to_owned()))
        );
    }

    #[test]
    fn a_separator_inside_quotes_does_not_end_the_statement() {
        let posix = posix_definitions("alias gl=\"glfr -10 | less -R\"\n");
        assert_eq!(
            posix.get("gl"),
            Some(&Body::Text("glfr -10 | less -R".to_owned()))
        );
        let fish = fish_definitions("alias gl 'glfr -10 | less -R'\n");
        assert_eq!(posix.get("gl"), fish.get("gl"));
    }

    #[test]
    fn a_definition_behind_a_guard_is_still_a_definition() {
        let found = posix_definitions("[ -d /opt ] && alias here='cd /opt'\n");
        assert_eq!(found.get("here"), Some(&Body::Text("cd /opt".to_owned())));
    }

    #[test]
    fn posix_functions_are_found_in_all_three_spellings() {
        for line in ["f() {", "function f {", "function f() {"] {
            let found = posix_definitions(&format!("{line}\n  :\n}}\n"));
            assert_eq!(found.get("f"), Some(&Body::Block), "{line}");
        }
    }

    #[test]
    fn a_backslash_that_only_escapes_alias_expansion_is_not_a_difference() {
        let posix = posix_definitions("alias which='\\type -a'\n");
        let fish = fish_definitions("alias which 'type -a'\n");
        assert_eq!(posix.get("which"), fish.get("which"));
    }

    #[test]
    fn fish_abbreviations_are_definitions_whatever_the_flags() {
        let found = fish_definitions("abbr --add gs 'git status'\nabbr -a -g gd 'git diff'\n");
        assert_eq!(found.get("gs"), Some(&Body::Text("git status".to_owned())));
        assert_eq!(found.get("gd"), Some(&Body::Text("git diff".to_owned())));
    }

    #[test]
    fn a_name_defined_twice_has_no_body_to_compare() {
        let found = posix_definitions(
            "if uname; then\n  alias ls='ls -FG'\nelse\n  alias ls='ls -F'\nfi\n",
        );
        assert_eq!(found.get("ls"), Some(&Body::Conditional));
    }

    #[test]
    fn a_pair_that_agrees_says_nothing() {
        let home = Home::new();
        home.posix("100-aliases", "alias gs='git status'\n");
        home.fish("100-aliases", "alias gs 'git status'\n");
        assert_eq!(home.summaries(), Vec::<String>::new());
    }

    #[test]
    fn a_name_on_one_side_only_is_soft_and_names_itself() {
        let home = Home::new();
        home.posix("100-aliases", "alias gs='git status'\nalias only='x'\n");
        home.fish("100-aliases", "alias gs 'git status'\n");
        let findings = home.findings();
        assert_eq!(findings.len(), 1);
        assert_eq!(findings.first().map(|f| f.severity), Some(Severity::Soft));
        assert!(home.evidence().contains("only"), "{}", home.evidence());
    }

    #[test]
    fn a_name_that_means_two_things_is_soft_where_a_name_check_saw_nothing() {
        let home = Home::new();
        home.posix("100-aliases", "alias gu='git reset'\n");
        home.fish("100-aliases", "alias gu 'git restore'\n");
        assert!(
            home.summaries()
                .iter()
                .any(|s| s.contains("mean different things")),
            "{:?}",
            home.summaries()
        );
    }

    #[test]
    fn the_policy_file_is_what_makes_a_difference_deliberate() {
        let home = Home::new();
        home.posix("100-aliases", "alias gu='a'\nalias only='x'\n");
        home.fish("100-aliases", "alias gu 'b'\n");
        assert_eq!(home.findings().len(), 2);

        home.put(
            POLICY,
            "[allow]\n\"100-aliases\" = [\"only\"]\n[diverge]\n\"100-aliases\" = [\"gu\"]\n",
        );
        assert_eq!(home.summaries(), Vec::<String>::new());
    }

    #[test]
    fn an_unpaired_file_that_defines_nothing_is_not_a_finding() {
        let home = Home::new();
        home.fish("040-env", "set -gx EDITOR vim\n");
        assert_eq!(home.summaries(), Vec::<String>::new());
    }

    #[test]
    fn an_unpaired_file_that_defines_names_is_soft_until_it_is_declared() {
        let home = Home::new();
        home.fish("sdk", "function __sdk_run\nend\n");
        assert!(
            home.summaries()
                .first()
                .is_some_and(|s| s.contains("no counterpart")),
            "{:?}",
            home.summaries()
        );

        home.put(
            POLICY,
            "[unpaired]\n\".config/fish/conf.d/sdk.fish\" = \"sdkman's fish shim\"\n",
        );
        assert_eq!(home.summaries(), Vec::<String>::new());
    }

    /// The shell checker hard-failed here, because its pair list named a file
    /// that only exists once the archive is decrypted.
    #[test]
    fn a_machine_with_one_dialect_is_skipped_not_failed() {
        let keep = tempfile::tempdir().expect("a scratch dir");
        let root = Utf8PathBuf::from_path_buf(keep.path().to_path_buf()).expect("utf-8");
        let outcome = ShellParity::default()
            .check(&Context::new(root, Vec::new()))
            .expect("the station ran");
        assert!(matches!(outcome, Outcome::Skipped(_)));
    }

    #[test]
    fn a_pair_is_found_wherever_its_posix_half_lives() {
        // 150-benefactor sits in env.d, not interactive.d, and is still a pair.
        let home = Home::new();
        home.put(
            ".config/shell/env.d/150-benefactor.sh",
            "alias bnd='cd /x'\n",
        );
        home.fish("150-benefactor", "alias bnd 'cd /y'\n");
        assert!(
            home.summaries()
                .iter()
                .any(|s| s.contains("mean different things")),
            "{:?}",
            home.summaries()
        );
    }
}
