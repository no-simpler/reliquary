use anyhow::{Result, bail};

/// Canonical order, so a guide reads the same whatever order its topics were
/// asked for.
pub const TOPICS: &[(&str, &str)] = &[
    ("modes", MODES),
    ("schedule", SCHEDULE),
    ("writing", WRITING),
];

pub const INTRO: &str = "\
MANTRA

A mode is a reusable paragraph of behavioural directives, switched on for a
session by a +token at the start of a prompt line.

A mode declares when it is said, not only what it says. Salience decays as a
context window fills, so a directive delivered once at turn one is the most
diluted thing in a long session.";

// Deliberately not opened with a `\` continuation: that would swallow this
// block's leading indentation along with the newline.
pub const USAGE: &str = "  mantra list                     what a +token can reach
  mantra explain                  why each mode did or did not fire
  mantra dry-run terse slash      what activating those would inject

  mantra guide modes|schedule|writing   doctrine
  mantra help                           CLI usage";

pub const MODES: &str = "\
MODES

Markdown, because prose is the payload. Optional metadata carries the
schedule and the refrain; the body carries the directives.

Home tree first, then its skills-dir plugins, then the project's. First hit
wins, so a project cannot redefine what a +token means on this machine.
Marketplace plugins are never searched: a +token must not pull directives
out of third-party code.

Switching a mode off is not a thing. Tokens cannot be taken back out of a
context, so the runtime can only stop repeating, never retract.";

pub const SCHEDULE: &str = "\
SCHEDULE

Measured in context tokens, not turns. A turn is a sentence or a hundred
thousand tokens of tool output, and only one of those moves a directive
away from the model's attention.

An activation says the body. A refresh says the refrain. Never both in one
injection, and the first delivery is always the body.

A compaction restates every active mode in full: the originals were
summarised away, and the state file is the only thing that still knows
they were ever switched on.";

pub const WRITING: &str = "\
WRITING

Directives, not steps. A mode is a behavioural toggle; a procedure belongs
in a skill.

Keep the body short. Every word is re-read on every activation, and a mode
long enough to skim is a mode that gets skimmed.

Give a refrain to a mode whose body carries exceptions. Repeating an
exception is periodically re-issuing permission to ignore the rule.

One line is the whole budget for a refrain. Needing a paragraph means the
imperative is not sharp enough yet to survive repetition.";

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
