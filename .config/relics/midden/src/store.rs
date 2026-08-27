use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::id::{Id, slugify};
use crate::note::{Note, Status};

const LOCK: &str = ".lock";
const NOTES: &str = "notes";
const ARCHIVE: &str = "archive";

/// A dismissed note is a decision already taken. It is kept only long enough
/// that the same session can see it was considered.
pub const DISMISSED_TTL_DAYS: i64 = 30;

/// An actioned note has become a directive somewhere. The fix is the record;
/// a quarter is long enough to notice the fix failing, after which recurrence
/// would have reopened it anyway.
pub const ACTIONED_TTL_DAYS: i64 = 90;

/// Seen once, half a year ago, never again: that is noise, and the corpus is
/// worth reading only while everything in it is still true.
pub const SINGLETON_IDLE_DAYS: i64 = 180;

/// Past this many open notes the corpus has stopped being a working list. The
/// bound is not enforced — a refused note would lose the observation — but it
/// is reported, because the answer is to drain it, not to raise the number.
pub const OPEN_CEILING: usize = 200;

/// `~/.claude/midden`, or wherever `MIDDEN_ROOT` points. The override exists so
/// tests and trial runs never touch the live corpus.
pub fn corpus_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("MIDDEN_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is unset"))?;
    Ok(PathBuf::from(home).join(".claude").join("midden"))
}

/// One note as it sits on disk. Parsing is total: a file whose metadata does
/// not deserialise still yields a record, so a malformed note can never vanish
/// from a listing.
pub struct Record {
    pub id: Id,
    pub path: PathBuf,
    /// Taken from the directory rather than from the metadata, so it is known
    /// even when the metadata does not parse.
    pub archived: bool,
    pub note: Result<Note, String>,
}

impl Record {
    pub fn occurrences(&self) -> u32 {
        self.note.as_ref().map(|note| note.occurrences).unwrap_or(0)
    }
}

pub struct Corpus {
    pub root: PathBuf,
}

impl Corpus {
    pub fn open() -> Result<Corpus> {
        Ok(Corpus {
            root: corpus_root()?,
        })
    }

    fn shelf(&self, archived: bool) -> PathBuf {
        self.root.join(if archived { ARCHIVE } else { NOTES })
    }

    fn ensure_root(&self) -> Result<()> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("creating {}", self.root.display()))?;
        Ok(())
    }

    /// Held across every mutation, so two sessions filing at once cannot
    /// interleave a dedup read with the write that answers it. Dropping the
    /// returned handle releases it.
    fn lock(&self) -> Result<File> {
        self.ensure_root()?;
        let file = File::create(self.root.join(LOCK))?;
        file.lock()?;
        Ok(file)
    }

    pub fn list(&self, archived: bool) -> Vec<Record> {
        let mut records: Vec<Record> = filenames(&self.shelf(archived))
            .into_iter()
            .map(|(id, path)| Record {
                id,
                archived,
                note: load(&path).map_err(|e| e.to_string()),
                path,
            })
            .collect();
        records.sort_by(|a, b| rank(b).cmp(&rank(a)).then_with(|| a.id.cmp(&b.id)));
        records
    }

    /// Ids are unique across the corpus, live and archived, so a lookup needs
    /// no other coordinate. Only the file that matches is opened.
    pub fn find(&self, id: Id) -> Result<Record> {
        for archived in [false, true] {
            if let Some((_, path)) = filenames(&self.shelf(archived))
                .into_iter()
                .find(|(candidate, _)| *candidate == id)
            {
                return Ok(Record {
                    id,
                    archived,
                    note: load(&path).map_err(|e| e.to_string()),
                    path,
                });
            }
        }
        bail!("no note with id {id}. Run midden list --all to see every note")
    }

    /// The live note already claiming this cause, if there is one. Archived
    /// notes never match: archiving is how a cause is retired, and a retired
    /// cause that comes back deserves a fresh note with a fresh date.
    pub fn by_fingerprint(&self, fingerprint: &str) -> Option<Record> {
        self.list(false).into_iter().find(|record| {
            record
                .note
                .as_ref()
                .is_ok_and(|note| note.fingerprint == fingerprint)
        })
    }

    /// Unique across the corpus and its archive, so an id is never reused even
    /// after the note that held it is gone.
    pub fn mint_id(&self) -> Id {
        let taken: Vec<Id> = [false, true]
            .into_iter()
            .flat_map(|archived| filenames(&self.shelf(archived)))
            .map(|(id, _)| id)
            .collect();
        loop {
            let id = Id::mint();
            if !taken.contains(&id) {
                return id;
            }
        }
    }

    fn note_path(&self, id: Id, slug: &str, archived: bool) -> PathBuf {
        self.shelf(archived).join(format!("{id}-{slug}.md"))
    }

    pub fn create(&self, note: &Note, body: &str) -> Result<PathBuf> {
        let _guard = self.lock()?;
        let path = self.note_path(note.id, &slugify(&note.title), false);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic(&path, &render(note, body)?)?;
        Ok(path)
    }

    /// Rewrites metadata in canonical order while preserving the body byte for
    /// byte, so the evidence a note was filed with is never rephrased by a
    /// later housekeeping pass.
    pub fn save(&self, record: &Record, note: &Note) -> Result<PathBuf> {
        let _guard = self.lock()?;
        self.save_locked(record, note)
    }

    /// The same, for callers already holding the lock across a batch.
    fn save_locked(&self, record: &Record, note: &Note) -> Result<PathBuf> {
        let body = read_body(&record.path)?;
        write_atomic(&record.path, &render(note, &body)?)?;
        Ok(record.path.clone())
    }

    /// Archive rather than delete: a note is an observation, and nothing can
    /// regenerate one after the session that made it is gone.
    pub fn archive(&self, record: &Record) -> Result<PathBuf> {
        let _guard = self.lock()?;
        self.archive_locked(record)
    }

    fn archive_locked(&self, record: &Record) -> Result<PathBuf> {
        if record.archived {
            return Ok(record.path.clone());
        }
        let dir = self.shelf(true);
        fs::create_dir_all(&dir)?;
        let name = record
            .path
            .file_name()
            .ok_or_else(|| anyhow!("note has no filename"))?;
        let target = dir.join(name);
        if target.exists() {
            fs::remove_file(&target)?;
        }
        fs::rename(&record.path, &target)
            .with_context(|| format!("archiving {}", record.path.display()))?;
        Ok(target)
    }

    /// Retention, applied to the live shelf only. The archive is terminal.
    ///
    /// Deleting rather than archiving a dismissed or spent note is deliberate:
    /// both have already had their decision recorded elsewhere, and keeping
    /// them would make the archive the same heap the live shelf just stopped
    /// being.
    pub fn sweep(&self, dry_run: bool) -> Result<Vec<Swept>> {
        let _guard = self.lock()?;
        let mut swept = Vec::new();
        for record in self.list(false) {
            let Ok(note) = &record.note else { continue };
            let idle = crate::ui::age_days(note.updated);
            let action = match note.status {
                Status::Dismissed if idle > DISMISSED_TTL_DAYS => Action::Dropped,
                Status::Actioned if idle > ACTIONED_TTL_DAYS => Action::Dropped,
                Status::Open if note.occurrences == 1 && idle > SINGLETON_IDLE_DAYS => {
                    Action::Archived
                }
                _ => continue,
            };
            if !dry_run {
                match action {
                    Action::Dropped => fs::remove_file(&record.path)
                        .with_context(|| format!("removing {}", record.path.display()))?,
                    Action::Archived => {
                        self.archive_locked(&record)?;
                    }
                }
            }
            swept.push(Swept {
                id: record.id,
                title: note.title.clone(),
                status: note.status,
                idle,
                action,
            });
        }
        Ok(swept)
    }

    /// Records another sighting against an existing note, under one lock so a
    /// concurrent filer cannot read the old count and write it back.
    pub fn bump(&self, record: &Record, at: jiff::Timestamp) -> Result<Note> {
        let _guard = self.lock()?;
        let mut note = record
            .note
            .clone()
            .map_err(|error| anyhow!("{} will not parse: {error}", record.id))?;
        note.saw(at);
        self.save_locked(record, &note)?;
        Ok(note)
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Dropped,
    Archived,
}

impl Action {
    pub fn as_str(self) -> &'static str {
        match self {
            Action::Dropped => "dropped",
            Action::Archived => "archived",
        }
    }
}

pub struct Swept {
    pub id: Id,
    pub title: String,
    pub status: Status,
    pub idle: i64,
    pub action: Action,
}

/// What puts a note at the top of a listing: how often it has happened, then
/// how recently. A malformed note sorts first regardless, because it is the one
/// thing a reader has to fix before anything else can be trusted.
fn rank(record: &Record) -> (u8, u32, i64) {
    match &record.note {
        Ok(note) => (0, note.occurrences, note.updated.as_second()),
        Err(_) => (1, u32::MAX, i64::MAX),
    }
}

/// Which notes sit on one shelf, and where their files are — from directory
/// entries alone. Nothing is opened, so a scan costs one readdir.
fn filenames(dir: &Path) -> Vec<(Id, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() || path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let Some(id) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.split('-').next())
            .and_then(|name| name.parse::<Id>().ok())
        else {
            continue;
        };
        found.push((id, path));
    }
    found
}

/// Atomic replacement lives in `relic-core`: both stores wrote this by hand, and
/// both copies replaced the destination's extension to name their temporary.
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    relic_core::fs::write_atomic(path, contents)
        .with_context(|| format!("replacing {}", path.display()))
}

/// Splits a document into its metadata and its body. The body is returned
/// untouched, so every rewrite preserves it exactly.
pub fn split(text: &str) -> Result<(&str, &str)> {
    let rest = text
        .strip_prefix("---\n")
        .or_else(|| text.strip_prefix("---\r\n"))
        .ok_or_else(|| anyhow!("no metadata: the file must open with a --- line"))?;

    let mut offset = 0;
    for line in rest.split_inclusive('\n') {
        if line.trim_end() == "---" {
            return Ok((&rest[..offset], &rest[offset + line.len()..]));
        }
        offset += line.len();
    }
    bail!("unterminated metadata: no closing --- line")
}

pub fn load(path: &Path) -> Result<Note> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let (front, _) = split(&text)?;
    let note: Note = serde_yaml_ng::from_str(front).map_err(|e| {
        // The opening `---` occupies line one, so the parser's line number is
        // one short of the line a reader would count in the file.
        match e.location() {
            Some(at) => anyhow!(
                "line {}, column {}: {}",
                at.line() + 1,
                at.column(),
                without_location(&e.to_string())
            ),
            None => anyhow!("{e}"),
        }
    })?;
    note.validate()?;
    Ok(note)
}

/// The parser appends its own coordinates, which are relative to the metadata
/// rather than to the file. One location per message, and it should be the one
/// a reader can act on.
fn without_location(message: &str) -> &str {
    match message.rfind(" at line ") {
        Some(cut) => message[..cut].trim_end(),
        None => message,
    }
}

pub fn read_body(path: &Path) -> Result<String> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let (_, body) = split(&text)?;
    Ok(body.to_owned())
}

pub fn render(note: &Note, body: &str) -> Result<String> {
    let front = serde_yaml_ng::to_string(note)?;
    Ok(format!("---\n{front}---\n{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitting_finds_the_body_untouched() {
        let text = "---\nid: b71c\n---\n# Title\n\nbody\n";
        let (front, body) = split(text).unwrap();
        assert_eq!(front, "id: b71c\n");
        assert_eq!(body, "# Title\n\nbody\n");
    }

    #[test]
    fn splitting_rejects_documents_without_metadata() {
        assert!(split("# no metadata\n").is_err());
        assert!(split("---\nid: b71c\n").is_err());
    }
}
