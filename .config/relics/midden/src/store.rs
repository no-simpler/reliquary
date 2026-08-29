use camino::{Utf8Path, Utf8PathBuf};
use fs_err as fs;

use anyhow::{Context, Result, anyhow, bail};
use relic_core::frontmatter;
use relic_core::lock::{Lock, Wait};

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
pub fn corpus_root() -> Result<Utf8PathBuf> {
    if let Some(root) = std::env::var_os("MIDDEN_ROOT") {
        return Ok(relic_core::path::utf8(root.into())?);
    }
    let home = relic_core::path::home().ok_or_else(|| anyhow!("HOME is unset or not UTF-8"))?;
    Ok(home.join(".claude").join("midden"))
}

/// One note as it sits on disk. Parsing is total: a file whose metadata does
/// not deserialise still yields a record, so a malformed note can never vanish
/// from a listing.
pub struct Record {
    pub id: Id,
    pub path: Utf8PathBuf,
    /// Taken from the directory rather than from the metadata, so it is known
    /// even when the metadata does not parse.
    pub archived: bool,
    pub note: Result<Note, String>,
}

impl Record {
    pub fn occurrences(&self) -> u32 {
        self.note.as_ref().map_or(0, |note| note.occurrences)
    }
}

pub struct Corpus {
    pub root: Utf8PathBuf,
}

impl Corpus {
    pub fn open() -> Result<Corpus> {
        Ok(Corpus {
            root: corpus_root()?,
        })
    }

    fn shelf(&self, archived: bool) -> Utf8PathBuf {
        self.root.join(if archived { ARCHIVE } else { NOTES })
    }

    fn ensure_root(&self) -> Result<()> {
        fs::create_dir_all(&self.root)?;
        Ok(())
    }

    /// Held across every mutation, so two sessions filing at once cannot
    /// interleave a dedup read with the write that answers it. Dropping the
    /// returned handle releases it.
    fn lock(&self) -> Result<Lock> {
        self.ensure_root()?;
        Ok(Lock::acquire(&self.root.join(LOCK), Wait::INTERACTIVE)?)
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

    fn note_path(&self, id: Id, slug: &str, archived: bool) -> Utf8PathBuf {
        self.shelf(archived).join(format!("{id}-{slug}.md"))
    }

    pub fn create(&self, note: &Note, body: &str) -> Result<Utf8PathBuf> {
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
    pub fn save(&self, record: &Record, note: &Note) -> Result<Utf8PathBuf> {
        let _guard = self.lock()?;
        Self::save_locked(record, note)
    }

    /// The same, for callers already holding the lock across a batch.
    fn save_locked(record: &Record, note: &Note) -> Result<Utf8PathBuf> {
        let body = read_body(&record.path)?;
        write_atomic(&record.path, &render(note, &body)?)?;
        Ok(record.path.clone())
    }

    /// Archive rather than delete: a note is an observation, and nothing can
    /// regenerate one after the session that made it is gone.
    pub fn archive(&self, record: &Record) -> Result<Utf8PathBuf> {
        let _guard = self.lock()?;
        self.archive_locked(record)
    }

    fn archive_locked(&self, record: &Record) -> Result<Utf8PathBuf> {
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
        fs::rename(&record.path, &target).with_context(|| format!("archiving {}", record.path))?;
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
        let now = jiff::Timestamp::now();
        let mut swept = Vec::new();
        for record in self.list(false) {
            let Ok(note) = &record.note else { continue };
            let idle = relic_core::fmt::age_days(note.updated, now);
            let action = match note.status {
                Status::Dismissed if idle > DISMISSED_TTL_DAYS => Action::Dropped,
                Status::Actioned if idle > ACTIONED_TTL_DAYS => Action::Dropped,
                Status::Open if note.occurrences == 1 && idle > SINGLETON_IDLE_DAYS => {
                    Action::Archived
                }
                Status::Dismissed | Status::Actioned | Status::Open => continue,
            };
            if !dry_run {
                match action {
                    Action::Dropped => fs::remove_file(&record.path)
                        .with_context(|| format!("removing {}", record.path))?,
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
        Self::save_locked(record, &note)?;
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
fn filenames(dir: &Utf8Path) -> Vec<(Id, Utf8PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        // Named from the shelf rather than from the entry, so a name the
        // filesystem holds but this program cannot spell drops out here — where
        // it is one skipped file — rather than at a later conversion.
        let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
            continue;
        };
        let path = dir.join(&name);
        if !path.is_file() || path.extension() != Some("md") {
            continue;
        }
        let Some(id) = name
            .split('-')
            .next()
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
fn write_atomic(path: &Utf8Path, contents: &str) -> Result<()> {
    relic_core::fs::write_atomic(path, contents).with_context(|| format!("replacing {path}"))
}

pub fn load(path: &Utf8Path) -> Result<Note> {
    let text = fs::read_to_string(path)?;
    let (front, _) = frontmatter::split(&text)?;
    let note: Note = frontmatter::parse(front)?;
    note.validate()?;
    Ok(note)
}

pub fn read_body(path: &Utf8Path) -> Result<String> {
    let text = fs::read_to_string(path)?;
    let (_, body) = frontmatter::split(&text)?;
    Ok(body.to_owned())
}

pub fn render(note: &Note, body: &str) -> Result<String> {
    Ok(frontmatter::render(note, body)?)
}
