use std::fs::{self, File};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};

use crate::git;
use crate::id::Id;
use crate::item::{Item, Kind, Wire};

const SENTINEL: &str = ".project";
const LOCK: &str = ".lock";
const SPEC_FILE: &str = "spec.md";
const ORDER_STEP: i64 = 10;

/// `~/.claude/docket`, or wherever `DOCKET_ROOT` points. The override exists so
/// tests and trial runs never touch the live depot.
pub fn depot_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("DOCKET_ROOT") {
        return Ok(PathBuf::from(root));
    }
    let home = std::env::var_os("HOME").ok_or_else(|| anyhow!("HOME is unset"))?;
    Ok(PathBuf::from(home).join(".claude").join("docket"))
}

/// Claude Code's own project-directory convention, so a docket sits beside the
/// transcript directory for the same project.
pub fn slug_for_path(path: &Path) -> String {
    path.to_string_lossy()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect()
}

fn digest(path: &Path) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghjkmnpqrstvwxyz";
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.to_string_lossy().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (0..4)
        .map(|i| ALPHABET[((hash >> (i * 5)) as usize) % ALPHABET.len()] as char)
        .collect()
}

/// One item as it sits on disk. Parsing is total: a file whose frontmatter does
/// not deserialise still yields a record, so a malformed item can never vanish
/// from a listing.
pub struct Record {
    pub id: Id,
    /// Where the file sits, which is what decides how it is moved and removed.
    /// Taken from the directory rather than from the frontmatter, so it is
    /// known even when the frontmatter does not parse.
    pub kind: Kind,
    pub path: PathBuf,
    pub project: PathBuf,
    pub item: Result<Item, String>,
}

impl Record {
    pub fn order(&self) -> i64 {
        self.item.as_ref().map(|i| i.order).unwrap_or(i64::MAX)
    }
}

pub struct Depot {
    pub root: PathBuf,
}

impl Depot {
    pub fn open() -> Result<Depot> {
        Ok(Depot {
            root: depot_root()?,
        })
    }

    /// The directory holding one project's items. The slug is lossy, so the
    /// sentinel decides: a slug already claimed by a different path falls back
    /// to a digest-suffixed sibling rather than merging two dockets.
    pub fn project_dir(&self, project: &Path) -> PathBuf {
        let slug = slug_for_path(project);
        let base = self.root.join(&slug);
        match fs::read_to_string(base.join(SENTINEL)) {
            Ok(claimed) if claimed.trim() != project.to_string_lossy() => {
                self.root.join(format!("{slug}--{}", digest(project)))
            }
            _ => base,
        }
    }

    fn ensure_project(&self, project: &Path) -> Result<PathBuf> {
        let dir = self.project_dir(project);
        fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
        let sentinel = dir.join(SENTINEL);
        if !sentinel.exists() {
            fs::write(&sentinel, format!("{}\n", project.display()))
                .with_context(|| format!("writing {}", sentinel.display()))?;
        }
        Ok(dir)
    }

    /// Held across every mutation, so two sessions cannot interleave an
    /// ordering rewrite. Dropping the returned handle releases it.
    ///
    /// Always nested inside the depot lock, never taken around it — see the
    /// ordering rule in `git`.
    fn lock(&self, project: &Path) -> Result<File> {
        let dir = self.ensure_project(project)?;
        let file = File::create(dir.join(LOCK))?;
        file.lock()?;
        Ok(file)
    }

    /// The coarse lock, held by a whole mutating command, so a snapshot and the
    /// change it brackets are one unit against a concurrent session.
    pub fn lock_depot(&self) -> Result<File> {
        fs::create_dir_all(&self.root)
            .with_context(|| format!("creating {}", self.root.display()))?;
        let file = File::create(self.root.join(LOCK))?;
        file.lock()?;
        Ok(file)
    }

    /// The depot lock if it is free this instant, and nothing if it is not.
    /// What runs at session start takes this one: a hook that waits on another
    /// session's write would be a hook that hangs a terminal.
    pub fn try_lock_depot(&self) -> Option<File> {
        let file = File::create(self.root.join(LOCK)).ok()?;
        file.try_lock().ok().map(|()| file)
    }

    /// Every project that has a docket directory.
    pub fn projects(&self) -> Vec<PathBuf> {
        let mut found = Vec::new();
        let Ok(entries) = fs::read_dir(&self.root) else {
            return found;
        };
        for entry in entries.flatten() {
            if !entry.path().is_dir() {
                continue;
            }
            if let Ok(claimed) = fs::read_to_string(entry.path().join(SENTINEL)) {
                found.push(PathBuf::from(claimed.trim()));
            }
        }
        found.sort();
        found
    }

    pub fn list(&self, project: &Path) -> Vec<Record> {
        let dir = self.project_dir(project);
        let mut records = Vec::new();
        for kind in Kind::ALL {
            collect(&dir.join(kind.dir()), kind, project, &mut records);
        }
        records.sort_by(|a, b| a.order().cmp(&b.order()).then_with(|| a.id.cmp(&b.id)));
        records
    }

    /// Every place an open item can sit, for the scans that only need
    /// filenames. A closed item sits nowhere: history holds it.
    fn shelves(&self, project: &Path) -> Vec<(Kind, PathBuf)> {
        let dir = self.project_dir(project);
        Kind::ALL
            .into_iter()
            .map(|kind| (kind, dir.join(kind.dir())))
            .collect()
    }

    /// Ids are unique across the whole depot, so a lookup needs no project —
    /// which is what makes an id copied out of one terminal usable in another.
    /// Only the file that matches is opened.
    pub fn find(&self, id: Id) -> Result<Record> {
        for project in self.projects() {
            for (kind, shelf) in self.shelves(&project) {
                if let Some((_, path)) = filenames(&shelf, kind)
                    .into_iter()
                    .find(|(candidate, _)| *candidate == id)
                {
                    return Ok(Record {
                        id,
                        kind,
                        item: load(&path).map_err(|e| e.to_string()),
                        path,
                        project,
                    });
                }
            }
        }
        let mut message =
            format!("no item with id {id}. Run docket list --all to see every open item");
        if git::Repo::open(&self.root).is_some() {
            message.push_str(&format!(
                ". If it was closed, history holds it: git -C {} log --diff-filter=D --name-only",
                self.root.display()
            ));
        }
        bail!(message)
    }

    /// Unique across every project, and across everything history ever held, so
    /// an id is never reused even after the item that held it is closed.
    pub fn mint_id(&self) -> Id {
        let mut taken: Vec<Id> = self
            .projects()
            .iter()
            .flat_map(|project| self.shelves(project))
            .flat_map(|(kind, shelf)| filenames(&shelf, kind))
            .map(|(id, _)| id)
            .collect();
        if let Some(repo) = git::Repo::open(&self.root)
            && let Ok(recorded) = repo.ids_ever()
        {
            taken.extend(recorded);
        }
        loop {
            let id = Id::mint();
            if !taken.contains(&id) {
                return id;
            }
        }
    }

    /// The readable half of a filename is the item's name, taken verbatim: the
    /// grammar admits nothing a filesystem minds. It is fixed at creation, so a
    /// rename never moves a file out from under an open session.
    fn item_path(&self, item: &Item, slug: &str) -> PathBuf {
        let dir = self.project_dir(&item.project).join(item.kind().dir());
        match item.kind() {
            Kind::Spec => dir.join(format!("{}-{}", item.id, slug)).join(SPEC_FILE),
            _ => dir.join(format!("{}-{}.md", item.id, slug)),
        }
    }

    /// Writes a brand new item and returns where its body should be authored.
    pub fn create(&self, item: &Item, body: &str) -> Result<PathBuf> {
        let _guard = self.lock(&item.project)?;
        let path = self.item_path(item, &item.name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic(&path, &render(item, body)?)?;
        Ok(path)
    }

    /// Rewrites frontmatter in canonical order while preserving the body byte
    /// for byte, relocating the item when its kind or its project changed.
    ///
    /// The slug is taken from where the file already sits and never from the
    /// item, so a rename never moves a file out from under an open session —
    /// which leaves the computed path differing from the current one exactly
    /// when a relocation is owed.
    pub fn save(&self, record: &Record, item: &Item) -> Result<PathBuf> {
        let _guard = self.lock(&item.project)?;
        let body = read_body(&record.path)?;
        let target = self.item_path(item, &existing_slug(&record.path, record.id));
        if target == record.path {
            write_atomic(&target, &render(item, &body)?)?;
            return Ok(target);
        }

        // The whole footprint moves, not the entry point alone: a spec's
        // directory carries supporting files that a file-by-file write would
        // orphan. The ladder runs forward, so a spec is a source only when a
        // move re-targets it, and a directory always lands as a directory.
        let from = self.footprint_at(record.kind, &record.path)?;
        if from.is_dir() {
            let to = self.footprint_at(item.kind(), &target)?;
            if let Some(parent) = to.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::rename(&from, &to)
                .with_context(|| format!("moving {} to {}", from.display(), to.display()))?;
            write_atomic(&target, &render(item, &body)?)?;
        } else {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            write_atomic(&target, &render(item, &body)?)?;
            remove_item(&record.path)?;
        }
        Ok(target)
    }

    /// Everything one item occupies: the file, or a spec's whole directory.
    /// Closing removes this and history is what keeps it; a move relocates it.
    pub fn footprint(&self, record: &Record) -> Result<PathBuf> {
        self.footprint_at(record.kind, &record.path)
    }

    fn footprint_at(&self, kind: Kind, path: &Path) -> Result<PathBuf> {
        match kind {
            Kind::Spec => path
                .parent()
                .map(Path::to_owned)
                .ok_or_else(|| anyhow!("{} has no directory", path.display())),
            _ => Ok(path.to_owned()),
        }
    }

    pub fn next_order(&self, project: &Path) -> i64 {
        self.list(project)
            .iter()
            .map(Record::order)
            .filter(|o| *o != i64::MAX)
            .max()
            .unwrap_or(0)
            + ORDER_STEP
    }

    /// Renumbers the whole project sparsely, so a later insertion rarely has to
    /// touch its neighbours.
    pub fn resequence(&self, project: &Path, ordered: &[Id]) -> Result<usize> {
        let _guard = self.lock(project)?;
        let records = self.list(project);
        let mut touched = 0;
        for (position, id) in ordered.iter().enumerate() {
            let Some(record) = records.iter().find(|r| r.id == *id) else {
                continue;
            };
            let Ok(item) = &record.item else { continue };
            let wanted = (position as i64 + 1) * ORDER_STEP;
            if item.order != wanted {
                let mut updated = item.clone();
                updated.order = wanted;
                let body = read_body(&record.path)?;
                write_atomic(&record.path, &render(&updated, &body)?)?;
                touched += 1;
            }
        }
        Ok(touched)
    }
}

/// The readable half of a filename: an item's name as it was written there.
/// The one place that knows how a filename is built, so it also answers for an
/// item whose metadata will not parse.
pub fn existing_slug(path: &Path, id: Id) -> String {
    let stem = if path.file_name().and_then(|n| n.to_str()) == Some(SPEC_FILE) {
        path.parent().and_then(|p| p.file_name())
    } else {
        path.file_stem()
    };
    stem.and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix(&format!("{id}-")))
        .unwrap_or("item")
        .to_owned()
}

/// Which items sit on one shelf, and where their files are — from directory
/// entries alone. Nothing is opened, so a scan across the whole depot costs one
/// readdir per shelf.
fn filenames(dir: &Path, kind: Kind) -> Vec<(Id, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut found = Vec::new();
    for entry in entries.flatten() {
        let path = match kind {
            Kind::Spec => entry.path().join(SPEC_FILE),
            _ => entry.path(),
        };
        if !path.is_file() {
            continue;
        }
        if kind != Kind::Spec && path.extension().and_then(|e| e.to_str()) != Some("md") {
            continue;
        }
        let name = entry.file_name();
        let Some(id) = name
            .to_str()
            .and_then(|n| n.split('-').next())
            .and_then(|n| n.parse::<Id>().ok())
        else {
            continue;
        };
        found.push((id, path));
    }
    found
}

fn collect(dir: &Path, kind: Kind, project: &Path, out: &mut Vec<Record>) {
    for (id, path) in filenames(dir, kind) {
        out.push(Record {
            id,
            kind,
            item: load(&path).map_err(|e| e.to_string()),
            path,
            project: project.to_owned(),
        });
    }
}

fn remove_item(path: &Path) -> Result<()> {
    if path.is_dir() {
        fs::remove_dir_all(path)?;
    } else if path.exists() {
        fs::remove_file(path)?;
    }
    Ok(())
}

/// Atomic replacement lives in `relic-core`: both stores wrote this by hand, and
/// both copies replaced the destination's extension to name their temporary.
fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    relic_core::fs::write_atomic(path, contents)
        .with_context(|| format!("replacing {}", path.display()))
}

/// Splits a document into its frontmatter and its body. The body is returned
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

pub fn load(path: &Path) -> Result<Item> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let (front, _) = split(&text)?;
    let wire: Wire = serde_yaml_ng::from_str(front).map_err(|e| {
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
    Item::try_from(wire)
}

/// The parser appends its own coordinates, which are relative to the
/// frontmatter rather than to the file. One location per message, and it should
/// be the one a reader can act on.
fn without_location(message: &str) -> &str {
    match message.rfind(" at line ") {
        Some(cut) => message[..cut].trim_end(),
        None => message,
    }
}

fn read_body(path: &Path) -> Result<String> {
    let text = fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    let (_, body) = split(&text)?;
    Ok(body.to_owned())
}

pub fn render(item: &Item, body: &str) -> Result<String> {
    let front = serde_yaml_ng::to_string(&Wire::from(item))?;
    Ok(format!("---\n{front}---\n{body}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_matches_the_claude_project_convention() {
        assert_eq!(
            slug_for_path(Path::new("/Users/example/.config")),
            "-Users-example--config"
        );
    }

    #[test]
    fn splitting_finds_the_body_untouched() {
        let text = "---\nid: b71c\n---\n# Title\n\nbody\n";
        let (front, body) = split(text).unwrap();
        assert_eq!(front, "id: b71c\n");
        assert_eq!(body, "# Title\n\nbody\n");
    }

    #[test]
    fn splitting_rejects_documents_without_frontmatter() {
        assert!(split("# no frontmatter\n").is_err());
        assert!(split("---\nid: b71c\n").is_err());
    }

    #[test]
    fn digests_differ_per_path() {
        assert_ne!(digest(Path::new("/a/b")), digest(Path::new("/a/c")));
        assert_eq!(digest(Path::new("/a/b")).len(), 4);
    }
}
