//! What one shell has already been shown.
//!
//! State is per shell, so opening a second terminal shows the card there too —
//! and neither shell repeats it until the outstanding set actually moves.
//!
//! The session id comes from the hook, which mints one per shell. A shell that
//! sets none is treated as a brand new session every time, which is the right
//! answer for `coop` run by hand.

use anyhow::Result;
use jiff::Timestamp;

use crate::paths::Paths;

/// How long a shell's mark outlives its last prompt before it is swept.
const KEEP_DAYS: i64 = 7;

/// A session id reduced to something that can safely be a filename.
///
/// The value arrives from the environment, so it is untrusted by construction:
/// a `../` in it would otherwise choose the file that gets written.
pub fn sanitize(raw: &str) -> Option<String> {
    let cleaned: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .take(64)
        .collect();
    (!cleaned.is_empty()).then_some(cleaned)
}

/// The digest this shell was last shown.
pub fn seen(paths: &Paths, session: &str) -> Option<String> {
    let path = paths.seen(session);
    fs_err::read_to_string(path)
        .ok()
        .map(|text| text.trim().to_owned())
}

/// Record what this shell has now been shown.
///
/// # Errors
///
/// When the mark cannot be written.
pub fn mark(paths: &Paths, session: &str, digest: &str) -> Result<()> {
    let path = paths.seen(session);
    if let Some(parent) = path.parent() {
        fs_err::create_dir_all(parent)?;
    }
    relic_core::fs::write_atomic(&path, digest)?;
    Ok(())
}

/// Drop marks left by shells that are gone.
///
/// Runs from the refresh path rather than the tick: a sweep is a directory walk,
/// and the prompt is not where directory walks belong.
///
/// # Errors
///
/// Never — a sweep that cannot read its own directory has nothing to sweep.
pub fn sweep(paths: &Paths, now: Timestamp) -> Result<()> {
    let dir = paths.seen_dir();
    let Ok(entries) = fs_err::read_dir(&dir) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if is_stale(&path, now) {
            let _ = fs_err::remove_file(&path);
        }
    }
    Ok(())
}

fn is_stale(path: &std::path::Path, now: Timestamp) -> bool {
    let Ok(metadata) = fs_err::metadata(path) else {
        return false;
    };
    let Ok(modified) = metadata.modified() else {
        return false;
    };
    let Ok(since) = modified.duration_since(std::time::UNIX_EPOCH) else {
        return false;
    };
    let Ok(seconds) = i64::try_from(since.as_secs()) else {
        return false;
    };
    let Ok(at) = Timestamp::from_second(seconds) else {
        return false;
    };
    relic_core::fmt::age_days(at, now) > KEEP_DAYS
}

/// The session id this process was told about, already made safe.
pub fn from_env() -> Option<String> {
    std::env::var(crate::paths::SESSION_VAR)
        .ok()
        .as_deref()
        .and_then(sanitize)
}

#[cfg(test)]
mod tests {
    use super::sanitize;

    #[test]
    fn a_traversal_cannot_choose_the_file_that_gets_written() {
        assert_eq!(sanitize("../../etc/passwd"), Some("etcpasswd".to_owned()));
        assert_eq!(sanitize("/"), None);
    }

    #[test]
    fn an_ordinary_id_survives_intact() {
        assert_eq!(sanitize("48213-9917"), Some("48213-9917".to_owned()));
    }

    #[test]
    fn an_absurd_id_is_bounded() {
        assert_eq!(sanitize(&"a".repeat(500)).unwrap().len(), 64);
    }

    #[test]
    fn an_empty_id_is_no_id() {
        assert_eq!(sanitize(""), None);
        assert_eq!(sanitize("!!!"), None);
    }
}
