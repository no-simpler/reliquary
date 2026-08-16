use std::fs::{self, File};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};

use crate::id::{Id, slugify};
use crate::item::{Item, Kind, Wire};

const SENTINEL: &str = ".project";
const LOCK: &str = ".lock";
const ARCHIVE: &str = "archive";
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

/// Absolute and symlink-free as far as the path exists, lexical the rest of the
/// way, so a target that has not been created yet keys the same as it will once
/// it is.
pub fn resolve_lenient(path: &Path) -> PathBuf {
    let expanded = expand_tilde(path);
    let absolute = if expanded.is_absolute() {
        expanded
    } else {
        std::env::current_dir().unwrap_or_default().join(expanded)
    };

    let mut real = PathBuf::new();
    let mut tail = PathBuf::new();
    for component in absolute.components() {
        match component {
            Component::CurDir => continue,
            Component::ParentDir if tail.as_os_str().is_empty() => {
                real.pop();
            }
            other => {
                if tail.as_os_str().is_empty() {
                    let candidate = real.join(other);
                    match candidate.canonicalize() {
                        Ok(resolved) => real = resolved,
                        Err(_) => tail.push(other),
                    }
                } else {
                    tail.push(other);
                }
            }
        }
    }
    // Joining an empty tail would append a separator, and a trailing separator
    // makes a different slug for the same directory.
    if tail.as_os_str().is_empty() {
        real
    } else {
        real.join(tail)
    }
}

fn expand_tilde(path: &Path) -> PathBuf {
    let text = path.to_string_lossy();
    let Some(rest) = text.strip_prefix('~') else {
        return path.to_owned();
    };
    if !(rest.is_empty() || rest.starts_with('/')) {
        return path.to_owned();
    }
    let Some(home) = std::env::var_os("HOME") else {
        return path.to_owned();
    };
    PathBuf::from(home).join(rest.trim_start_matches('/'))
}

/// The main checkout root of the containing repository, or the working
/// directory when there is none. Linked worktrees fold into their main
/// checkout, because `git worktree list` reports it first; a submodule reports
/// its own root, which is what a per-aspect repository layout needs.
pub fn project_key(cwd: &Path) -> PathBuf {
    let output = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["worktree", "list", "--porcelain"])
        .output();

    if let Ok(output) = output
        && output.status.success()
        && let Ok(text) = String::from_utf8(output.stdout)
        && let Some(main) = text
            .lines()
            .next()
            .and_then(|line| line.strip_prefix("worktree "))
        && !main.is_empty()
    {
        return resolve_lenient(Path::new(main));
    }
    resolve_lenient(cwd)
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
    fn lock(&self, project: &Path) -> Result<File> {
        let dir = self.ensure_project(project)?;
        let file = File::create(dir.join(LOCK))?;
        file.lock()?;
        Ok(file)
    }

    /// Every project that has a docket directory, live or archived.
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

    pub fn list(&self, project: &Path, archived: bool) -> Vec<Record> {
        let dir = self.project_dir(project);
        let base = if archived { dir.join(ARCHIVE) } else { dir };
        let mut records = Vec::new();
        for kind in Kind::ALL {
            collect(&base.join(kind.dir()), kind, project, &mut records);
        }
        records.sort_by(|a, b| a.order().cmp(&b.order()).then_with(|| a.id.cmp(&b.id)));
        records
    }

    /// Every place an item can sit, live and archived, for the scans that only
    /// need filenames.
    fn shelves(&self, project: &Path) -> Vec<(Kind, PathBuf)> {
        let dir = self.project_dir(project);
        let archive = dir.join(ARCHIVE);
        Kind::ALL
            .into_iter()
            .flat_map(|kind| {
                [
                    (kind, dir.join(kind.dir())),
                    (kind, archive.join(kind.dir())),
                ]
            })
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
        bail!("no item with id {id}. Run docket list --all to see every open item")
    }

    /// Unique across every project and every archive, so an id is never reused
    /// even after the item that held it is gone.
    pub fn mint_id(&self) -> Id {
        let taken: Vec<Id> = self
            .projects()
            .iter()
            .flat_map(|project| self.shelves(project))
            .flat_map(|(kind, shelf)| filenames(&shelf, kind))
            .map(|(id, _)| id)
            .collect();
        loop {
            let id = Id::mint();
            if !taken.contains(&id) {
                return id;
            }
        }
    }

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
        let path = self.item_path(item, &slugify(&item.title));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_atomic(&path, &render(item, body)?)?;
        Ok(path)
    }

    /// Rewrites frontmatter in canonical order while preserving the body byte
    /// for byte, and moves the file when a promotion changed its kind.
    pub fn save(&self, record: &Record, item: &Item) -> Result<PathBuf> {
        let _guard = self.lock(&item.project)?;
        let body = read_body(&record.path)?;
        let target = if record.kind == item.kind() {
            record.path.clone()
        } else {
            self.item_path(item, &existing_slug(&record.path, record.id))
        };

        if target != record.path {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            write_atomic(&target, &render(item, &body)?)?;
            remove_item(&record.path)?;
        } else {
            write_atomic(&target, &render(item, &body)?)?;
        }
        Ok(target)
    }

    /// Archive rather than delete: a handoff is the one artefact here that
    /// nothing else can regenerate.
    pub fn archive(&self, record: &Record) -> Result<PathBuf> {
        let _guard = self.lock(&record.project)?;
        let dir = self
            .project_dir(&record.project)
            .join(ARCHIVE)
            .join(record.kind.dir());
        fs::create_dir_all(&dir)?;

        let (source, target) = match record.kind {
            Kind::Spec => {
                let spec_dir = record
                    .path
                    .parent()
                    .ok_or_else(|| anyhow!("spec has no directory"))?
                    .to_owned();
                let name = spec_dir
                    .file_name()
                    .ok_or_else(|| anyhow!("spec directory has no name"))?;
                (spec_dir.clone(), dir.join(name))
            }
            _ => {
                let name = record
                    .path
                    .file_name()
                    .ok_or_else(|| anyhow!("item has no filename"))?;
                (record.path.clone(), dir.join(name))
            }
        };
        if target.exists() {
            remove_item(&target)?;
        }
        fs::rename(&source, &target).with_context(|| format!("archiving {}", source.display()))?;
        Ok(target)
    }

    pub fn next_order(&self, project: &Path) -> i64 {
        self.list(project, false)
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
        let records = self.list(project, false);
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

fn existing_slug(path: &Path, id: Id) -> String {
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

fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let temp = path.with_extension("tmp");
    {
        let mut file =
            File::create(&temp).with_context(|| format!("writing {}", temp.display()))?;
        file.write_all(contents.as_bytes())?;
        file.sync_all()?;
    }
    fs::rename(&temp, path).with_context(|| format!("replacing {}", path.display()))?;
    Ok(())
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
    fn lenient_resolution_keeps_the_missing_tail() {
        let resolved = resolve_lenient(Path::new("/tmp/definitely-absent-xyz/deeper"));
        assert!(resolved.is_absolute());
        assert!(resolved.ends_with("definitely-absent-xyz/deeper"));
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
