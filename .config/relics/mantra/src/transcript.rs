//! How full the context window is, read from the session's own transcript.
//!
//! Claude Code hands every hook a `transcript_path`, and the transcript records
//! what each request actually cost. This is the same computation Claude Code
//! performs on itself — the one behind auto-compaction and behind the
//! `context_tokens` it volunteers on a resumed session — reproduced here because
//! a hook is given the path and not the number.
//!
//! Three parts of it are easy to get wrong, and all three are load-bearing:
//!
//! - **Never sum across lines.** Each assistant message's `usage` already
//!   describes the whole window at that moment; adding them together
//!   double-counts every cached token in the session, which is nearly all of
//!   them.
//! - **Stop at the last compaction.** Everything before a `compact_boundary`
//!   was dropped, so a usage record from before it describes a window that no
//!   longer exists.
//! - **Take the last `iterations` entry when there is one.** A server-side tool
//!   loop reports its whole loop in the outer totals and each pass separately;
//!   the window is the last pass, not the sum.
//!
//! Sidechains are excluded because a subagent's window is not this one, and
//! `<synthetic>` messages because they are Claude Code talking to itself and
//! carry no real usage.
//!
//! The read is bounded and backwards: the answer is almost always in the last
//! few lines, so it starts with a small tail and widens only if it has to. That
//! matters because this runs on a tool-batch hook, against a file that reaches
//! tens of megabytes.

use std::io::{Read, Seek, SeekFrom};

use camino::Utf8Path;
use fs_err::File;
use serde::Deserialize;

/// The first tail read. Comfortably more than the handful of lines that answer
/// the question in a session whose last turn was ordinary.
const FIRST_WINDOW: u64 = 256 * 1024;

/// The widest tail worth reading. Past this the transcript is pathological —
/// one turn of tool output larger than eight megabytes — and answering `None`
/// costs a delayed refresh rather than a wrong one.
const MAX_WINDOW: u64 = 8 * 1024 * 1024;

/// One transcript line, in the fields that bear on the window size.
#[derive(Deserialize)]
struct Line {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    subtype: String,
    #[serde(rename = "isSidechain", default)]
    is_sidechain: bool,
    #[serde(default)]
    message: Option<Message>,
    #[serde(rename = "compactMetadata", default)]
    compact_metadata: Option<CompactMetadata>,
}

#[derive(Deserialize)]
struct Message {
    #[serde(default)]
    model: String,
    #[serde(default)]
    usage: Option<Usage>,
}

#[derive(Default, Deserialize)]
struct Usage {
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
    #[serde(default)]
    iterations: Vec<Iteration>,
}

#[derive(Deserialize)]
struct Iteration {
    #[serde(rename = "type", default)]
    kind: String,
    #[serde(default)]
    input_tokens: u64,
    #[serde(default)]
    cache_creation_input_tokens: u64,
    #[serde(default)]
    cache_read_input_tokens: u64,
    #[serde(default)]
    output_tokens: u64,
}

#[derive(Deserialize)]
struct CompactMetadata {
    #[serde(rename = "postTokens", default)]
    post_tokens: Option<u64>,
}

/// The window size at the end of `path`, or nothing when the transcript cannot
/// answer — it is unreadable, still empty, or has no usage record inside the
/// widest tail worth reading.
pub fn context_tokens(path: &Utf8Path) -> Option<u64> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if len == 0 {
        return None;
    }
    let mut window = FIRST_WINDOW;
    loop {
        let start = len.saturating_sub(window);
        let text = read_at(&mut file, start, len)?;
        // Only a read that reached the start of the file has seen every line,
        // so only that read may conclude there is nothing to find.
        let whole = start == 0;
        if let Some(found) = scan(&text, whole) {
            return Some(found);
        }
        if whole || window >= MAX_WINDOW {
            return None;
        }
        window = window.saturating_mul(4);
    }
}

/// `start..len` of `file`, decoded leniently: the window begins mid-line and can
/// begin mid-character, and that first line is discarded either way.
fn read_at(file: &mut File, start: u64, len: u64) -> Option<String> {
    file.seek(SeekFrom::Start(start)).ok()?;
    let size = usize::try_from(len - start).ok()?;
    let mut bytes = vec![0_u8; size];
    file.read_exact(&mut bytes).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// The window size from a tail of the transcript, scanning backwards. `whole`
/// says the tail starts at the beginning of the file, which is what makes the
/// first line usable.
fn scan(text: &str, whole: bool) -> Option<u64> {
    let mut lines: Vec<&str> = text.lines().collect();
    if !whole && !lines.is_empty() {
        lines.remove(0);
    }
    for line in lines.iter().rev() {
        // Parsing a transcript line means parsing a whole tool result, so the
        // cheap test comes first: neither answer can come from a line that
        // mentions neither.
        if !line.contains("\"usage\"") && !line.contains("compact_boundary") {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Line>(line) else {
            continue;
        };
        if parsed.kind == "system" && parsed.subtype == "compact_boundary" {
            // Everything before this is gone. What survived it is the summary,
            // whose size compaction recorded.
            return Some(
                parsed
                    .compact_metadata
                    .and_then(|meta| meta.post_tokens)
                    .unwrap_or(0),
            );
        }
        if parsed.kind != "assistant" || parsed.is_sidechain {
            continue;
        }
        let Some(message) = parsed.message else {
            continue;
        };
        if message.model == "<synthetic>" {
            continue;
        }
        let Some(usage) = message.usage else {
            continue;
        };
        let found = window_of(&usage);
        if found > 0 {
            return Some(found);
        }
    }
    None
}

/// The window one usage record describes.
fn window_of(usage: &Usage) -> u64 {
    let total = usage.input_tokens
        + usage.cache_creation_input_tokens
        + usage.cache_read_input_tokens
        + usage.output_tokens;
    if total == 0 {
        return 0;
    }
    match usage
        .iterations
        .iter()
        .rev()
        .find(|it| it.kind == "message" || it.kind == "fallback_message")
    {
        Some(last) => {
            last.input_tokens
                + last.cache_creation_input_tokens
                + last.cache_read_input_tokens
                + last.output_tokens
        }
        None => total,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fs_err as fs;

    fn assistant(read: u64, out: u64) -> String {
        format!(
            r#"{{"type":"assistant","message":{{"model":"claude-opus-5","content":[],"usage":{{"input_tokens":2,"cache_creation_input_tokens":0,"cache_read_input_tokens":{read},"output_tokens":{out}}}}}}}"#
        )
    }

    fn transcript(lines: &[String]) -> (tempfile::TempDir, camino::Utf8PathBuf) {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let path = relic_core::path::utf8(dir.path().join("t.jsonl")).expect("utf-8");
        fs::write(&path, format!("{}\n", lines.join("\n"))).expect("write");
        (dir, path)
    }

    #[test]
    fn the_last_assistant_line_answers() {
        let (_dir, path) = transcript(&[assistant(100, 5), assistant(664_298, 330)]);
        assert_eq!(context_tokens(&path), Some(664_630));
    }

    #[test]
    fn usage_is_never_summed_across_lines() {
        let (_dir, path) = transcript(&[assistant(1_000, 0), assistant(1_000, 0)]);
        assert_eq!(context_tokens(&path), Some(1_002));
    }

    #[test]
    fn a_sidechain_turn_is_not_this_window() {
        let mut sidechain = assistant(999_999, 0);
        sidechain = sidechain.replace(
            r#""type":"assistant""#,
            r#""type":"assistant","isSidechain":true"#,
        );
        let (_dir, path) = transcript(&[assistant(500, 1), sidechain]);
        assert_eq!(context_tokens(&path), Some(503));
    }

    #[test]
    fn a_synthetic_message_carries_no_usage_worth_reading() {
        let synthetic = assistant(999_999, 0).replace("claude-opus-5", "<synthetic>");
        let (_dir, path) = transcript(&[assistant(500, 1), synthetic]);
        assert_eq!(context_tokens(&path), Some(503));
    }

    #[test]
    fn a_server_tool_loop_reports_its_last_pass() {
        let line = r#"{"type":"assistant","message":{"model":"claude-opus-5","usage":{"input_tokens":10,"cache_read_input_tokens":90,"output_tokens":0,"iterations":[{"type":"message","input_tokens":1,"cache_read_input_tokens":9,"output_tokens":0},{"type":"message","input_tokens":2,"cache_read_input_tokens":40,"output_tokens":8}]}}}"#;
        let (_dir, path) = transcript(&[line.to_owned()]);
        assert_eq!(context_tokens(&path), Some(50));
    }

    #[test]
    fn a_compaction_hides_everything_before_it() {
        let boundary = r#"{"type":"system","subtype":"compact_boundary","compactMetadata":{"preTokens":399827,"postTokens":9958}}"#;
        let (_dir, path) = transcript(&[assistant(399_000, 800), boundary.to_owned()]);
        assert_eq!(context_tokens(&path), Some(9_958));
    }

    #[test]
    fn a_turn_after_a_compaction_answers_over_it() {
        let boundary = r#"{"type":"system","subtype":"compact_boundary","compactMetadata":{"postTokens":9958}}"#;
        let (_dir, path) = transcript(&[
            assistant(399_000, 800),
            boundary.to_owned(),
            assistant(12_000, 40),
        ]);
        assert_eq!(context_tokens(&path), Some(12_042));
    }

    #[test]
    fn an_empty_or_missing_transcript_answers_nothing() {
        let dir = tempfile::tempdir().expect("a temporary directory");
        let missing = relic_core::path::utf8(dir.path().join("absent.jsonl")).expect("utf-8");
        assert_eq!(context_tokens(&missing), None);
        let (_dir, path) = transcript(&[]);
        fs::write(&path, "").expect("truncate");
        assert_eq!(context_tokens(&path), None);
    }

    #[test]
    fn a_transcript_of_only_user_lines_answers_nothing() {
        let (_dir, path) = transcript(&[r#"{"type":"user","message":"hello"}"#.to_owned()]);
        assert_eq!(context_tokens(&path), None);
    }

    #[test]
    fn a_torn_last_line_does_not_answer_wrongly() {
        let (_dir, path) = transcript(&[assistant(500, 1)]);
        let mut text = fs::read_to_string(&path).expect("read");
        text.push_str(r#"{"type":"assistant","message":{"model":"claude"#);
        fs::write(&path, text).expect("write");
        assert_eq!(context_tokens(&path), Some(503));
    }

    #[test]
    fn the_window_widens_past_a_very_long_turn() {
        // One line larger than the first read, so the answer is only reachable
        // by widening.
        let padding = "x".repeat(usize::try_from(FIRST_WINDOW).expect("fits") + 1_000);
        let bulky = format!(r#"{{"type":"user","message":{{"content":"{padding}"}}}}"#);
        let (_dir, path) = transcript(&[assistant(700, 3), bulky]);
        assert_eq!(context_tokens(&path), Some(705));
    }
}
