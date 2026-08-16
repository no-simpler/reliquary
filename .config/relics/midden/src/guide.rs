use anyhow::{Result, bail};

/// Canonical order, so a guide reads the same whatever order its topics were
/// asked for.
pub const TOPICS: &[(&str, &str)] = &[("kinds", KINDS), ("filing", FILING), ("draining", DRAINING)];

pub const INTRO: &str = "\
MIDDEN

Machine-wide corpus of what the harness cost an agent. One note is one cause,
filed while the evidence is still in front of you.

Most sessions produce none. That is the expected outcome, not a failure to
notice: a note nobody can act on dilutes the ones that matter, and a heap that
stops being worth reading stops being read.";

// Deliberately not opened with a `\` continuation: that would swallow this
// block's leading indentation along with the newline.
pub const USAGE: &str = "  midden note --kind <kind> --title '...' --target <path>   files one
  midden                                                    open notes
  midden digest                                             grouped by fix site
  midden resolve <id> --actioned|--dismissed                closes one

  midden guide kinds|filing|draining                        doctrine
  midden help                                               CLI usage";

pub const KINDS: &str = "\
KINDS

Chosen before the title is written, because each one names where its fix lives.
A cause fitting none of them is usually not understood yet.

  gap        Nothing covered the situation. You guessed.
  conflict   Two directives pulled opposite ways.
  stale      A directive named a path, flag or symbol that no longer resolves.
  hunt       A fact that should have been declared cost repeated searching.
  rebuff     The user corrected or rejected you.
  friction   Harness or tooling: a permission prompt, a missing capability.
  rework     Work was done, then discarded.";

pub const FILING: &str = "\
FILING

File at the moment it happens, not at the end. A long session compacts, and
what compaction takes is exactly the early friction worth reporting. Anything
before a compact boundary is not yours to reconstruct — say it was lost.

  Evidence or nothing. The body quotes what happened. No quote, no note.
  Name the cause, not the feeling. Not confusing: unstated, contradicted, moved.
  One note per cause. Two causes in one note cannot be resolved separately.
  Target the fix, not the symptom. The file that should have said it.

For a rebuff, the title names the directive that would have prevented it, not
what you were asked to do differently.

Do not look for an existing note first. Filing the same kind, target and claim
again folds into the one already there and bumps its count.";

pub const DRAINING: &str = "\
DRAINING

For the session that acts on the heap, not the one that fills it.

  midden digest groups open notes by where their fix would land.
  Work a group at a time: one section is one file to open.
  resolve --actioned once the directive actually changed.
  resolve --dismissed when the friction is simply the cost of doing business.

An actioned note that recurs reopens itself. That is the corpus's most useful
signal: the fix did not hold.

Retention runs from up. Nothing here needs pruning by hand.";

/// The intro, then whichever topics were asked for, then the usage block. Every
/// invocation is a whole document, so a topic never arrives without the frame
/// that makes it legible.
pub fn render(asked: &[String]) -> Result<String> {
    if let Some(unknown) = asked.iter().find(|name| topic(name).is_none()) {
        bail!(
            "no guide topic called {unknown:?}. Topics: {}",
            topic_names()
        );
    }
    let mut out = String::from(INTRO);
    for (name, body) in TOPICS {
        if asked.iter().any(|a| a == name) {
            out.push_str("\n\n");
            out.push_str(body);
        }
    }
    out.push_str("\n\n");
    out.push_str(USAGE);
    Ok(out)
}

fn topic(name: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, body)| *body)
}

pub fn topic_names() -> String {
    TOPICS
        .iter()
        .map(|(key, _)| *key)
        .collect::<Vec<_>>()
        .join(", ")
}
