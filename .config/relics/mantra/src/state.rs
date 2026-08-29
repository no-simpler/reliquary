//! What a session remembers about its own modes.
//!
//! The context can be summarised away; this cannot. That asymmetry is the whole
//! reason the file exists: after a compaction nothing in the model's context
//! records that a mode was ever switched on, so something outside it has to.
//!
//! **State is a cache; the transcript is the record.** A session id keys one
//! file, and Claude Code keeps that id across a compaction — but not across a
//! fork, and a file can be deleted or corrupted. So every path that finds no
//! usable state rebuilds it by re-reading the `+token` activations out of the
//! transcript, which is where they were written down in the user's own words and
//! where they stay. Losing this file costs a rebuild, never a mode.
//!
//! Machine-local and untracked, beside the other per-machine agentic stores.

use camino::{Utf8Path, Utf8PathBuf};
use fs_err as fs;
use serde::{Deserialize, Serialize};

use anyhow::{Result, anyhow};

/// How long a session's state outlives its last write. Long enough to resume a
/// session after a weekend, short enough that the directory does not accumulate
/// a file per session forever.
pub const MAX_AGE_DAYS: u64 = 30;

/// The longest session id worth believing. Ids are UUIDs; the bound is here
/// because the id becomes a file name.
const MAX_ID: usize = 128;

/// `~/.claude/mantra`, or wherever `MANTRA_ROOT` points. The override exists so
/// tests and trial runs never touch live state.
pub fn root() -> Result<Utf8PathBuf> {
    if let Some(root) = std::env::var_os("MANTRA_ROOT") {
        return Ok(relic_core::path::utf8(root.into())?);
    }
    let home = relic_core::path::home().ok_or_else(|| anyhow!("HOME is unset or not UTF-8"))?;
    Ok(home.join(".claude").join("mantra"))
}

/// Whether an id may be used as a file name. A hook payload is data, and a
/// session id that is a path is the one way this could write outside its own
/// directory.
pub fn is_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_ID
        && id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// One session's modes and marks.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Session {
    /// Incremented by every compaction, so a listing can say which one it is.
    #[serde(default)]
    pub generation: u32,
    /// Prompts seen.
    #[serde(default)]
    pub turns: u64,
    /// The window size when this was last written.
    #[serde(default)]
    pub tokens: u64,
    /// Every mode switched on, in activation order.
    #[serde(default)]
    pub modes: Vec<Active>,
}

/// One mode's standing in one session.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Active {
    /// The `+token`.
    pub name: String,
    /// The window size when it was switched on.
    #[serde(default)]
    pub activated_at: u64,
    /// The window size when it was last said. Equal to `activated_at` until it
    /// has been.
    #[serde(default)]
    pub last_fired_at: u64,
    /// How many times it has been said. Zero means the body is still owed.
    #[serde(default)]
    pub fires: u32,
    /// The `when` marks already crossed, so an edge is consumed once and a
    /// compaction cannot re-arm it.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub latched: Vec<u64>,
}

impl Active {
    /// A mode switched on at `tokens`, owing its body.
    pub fn new(name: String, tokens: u64) -> Self {
        Self {
            name,
            activated_at: tokens,
            last_fired_at: tokens,
            fires: 0,
            latched: Vec::new(),
        }
    }
}

impl Session {
    /// Whether `name` is already switched on. Activating twice is one activation
    /// — the repetition that motivated the schedule, not a second mode.
    pub fn holds(&self, name: &str) -> bool {
        self.modes.iter().any(|m| m.name == name)
    }
}

/// The file one session's state lives in.
pub fn path(root: &Utf8Path, id: &str) -> Utf8PathBuf {
    root.join(format!("{id}.json"))
}

/// The lock beside it.
fn lock_path(root: &Utf8Path, id: &str) -> Utf8PathBuf {
    root.join(format!("{id}.lock"))
}

/// Reads one session's state. Absent and unreadable are the same answer on
/// purpose: both mean "rebuild rather than guess", and neither may fail a hook.
pub fn load(root: &Utf8Path, id: &str) -> Option<Session> {
    let text = fs::read_to_string(path(root, id)).ok()?;
    serde_json::from_str(&text).ok()
}

/// Replaces one session's state, under a lock nothing waits on.
///
/// # Errors
///
/// When the directory cannot be made, the lock is held, or the write fails. On a
/// hook path every one of those means "skip this injection", never "fail".
pub fn save(root: &Utf8Path, id: &str, session: &Session) -> Result<()> {
    fs::create_dir_all(root)?;
    // Bounded to nothing: two hooks in one session overlap rarely, and the next
    // boundary is a better answer than a hook that waits.
    let _guard = relic_core::lock::Lock::try_acquire(&lock_path(root, id))?;
    let text = serde_json::to_string(session)?;
    relic_core::fs::write_atomic(&path(root, id), &text)?;
    Ok(())
}

/// Forgets one session. What `/clear` means: the context is gone, so the modes
/// that were spoken into it are too.
pub fn remove(root: &Utf8Path, id: &str) {
    let _ = fs::remove_file(path(root, id));
    let _ = fs::remove_file(lock_path(root, id));
}

/// How many sessions have state at all.
pub fn count(root: &Utf8Path) -> usize {
    sessions(root).len()
}

/// The sessions nothing has written to in `max_age_days`, without removing
/// them, so `gc -n` reports exactly what `gc` would take.
///
/// # Errors
///
/// When the directory exists and cannot be read.
pub fn stale(root: &Utf8Path, max_age_days: u64) -> Result<Vec<String>> {
    let cutoff = cutoff(max_age_days)?;
    Ok(sessions(root)
        .into_iter()
        .filter(|(_, at)| *at < cutoff)
        .map(|(id, _)| id)
        .collect())
}

/// Every session with state, and when it was last written.
fn sessions(root: &Utf8Path) -> Vec<(String, std::time::SystemTime)> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut found: Vec<(String, std::time::SystemTime)> = entries
        .flatten()
        .filter_map(|entry| {
            let path = relic_core::path::utf8(entry.path()).ok()?;
            if path.extension() != Some("json") {
                return None;
            }
            let at = entry.metadata().and_then(|m| m.modified()).ok()?;
            Some((path.file_stem()?.to_owned(), at))
        })
        .collect();
    found.sort();
    found
}

fn cutoff(max_age_days: u64) -> Result<std::time::SystemTime> {
    std::time::SystemTime::now()
        .checked_sub(std::time::Duration::from_secs(max_age_days * 24 * 60 * 60))
        .ok_or_else(|| anyhow!("a cutoff that far back is not a time"))
}

/// Removes state for sessions nothing has written to in `max_age_days`, and
/// returns how many went. A session that is still live rewrites its file every
/// turn, so age is the whole test.
///
/// # Errors
///
/// When the directory cannot be read. An absent directory is not an error —
/// there is nothing to sweep.
pub fn gc(root: &Utf8Path, max_age_days: u64) -> Result<usize> {
    if !root.is_dir() {
        return Ok(0);
    }
    let doomed = stale(root, max_age_days)?;
    for id in &doomed {
        remove(root, id);
    }
    Ok(doomed.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch() -> (tempfile::TempDir, Utf8PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let root = relic_core::path::utf8(dir.path().join("mantra")).expect("utf-8");
        (dir, root)
    }

    fn session() -> Session {
        Session {
            generation: 1,
            turns: 4,
            tokens: 120_000,
            modes: vec![Active {
                name: "terse".to_owned(),
                activated_at: 8_000,
                last_fired_at: 100_000,
                fires: 3,
                latched: vec![500_000],
            }],
        }
    }

    #[test]
    fn a_session_round_trips() {
        let (_dir, root) = scratch();
        save(&root, "abc-123", &session()).expect("save");
        assert_eq!(load(&root, "abc-123"), Some(session()));
    }

    #[test]
    fn absent_and_corrupt_read_the_same() {
        let (_dir, root) = scratch();
        assert_eq!(load(&root, "absent"), None);
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(path(&root, "torn"), "{not json").expect("write");
        assert_eq!(load(&root, "torn"), None);
    }

    #[test]
    fn an_unknown_key_is_not_silently_carried() {
        let (_dir, root) = scratch();
        fs::create_dir_all(&root).expect("mkdir");
        fs::write(path(&root, "old"), r#"{"generation":1,"pace":9}"#).expect("write");
        assert_eq!(load(&root, "old"), None);
    }

    #[test]
    fn removing_takes_the_lock_with_it() {
        let (_dir, root) = scratch();
        save(&root, "gone", &session()).expect("save");
        assert!(lock_path(&root, "gone").is_file());
        remove(&root, "gone");
        assert!(!path(&root, "gone").exists());
        assert!(!lock_path(&root, "gone").exists());
    }

    #[test]
    fn only_a_name_that_cannot_escape_the_directory_is_an_id() {
        assert!(is_id("4b072797-f8b1-4a19-8424-a0687629cbd7"));
        assert!(is_id("a_b-1"));
        assert!(!is_id(""));
        assert!(!is_id("../elsewhere"));
        assert!(!is_id("a/b"));
        assert!(!is_id("a.b"));
        assert!(!is_id(&"x".repeat(MAX_ID + 1)));
    }

    #[test]
    fn sweeping_an_absent_directory_is_not_a_failure() {
        let (_dir, root) = scratch();
        assert_eq!(gc(&root, MAX_AGE_DAYS).expect("gc"), 0);
    }

    #[test]
    fn a_fresh_session_survives_a_sweep() {
        let (_dir, root) = scratch();
        save(&root, "live", &session()).expect("save");
        assert_eq!(gc(&root, MAX_AGE_DAYS).expect("gc"), 0);
        assert!(load(&root, "live").is_some());
    }

    #[test]
    fn a_stale_session_is_swept() {
        let (_dir, root) = scratch();
        save(&root, "stale", &session()).expect("save");
        // Zero days makes everything already written older than the cutoff.
        assert_eq!(gc(&root, 0).expect("gc"), 1);
        assert_eq!(load(&root, "stale"), None);
    }

    #[test]
    fn a_mode_is_switched_on_once() {
        let mut s = Session::default();
        s.modes.push(Active::new("terse".to_owned(), 10));
        assert!(s.holds("terse"));
        assert!(!s.holds("slash"));
    }
}
