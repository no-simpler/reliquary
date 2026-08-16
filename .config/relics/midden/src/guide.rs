use anyhow::{Result, bail};

/// Canonical order, so a guide reads the same whatever order its topics were
/// asked for.
pub const TOPICS: &[(&str, &str)] = &[("file", FILE), ("drain", DRAIN)];

pub const INTRO: &str = "Agents give feedback to the user via this tool.";

// Deliberately not opened with a `\` continuation: that would swallow this
// block's leading indentation along with the newline.
pub const USAGE: &str = "  midden help   CLI usage";

pub const FILE: &str = "\
FILE

  midden note --kind <kind> --title '...' --target <path> --body -

kind, chosen before the title, because each one names where its fix lives:

  gap        Nothing covered the situation. You guessed.
  conflict   Two directives pulled opposite ways.
  stale      A directive named a path, flag or symbol that no longer resolves.
  hunt       A fact that should have been declared cost repeated searching.
  rebuff     The user corrected or rejected you.
  friction   Harness or tooling: a permission prompt, a missing capability.
  rework     Work was done, then discarded.

One note, one cause. Two causes in one cannot be resolved separately.
Evidence in the body: quote what happened. No quote, no note.
Target the file that should have said it, not the symptom.
Name the cause, not the feeling: unstated, contradicted, moved.

For a rebuff, the title names the directive that would have prevented it, not
what you were asked to do differently.

Do not look for an existing note first. The same cause folds itself and bumps
its count.

Anything before a compact boundary is lost. Say so; do not reconstruct it.";

pub const DRAIN: &str = "\
DRAIN

Turn the corpus into directive changes, then empty it. No other session will.

  midden digest                 open notes, grouped by the file their fix lands in
  midden show <id>              the evidence behind one
  midden resolve <id> --actioned | --dismissed
  midden archive <id>           retire without a verdict

Work a group at a time: one section is one file to open. Groups are ordered by
what the corpus says that file has cost — heaviest first, and notes with no
target last, because those are not yet a worklist.

Read the evidence before changing anything. A note is one session's account of
one moment, and occurrences are how much weight that account carries. Seen once
may still be right; seen five times may still be the cost of doing business.

Every note gets one of three outcomes.

  actioned    a directive actually changed. Name the edit in your report.
  dismissed   the friction is real and no fix is coming.
  archive     unusable as filed: no evidence, two causes tangled together, or a
              claim that no longer applies to how the tree looks now.

Resolving is not bookkeeping. An open note is a claim on the next draining
session, and a corpus nobody closes is a corpus nobody reads.

An actioned note that recurs reopens itself. That is the most useful signal
here: the fix did not hold, so the second attempt wants a different shape than
the first — a stricter rule, a different file, or a check that fails loudly.

Where a fix lands follows co-locality: the most granular file covering every
affected case. What one project needs does not belong in a root directive, and
what three projects hit does not belong in one of them.

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
