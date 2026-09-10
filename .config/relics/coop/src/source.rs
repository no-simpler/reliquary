//! The declaration: what a producer tells coop, once, in a tracked file.
//!
//! A source declares a *condition*, not a notification. That is the whole
//! integration protocol, and it is why neither migrated producer needed code
//! written for it: coop evaluates, so nothing has to remember to retract.

use std::time::Duration;

use anyhow::{Context, Result, bail};
use camino::{Utf8Path, Utf8PathBuf};
use jiff::Timestamp;
use relic_core::finding::{FixHint, Severity, StationId};
use serde::Deserialize;

use crate::paths::expand;
use crate::predicate::{Missing, Predicate, non_empty};
use crate::span;

/// How an asked source's stdout is read.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A relic-core `Report` on stdout. Every binary answering
    /// `doctor --format json` already speaks this.
    Findings,
    /// Non-blank lines, verbatim. The escape hatch that makes an arbitrary
    /// script a producer without teaching it anything.
    Text,
}

/// A declaration, compiled and validated.
#[derive(Clone, Debug)]
pub struct Source {
    /// The published id. Doubles as the station a text source's findings carry.
    pub id: StationId,
    /// The line, before `{age}` is resolved.
    pub summary: String,
    /// The line for a path that is not there at all, when that reads
    /// differently. Never having run and being overdue are different facts, and
    /// a template cannot say both.
    pub summary_missing: Option<String>,
    /// What retires it.
    pub fix: Option<FixHint>,
    /// How loudly.
    pub severity: Severity,
    /// The tier.
    pub tier: Tier,
}

/// Which of the two evaluation tiers a source lives in.
#[derive(Clone, Debug)]
pub enum Tier {
    /// Stat and arithmetic, inline, every prompt.
    When(Predicate),
    /// A command, memoized, refreshed off the prompt path.
    Ask(Ask),
}

/// An asked source, and the terms on which its answer may be reused.
#[derive(Clone, Debug)]
pub struct Ask {
    /// argv, program first.
    pub run: Vec<String>,
    /// How its stdout is read.
    pub kind: Kind,
    /// How long an answer stays good on the clock alone.
    pub refresh: Duration,
    /// What else invalidates it before the clock does.
    pub keys: Vec<Key>,
}

impl Ask {
    /// The program, which is what decides whether the source is dormant.
    pub fn program(&self) -> &str {
        self.run.first().map_or("", String::as_str)
    }

    /// The arguments after it.
    pub fn args(&self) -> &[String] {
        self.run.get(1..).unwrap_or(&[])
    }
}

/// One component of a cache key.
///
/// A TTL alone is a guess about how fast the world moves. A fingerprint is an
/// answer: `rote`'s cache file changing is exactly when its count can have
/// changed, so the command runs then and not on a timer.
#[derive(Clone, Debug)]
pub enum Key {
    /// A path's mtime and size.
    Path(Utf8PathBuf),
    /// The local calendar day, with a rollover hour that need not be midnight.
    Day {
        /// Hour the day is considered to turn over.
        hour: i8,
        /// Minute within that hour.
        minute: i8,
    },
}

// ---------------------------------------------------------------------------
// The wire form
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    source: Spec,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct Spec {
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    summary_missing: String,
    #[serde(default)]
    fix: String,
    #[serde(default)]
    severity: SeverityName,
    when: Option<WhenSpec>,
    ask: Option<AskSpec>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum SeverityName {
    #[default]
    Soft,
    Broken,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct WhenSpec {
    path: Option<Utf8PathBuf>,
    older_than: Option<String>,
    exists: Option<bool>,
    before: Option<String>,
    after: Option<String>,
    missing: Option<String>,
    all_of: Option<Vec<WhenSpec>>,
    any_of: Option<Vec<WhenSpec>>,
    not: Option<Box<WhenSpec>>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct AskSpec {
    run: Vec<String>,
    kind: Kind,
    refresh: String,
    #[serde(default)]
    keys: Vec<String>,
}

/// Read and compile one declaration.
///
/// # Errors
///
/// When the file cannot be read, does not parse, or describes a source that
/// could never say anything.
pub fn read(path: &Utf8Path) -> Result<Source> {
    let text = fs_err::read_to_string(path).with_context(|| format!("reading {path}"))?;
    let file: File = toml::from_str(&text).with_context(|| format!("parsing {path}"))?;
    compile(file.source).with_context(|| format!("in {path}"))
}

/// Every declaration in the drop-in directory, and the ones that would not
/// compile.
///
/// A bad file never takes the good ones down with it: the prompt is not the
/// place to learn that a config edit went wrong, and `doctor` is.
///
/// # Errors
///
/// When the directory exists but cannot be listed. An absent directory is an
/// empty coop, not a failure.
pub fn read_dir(dir: &Utf8Path) -> Result<(Vec<Source>, Vec<Broken>)> {
    let mut sources = Vec::new();
    let mut broken = Vec::new();
    if !dir.is_dir() {
        return Ok((sources, broken));
    }
    let mut files: Vec<Utf8PathBuf> = Vec::new();
    for entry in fs_err::read_dir(dir).with_context(|| format!("listing {dir}"))? {
        let entry = entry.with_context(|| format!("listing {dir}"))?;
        let path = relic_core::path::utf8(entry.path())?;
        if path.extension() == Some("toml") {
            files.push(path);
        }
    }
    files.sort();
    for path in files {
        match read(&path) {
            Ok(source) => sources.push(source),
            Err(error) => broken.push(Broken {
                path,
                why: format!("{error:#}"),
            }),
        }
    }
    Ok((sources, broken))
}

/// A declaration that would not compile.
#[derive(Clone, Debug)]
pub struct Broken {
    /// Which file.
    pub path: Utf8PathBuf,
    /// What was wrong with it.
    pub why: String,
}

fn compile(spec: Spec) -> Result<Source> {
    let id: StationId = spec
        .id
        .parse()
        .with_context(|| format!("the id {:?} is not a station id", spec.id))?;
    let tier = match (spec.when, spec.ask) {
        (Some(when), None) => Tier::When(compile_when(when)?),
        (None, Some(ask)) => Tier::Ask(compile_ask(ask)?),
        (Some(_), Some(_)) => bail!("a source declares either when or ask, never both"),
        (None, None) => bail!("a source declares either when or ask"),
    };
    if matches!(tier, Tier::When(_)) {
        if spec.summary.trim().is_empty() {
            bail!("a stat source has no other way to say anything, so summary is required");
        }
        // Every notice is a nag built to be dismissed, and a nag that cannot say
        // how is furniture the moment it is read.
        if spec.fix.trim().is_empty() {
            bail!("fix is required: a notice nobody can act on does not belong in the coop");
        }
    }
    Ok(Source {
        id,
        summary_missing: (!spec.summary_missing.trim().is_empty()).then_some(spec.summary_missing),
        summary: spec.summary,
        fix: (!spec.fix.trim().is_empty()).then(|| FixHint::lossy(&spec.fix)),
        severity: match spec.severity {
            SeverityName::Soft => Severity::Soft,
            SeverityName::Broken => Severity::Broken,
        },
        tier,
    })
}

fn compile_when(spec: WhenSpec) -> Result<Predicate> {
    if let Some(branches) = spec.all_of {
        let compiled = branches
            .into_iter()
            .map(compile_when)
            .collect::<Result<Vec<_>>>()?;
        non_empty(&compiled, "all-of")?;
        return Ok(Predicate::AllOf(compiled));
    }
    if let Some(branches) = spec.any_of {
        let compiled = branches
            .into_iter()
            .map(compile_when)
            .collect::<Result<Vec<_>>>()?;
        non_empty(&compiled, "any-of")?;
        return Ok(Predicate::AnyOf(compiled));
    }
    if let Some(inner) = spec.not {
        return Ok(Predicate::Not(Box::new(compile_when(*inner)?)));
    }
    if let Some(text) = spec.before {
        return Ok(Predicate::Before(instant(&text)?));
    }
    if let Some(text) = spec.after {
        return Ok(Predicate::After(instant(&text)?));
    }
    let Some(path) = spec.path else {
        bail!("a condition needs a path, a time, or a combinator");
    };
    let path = expand(&path)?;
    if let Some(text) = spec.older_than {
        let age = span::parse(&text)
            .with_context(|| format!("older-than {text:?} is not a duration like 1d or 4h"))?;
        let missing = match spec.missing.as_deref() {
            None | Some("fire") => Missing::Fire,
            Some("quiet") => Missing::Quiet,
            Some(other) => bail!("missing is fire or quiet, not {other:?}"),
        };
        return Ok(Predicate::OlderThan { path, age, missing });
    }
    match spec.exists {
        Some(true) => Ok(Predicate::Exists { path }),
        Some(false) => Ok(Predicate::Not(Box::new(Predicate::Exists { path }))),
        None => bail!("a path needs older-than or exists"),
    }
}

fn compile_ask(spec: AskSpec) -> Result<Ask> {
    if spec.run.is_empty() {
        bail!("run needs a program");
    }
    let refresh = span::parse(&spec.refresh)
        .with_context(|| format!("refresh {:?} is not a duration like 10m", spec.refresh))?;
    let keys = spec
        .keys
        .iter()
        .map(|key| compile_key(key))
        .collect::<Result<Vec<_>>>()?;
    Ok(Ask {
        run: spec.run,
        kind: spec.kind,
        refresh,
        keys,
    })
}

fn compile_key(text: &str) -> Result<Key> {
    let Some(rest) = text.strip_prefix("day") else {
        return Ok(Key::Path(expand(Utf8Path::new(text))?));
    };
    let clock = match rest.strip_prefix(':') {
        None if rest.is_empty() => "00:00",
        None => bail!("a day key is day or day:HH:MM, not {text:?}"),
        Some(clock) => clock,
    };
    let (hour, minute) = clock
        .split_once(':')
        .with_context(|| format!("a day key is day:HH:MM, not {text:?}"))?;
    Ok(Key::Day {
        hour: hour
            .parse()
            .with_context(|| format!("bad hour in {text:?}"))?,
        minute: minute
            .parse()
            .with_context(|| format!("bad minute in {text:?}"))?,
    })
}

fn instant(text: &str) -> Result<Timestamp> {
    text.parse()
        .with_context(|| format!("{text:?} is not an RFC 3339 instant"))
}

#[cfg(test)]
mod tests {
    use super::{Kind, Source, Tier, compile_key, read};
    use crate::predicate::Missing;
    use camino::Utf8PathBuf;

    fn write(text: &str) -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = Utf8PathBuf::from_path_buf(dir.path().join("s.toml")).unwrap();
        fs_err::write(&path, text).unwrap();
        (dir, path)
    }

    fn parse(text: &str) -> anyhow::Result<Source> {
        let (_dir, path) = write(text);
        read(&path)
    }

    #[test]
    fn the_up_declaration_compiles_to_a_stat_predicate() {
        let source = parse(
            r#"
[source]
id = "up"
summary = "{age} since the last system update"
fix = "up"

[source.when]
path = "/tmp/coop-test-stamp"
older-than = "1d"
missing = "fire"
"#,
        )
        .unwrap();
        assert_eq!(source.id.as_str(), "up");
        match source.tier {
            Tier::When(crate::predicate::Predicate::OlderThan { missing, age, .. }) => {
                assert_eq!(missing, Missing::Fire);
                assert_eq!(age.as_secs(), 86_400);
            }
            Tier::When(_) | Tier::Ask(_) => panic!("expected a stat predicate"),
        }
    }

    #[test]
    fn the_rote_declaration_compiles_to_an_asked_source() {
        let source = parse(
            r#"
[source]
id = "rote"

[source.ask]
run = ["rote", "banner", "--format", "json"]
kind = "findings"
refresh = "10m"
keys = ["~/.local/state/rote/cache.json", "day:04:00"]
"#,
        )
        .unwrap();
        match source.tier {
            Tier::Ask(ask) => {
                assert_eq!(ask.program(), "rote");
                assert_eq!(ask.kind, Kind::Findings);
                assert_eq!(ask.keys.len(), 2);
                assert_eq!(ask.args(), ["banner", "--format", "json"]);
            }
            Tier::When(_) => panic!("expected an asked source"),
        }
    }

    #[test]
    fn a_missing_path_may_say_something_of_its_own() {
        let source = parse(
            r#"
[source]
id = "up"
summary = "{age} since the last system update"
summary-missing = "no record of a system update"
fix = "up"

[source.when]
path = "/tmp/coop-test-stamp"
older-than = "1d"
"#,
        )
        .unwrap();
        assert_eq!(
            source.summary_missing.as_deref(),
            Some("no record of a system update")
        );
    }

    #[test]
    fn a_stat_source_without_a_fix_is_refused() {
        let error = parse(
            r#"
[source]
id = "x"
summary = "something happened"

[source.when]
path = "/tmp/x"
exists = true
"#,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("fix is required"));
    }

    #[test]
    fn a_source_declaring_neither_tier_is_refused() {
        let error = parse("[source]\nid = \"x\"\n").unwrap_err();
        assert!(format!("{error:#}").contains("either when or ask"));
    }

    #[test]
    fn a_source_declaring_both_tiers_is_refused() {
        let error = parse(
            r#"
[source]
id = "x"
summary = "s"
fix = "f"

[source.when]
path = "/tmp/x"
exists = true

[source.ask]
run = ["true"]
kind = "text"
refresh = "1h"
"#,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("never both"));
    }

    #[test]
    fn an_unknown_key_is_refused_rather_than_ignored() {
        let error = parse("[source]\nid = \"x\"\ntypo = 1\n").unwrap_err();
        assert!(format!("{error:#}").contains("typo"));
    }

    #[test]
    fn an_id_that_is_not_a_station_id_is_refused() {
        let error = parse(
            r#"
[source]
id = "Not An Id"
summary = "s"
fix = "f"

[source.when]
path = "/tmp/x"
exists = true
"#,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("station id"));
    }

    #[test]
    fn day_keys_parse_with_and_without_a_rollover() {
        assert!(matches!(
            compile_key("day").unwrap(),
            super::Key::Day { hour: 0, minute: 0 }
        ));
        assert!(matches!(
            compile_key("day:04:00").unwrap(),
            super::Key::Day { hour: 4, minute: 0 }
        ));
        assert!(compile_key("day:4").is_err());
    }

    #[test]
    fn a_path_key_expands_its_tilde() {
        match compile_key("~/x").unwrap() {
            super::Key::Path(path) => assert!(!path.as_str().contains('~')),
            super::Key::Day { .. } => panic!("expected a path key"),
        }
    }
}
