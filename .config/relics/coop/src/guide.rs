//! Doctrine: what this is for, and what it is deliberately not for.

/// Every doctrine topic, in the order a reader meets them.
pub const TOPICS: &[(&str, &str)] = &[("sources", SOURCES), ("cadence", CADENCE)];

const INTRO: &str = "\
coop is an inbox for nags. Its author keeps inboxes empty: a nag is read,
acted on, and gone. The design leans on that habit, so most of the time
there is nothing here.
";

const SOURCES: &str = "\
sources

  A producer declares a condition rather than sending a notification, so a
  notice retires itself when the condition stops holding. That spares the
  producer the step most easily forgotten: retracting, which otherwise
  leaves a notice about something fixed a month ago.

  The notice for up is a predicate over the timestamp up already writes,
  which means running up is the dismissal. Neither producer migrated into coop
  needed a line of code written for it.

  coop holds no answer of its own. It evaluates every source afresh before
  every prompt; a producer that needs a cache owns it, because only the
  producer knows when its truth changes.

  Two defaults follow from the empty-inbox habit.

  Actionable. A source says what retires it, and coop asks every stat
  declaration for a fix. A notice nobody can act on tends to sit, and a card
  of sitting notices tends to stop being read.

  Transient. A note in relic-core's vocabulary is read but not graded, so
  the card leaves notes out. Standing status usually fits the prompt or
  assay better.

  doctor nudges after the fact: a notice standing past a fortnight is
  reported as likely furniture, a prompt to act on it or reconsider the
  source.
";

const CADENCE: &str = "\
cadence

  The card is edge-triggered. It is drawn on the first prompt of a shell, and
  after that only when the outstanding set actually changes.

  A card drawn before every prompt tends to stop being read within a day,
  which costs exactly the thing it is for. It is also unsound in fish, where
  the prompt event is multi-subscriber and promises nothing about repaints:
  a window resize would redraw the card.

  What is level-triggered instead is the badge, a count in the prompt itself.
  It is there for as long as anything is outstanding and gone the moment
  nothing is, which is the usual case.

  Per-shell rather than per-machine, so a second terminal shows the card too.
  Neither repeats it.
";

const USAGE: &str = "  coop help   CLI usage";

/// One topic, by name.
pub fn topic(name: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(topic, _)| *topic == name)
        .map(|(_, body)| *body)
}

/// Every topic name.
pub fn topic_names() -> Vec<&'static str> {
    TOPICS.iter().map(|(name, _)| *name).collect()
}

/// The frame, the asked topics in canonical order, and the footer.
///
/// Every invocation is a whole document, so a topic never arrives without the
/// frame that makes it legible.
pub fn render(asked: &[String]) -> String {
    let wanted: Vec<&'static str> = if asked.is_empty() {
        topic_names()
    } else {
        TOPICS
            .iter()
            .filter(|(name, _)| asked.iter().any(|topic| topic == name))
            .map(|(name, _)| *name)
            .collect()
    };
    let mut out = String::from(INTRO);
    out.push('\n');
    if wanted.is_empty() {
        out.push_str("No such topic. Try one of: ");
        out.push_str(&topic_names().join(", "));
        out.push_str("\n\n");
    }
    for name in wanted {
        if let Some(body) = topic(name) {
            out.push_str(body);
            out.push('\n');
        }
    }
    out.push_str(USAGE);
    out.push('\n');
    out
}
