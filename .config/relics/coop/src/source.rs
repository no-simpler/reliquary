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

/// How a producer's answer is read.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    /// A relic-core `Report`. Every binary answering `doctor --format json`
    /// already speaks this.
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

/// Which of the three tiers a source lives in — picked by how expensive the
/// answer is.
#[derive(Clone, Debug)]
pub enum Tier {
    /// Stat and arithmetic, inline, every prompt.
    When(Predicate),
    /// A command, run every prompt under a hard budget.
    Ask(Ask),
    /// A report file the producer rewrites on its own cadence, read every
    /// prompt. The file is the producer's cache, and the producer owns it.
    Read(Read),
}

/// An asked source.
#[derive(Clone, Debug)]
pub struct Ask {
    /// argv, program first.
    pub run: Vec<String>,
    /// How its stdout is read.
    pub kind: Kind,
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

/// A read source.
#[derive(Clone, Debug)]
pub struct Read {
    /// The report the producer writes.
    pub path: Utf8PathBuf,
    /// How it is read.
    pub kind: Kind,
    /// How often the producer promises to rewrite it. Past this, doctor says
    /// the producer has stopped; the card still shows what is there.
    pub expect_every: Option<Duration>,
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
    read: Option<ReadSpec>,
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
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
struct ReadSpec {
    path: Utf8PathBuf,
    kind: Kind,
    expect_every: Option<String>,
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
    let tier = match (spec.when, spec.ask, spec.read) {
        (Some(when), None, None) => Tier::When(compile_when(when)?),
        (None, Some(ask), None) => Tier::Ask(compile_ask(ask)?),
        (None, None, Some(read)) => Tier::Read(compile_read(read)?),
        (None, None, None) => bail!("a source declares one of when, ask or read"),
        _ => bail!("a source declares one of when, ask or read, never more"),
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
    Ok(Ask {
        run: spec.run,
        kind: spec.kind,
    })
}

fn compile_read(spec: ReadSpec) -> Result<Read> {
    let expect_every = spec
        .expect_every
        .map(|text| {
            span::parse(&text)
                .with_context(|| format!("expect-every {text:?} is not a duration like 1d"))
        })
        .transpose()?;
    Ok(Read {
        path: expand(&spec.path)?,
        kind: spec.kind,
        expect_every,
    })
}

fn instant(text: &str) -> Result<Timestamp> {
    text.parse()
        .with_context(|| format!("{text:?} is not an RFC 3339 instant"))
}

#[cfg(test)]
mod tests {
    use super::{Kind, Source, Tier, read};
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
            Tier::When(_) | Tier::Ask(_) | Tier::Read(_) => panic!("expected a stat predicate"),
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
"#,
        )
        .unwrap();
        match source.tier {
            Tier::Ask(ask) => {
                assert_eq!(ask.program(), "rote");
                assert_eq!(ask.kind, Kind::Findings);
                assert_eq!(ask.args(), ["banner", "--format", "json"]);
            }
            Tier::When(_) | Tier::Read(_) => panic!("expected an asked source"),
        }
    }

    #[test]
    fn a_read_declaration_expands_its_path_and_keeps_its_promise() {
        let source = parse(
            r#"
[source]
id = "slow"

[source.read]
path = "~/.local/state/slow/report.json"
kind = "findings"
expect-every = "1d"
"#,
        )
        .unwrap();
        match source.tier {
            Tier::Read(read) => {
                assert!(!read.path.as_str().contains('~'));
                assert_eq!(read.expect_every.unwrap().as_secs(), 86_400);
            }
            Tier::When(_) | Tier::Ask(_) => panic!("expected a read source"),
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
    fn a_source_declaring_no_tier_is_refused() {
        let error = parse("[source]\nid = \"x\"\n").unwrap_err();
        assert!(format!("{error:#}").contains("one of when, ask or read"));
    }

    #[test]
    fn a_source_declaring_two_tiers_is_refused() {
        let error = parse(
            r#"
[source]
id = "x"
summary = "s"
fix = "f"

[source.when]
path = "/tmp/x"
exists = true

[source.read]
path = "/tmp/y"
kind = "text"
"#,
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("never more"));
    }

    #[test]
    fn an_unknown_key_is_refused_rather_than_ignored() {
        let error = parse("[source]\nid = \"x\"\ntypo = 1\n").unwrap_err();
        assert!(format!("{error:#}").contains("typo"));
    }

    #[test]
    fn a_retired_field_is_refused_rather_than_ignored() {
        let error = parse(
            "[source]\nid = \"x\"\n\n[source.ask]\nrun = [\"true\"]\nkind = \"text\"\nrefresh = \"1h\"\n",
        )
        .unwrap_err();
        assert!(format!("{error:#}").contains("refresh"));
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
}
