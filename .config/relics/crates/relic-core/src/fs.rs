//! Replacing a file's contents without ever exposing a partial one.
//!
//! `docket` and `midden` each wrote the same tmp-then-rename by hand, byte for
//! byte, which is what admitted this here. Both stores are read by a session hook
//! that can fire while a write is in flight, so a torn read is a real failure mode
//! and not a theoretical one.
//!
//! Three things the hand-rolled copies got wrong, fixed here once:
//!
//! - The temporary was `path.with_extension("tmp")`, which **replaces** the
//!   extension rather than appending to it. `a.md` and `a.json` therefore collide
//!   on `a.tmp`, and two writers to one path collide outright — each truncating the
//!   other's temporary before either renames.
//! - Nothing removed the temporary when the write failed, so every failed save left
//!   litter beside the file it failed to replace.
//! - `rename` was not followed by an fsync of the parent directory, so the
//!   directory entry could be lost to a crash even though the file's own bytes were
//!   synced — which is the one thing the whole dance is for.
//!
//! Every filesystem call goes through `fs_err`, so an error names the file it is
//! about. Bare `io::Error` says "permission denied" and leaves the reader to guess
//! which of the four paths in a write it meant.

use std::io::{self, Write};
use std::sync::atomic::{AtomicU64, Ordering};

use camino::{Utf8Path, Utf8PathBuf};
use fs_err as fs;
use fs_err::{File, OpenOptions};

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// Removes the temporary unless it has been disarmed, so no error path leaks one.
struct Scratch {
    path: Utf8PathBuf,
    armed: bool,
}

impl Scratch {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_file(&self.path);
        }
    }
}

/// Writes `contents` to `path`, atomically: a reader sees either the previous
/// contents or the new ones, never a prefix of either.
///
/// # Errors
///
/// Any [`io::Error`] the write, the rename or the temporary raises. It names the
/// path it is about; callers add the verb.
pub fn write_atomic(path: &Utf8Path, contents: &str) -> io::Result<()> {
    let dir = path.parent().unwrap_or(Utf8Path::new("."));
    let (mut scratch, mut file) = create_scratch(path, dir)?;

    file.write_all(contents.as_bytes())?;
    file.sync_all()?;
    drop(file);

    // The temporary is named after the destination, so the source of a rename is
    // always in the destination's own directory and the rename never crosses a
    // filesystem — the one condition under which it would stop being atomic.
    fs::rename(&scratch.path, path)?;
    scratch.disarm();

    // The bytes are durable and the entry is not until the directory is synced.
    // Not fatal on a directory that refuses to open for reading: the rename has
    // already happened, and reporting failure here would tell the caller nothing
    // was written when something was.
    if let Ok(handle) = File::open(dir) {
        let _ = handle.sync_all();
    }
    Ok(())
}

/// A temporary beside the destination, under a name nothing else can be using.
///
/// The leading dot keeps it out of any directory scan that globs or filters by
/// extension — both stores enumerate their own directories, and a temporary that
/// looks like a record is a record as far as they are concerned.
fn create_scratch(path: &Utf8Path, dir: &Utf8Path) -> io::Result<(Scratch, File)> {
    let stem = path
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "no file name to replace"))?;
    let pid = std::process::id();

    // The pid and counter make a collision possible only across processes that
    // reused a pid within one directory, so a bounded retry settles it. Unbounded
    // would turn a permission error into a spin.
    let mut last = None;
    for _ in 0..64 {
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let candidate = dir.join(format!(".{stem}.tmp.{pid}.{n}"));
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => {
                return Ok((
                    Scratch {
                        path: candidate,
                        armed: true,
                    },
                    file,
                ));
            }
            // Taken since the name was composed: the next one is free.
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap_or_else(|| {
        io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no free temporary name beside the destination",
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Barrier;

    fn scratch_dir(name: &str) -> Utf8PathBuf {
        let dir = crate::path::utf8(std::env::temp_dir())
            .expect("a temporary directory this program can name")
            .join(format!("relic-core-fs-{}-{}", name, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        dir
    }

    fn leftovers(dir: &Utf8Path) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(dir)
            .expect("read scratch dir")
            .map(|e| e.expect("entry").file_name().to_string_lossy().into_owned())
            .filter(|n| n.contains(".tmp."))
            .collect();
        names.sort();
        names
    }

    #[test]
    fn replaces_contents_and_leaves_no_temporary() {
        let dir = scratch_dir("replace");
        let path = dir.join("note.md");
        write_atomic(&path, "first").expect("first write");
        write_atomic(&path, "second").expect("second write");
        assert_eq!(fs::read_to_string(&path).expect("read"), "second");
        assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_failed_write_leaves_no_temporary() {
        let dir = scratch_dir("failure");
        // A destination inside a directory that does not exist: the temporary is
        // never created, and a destination that is itself a directory fails at the
        // rename with a temporary already on disk.
        let occupied = dir.join("occupied");
        fs::create_dir(&occupied).expect("make a directory in the way");
        assert!(write_atomic(&occupied, "payload").is_err());
        assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_shared_stem_does_not_collide() {
        // `with_extension("tmp")` gave both of these the same temporary.
        let dir = scratch_dir("stem");
        let md = dir.join("a.md");
        let json = dir.join("a.json");
        let barrier = Barrier::new(2);
        std::thread::scope(|s| {
            s.spawn(|| {
                barrier.wait();
                for _ in 0..40 {
                    write_atomic(&md, "MARKDOWN").expect("md");
                }
            });
            s.spawn(|| {
                barrier.wait();
                for _ in 0..40 {
                    write_atomic(&json, "JSON").expect("json");
                }
            });
        });
        assert_eq!(fs::read_to_string(&md).expect("read md"), "MARKDOWN");
        assert_eq!(fs::read_to_string(&json).expect("read json"), "JSON");
        assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn concurrent_writers_never_yield_a_torn_file() {
        let dir = scratch_dir("torn");
        let path = dir.join("contested.md");
        let long = "A".repeat(200_000);
        let short = "B".repeat(200_000);
        let barrier = Barrier::new(3);
        std::thread::scope(|s| {
            s.spawn(|| {
                barrier.wait();
                for _ in 0..20 {
                    write_atomic(&path, &long).expect("writer a");
                }
            });
            s.spawn(|| {
                barrier.wait();
                for _ in 0..20 {
                    write_atomic(&path, &short).expect("writer b");
                }
            });
            s.spawn(|| {
                barrier.wait();
                for _ in 0..60 {
                    if let Ok(seen) = fs::read_to_string(&path) {
                        assert!(
                            seen == long || seen == short,
                            "read a partial file of {} bytes",
                            seen.len()
                        );
                    }
                }
            });
        });
        assert!(leftovers(&dir).is_empty(), "{:?}", leftovers(&dir));
        fs::remove_dir_all(&dir).ok();
    }
}
