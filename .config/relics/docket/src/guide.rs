use anyhow::{Result, bail};

/// Canonical order, so a guide reads the same whatever order its topics were
/// asked for.
pub const TOPICS: &[(&str, &str)] = &[("handoff", HANDOFF), ("relay", RELAY), ("spec", SPEC)];

pub const INTRO: &str = "\
DOCKET

Semi-transient agentic memory for bridging and orchestrating otherwise unrelated
sessions.

handoff = one-off prompt.
relay = transitive prompt; a relay goes in, another comes out.
spec = multi-session plan, evolved across design and implementation stages.

All item kinds archived on completion.";

// Deliberately not opened with a `\` continuation: that would swallow this
// block's leading indentation along with the newline.
pub const USAGE: &str =
    "  docket create <kind> --title '...' --tagline '...'   opens one, prints path
  docket show 4mve                                     reads one
  docket archive 4mve                                  completes one

  docket guide handoff|relay|spec                      per-kind guidance
  docket help                                          CLI usage";

pub const HANDOFF: &str = "\
HANDOFF

Singleton: deferred work for one session.
May be one task, may be multiple, even unrelated.
Transitive handoffs ok, but consider relay.";

pub const RELAY: &str = "\
RELAY

Singleton: deferred work for multiple sessions.
Each session discovers scope of next, replaces relay or archives.
For design knowable in advance, consider spec.";

pub const SPEC: &str = "\
SPEC

Directory with entrypoint + supporting files.
Cross-session evolution in-place: design in-depth, then checklist
implementation.";

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
