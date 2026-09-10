//! Doctrine: what this is for, and what it is deliberately not for.

/// Every doctrine topic, in the order a reader meets them.
pub const TOPICS: &[(&str, &str)] = &[("sources", SOURCES), ("cadence", CADENCE)];

const INTRO: &str = "\
coop is an inbox for nags, and its intended state is empty.

Every notice in it is something you can be rid of today, by doing one thing.
That is the whole admission test, and it is what keeps a card worth reading
after the first week.
";

const SOURCES: &str = "\
sources

  A producer declares a condition. It does not send a notification, and it
  never retracts one — which is the point. A thing that has to remember to
  retract eventually forgets, and then the coop holds a notice about a problem
  that was fixed a month ago.

  So a notice retires itself. The one for up is a predicate over the timestamp
  up already writes, which means running up is the dismissal. Neither producer
  migrated into coop needed a line of code written for it.

  Two rules decide whether something belongs here.

  It must be actionable. A source states what retires it, and coop refuses a
  stat declaration without one. A notice nobody can act on is furniture, and
  furniture is what stops a card being read.

  It must be transient. A note in relic-core's vocabulary is read but not
  graded, which is the opposite of actionable, so notes never reach the card at
  all. Standing status belongs in the prompt, or in assay, or nowhere.

  doctor enforces both after the fact: a notice still standing after a
  fortnight is reported as furniture, whoever declared it.
";

const CADENCE: &str = "\
cadence

  The card is edge-triggered. It is drawn on the first prompt of a shell, and
  after that only when the outstanding set actually changes.

  Drawing it before every prompt was the obvious design and the wrong one. A
  card that repeats stops being read within a day, which costs exactly the
  thing it was for. It is also unsound in fish, where the prompt event is
  multi-subscriber and promises nothing about repaints: a window resize would
  redraw the card.

  What is level-triggered instead is the badge, a count in the prompt itself.
  It is there for as long as anything is outstanding and gone the moment
  nothing is, which is the 99% case.

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
