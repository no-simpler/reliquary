//! Doctrine. What this is for, and why it is shaped the way it is.
//!
//! Disjoint from `help`, which carries reference.

use anyhow::{Result, bail};

/// Canonical order, so a guide reads the same whatever order its topics were
/// asked for.
pub const TOPICS: &[(&str, &str)] = &[
    ("ladder", LADDER),
    ("irregularity", IRREGULARITY),
    ("custody", CUSTODY),
];

/// The frame every guide arrives in.
pub const INTRO: &str = "\
ROTE

  A spaced-repetition drill for the small set of passwords that have to live in
  a head rather than in a vault. It measures whether they are still there.";

// Deliberately not opened with a continuation: that would swallow this block's
// leading indentation along with the newline.
/// The usage block every rendering is framed with.
pub const USAGE: &str = "  rote help   CLI usage";

const LADDER: &str = "\
THE LADDER

  Expanding intervals, 1, 1, 2, 4, 7, 14, 30 days, held at a month. An unaided
  first pass moves up a rung; a first miss returns to the foot, which keeps a
  shaky secret at daily intervals without a separate learning mode. The shape is
  from Bonneau and Schechter, USENIX Security 2014, who held 56-bit secrets at
  roughly 88% unaided recall on a schedule of this kind. The intervals are
  configurable; the shape is the default and not a rule.

  The cap is not a claim about memory. It is how stale the evidence is allowed
  to get: at a month you are never more than a month from finding out whether
  you still have it, and the record accumulates month-long retention data as a
  by-product of ordinary use. Nothing has to be remembered to collect it.

  rote records and proposes. It renders no verdict. It will not refuse a drill
  that is not due, it will not refuse a second one the same day, and nothing
  else in the system is made conditional on a drill being current. What the
  numbers mean is yours to decide, which is why every threshold that once
  produced a word now produces a number in rote stats.

  A rotation is a different secret and a different memory, so it starts a new
  engram at the foot and the measurement starts again with it. Nothing is
  pooled across one.";

const IRREGULARITY: &str = "\
IRREGULARITY

  The schedule is a proposal and the actual cadence is yours. Drilling more
  often than proposed, less often, or not at all for a fortnight are all
  ordinary, and all three stay measurable.

  Three intervals are recorded. The scheduled one is what the ladder asked for.
  The actual one is how long it had been since the schedule was last served. The
  effective one is how long it had been since the secret was in front of a
  person at all, by any route, and that is the honest retention interval: extra
  practice cannot dress a one-day recall up as a month-long one, and a fortnight
  away arrives as long-interval evidence rather than as a gap.

  Those figures are recomputed from the whole record rather than read back off
  the line that carries them. A machine writes what it could see at the time,
  and two machines that wrote without having seen each other saw different
  things.

  A week away from the drill is therefore where long-interval readings come
  from, and they are banded with everything else in rote stats.";

const CUSTODY: &str = "\
CUSTODY

  The secret is kept as an argon2id verifier over a random salt, in no
  recoverable form. rote can say wrong. It can never say what the right answer
  was, and there is no reveal.

  The stance is good faith throughout. rote guards against accident and
  confusion and not against you: it records what it was told and what it saw,
  and it verifies only what it can. You could make the record say anything you
  liked. You are trusted not to, and at worst to do it by mistake.

  The drill is blind and refuses a paste, because an answer read out of a vault
  measures nothing. Submitting nothing concedes: that is a failure of recall and
  is recorded as one, which is more honest than typing something to get past the
  prompt. After a miss the lookup is offered, and an entry taken that way is
  aided: recorded in full, kept out of every figure that claims to measure a
  memory. The first week is aided almost throughout, and that is the shape of a
  first week rather than a run of failures.

  An aided entry earns its keep twice. Typing what the vault shows and being
  told wrong is the one routine signal that the vault item and the verifier hold
  different secrets, which is otherwise undetectable.

  Three verbs take a secret, and the difference between them is the whole point.

  enroll opens a lineage with its first engram.

  rotate proves the current secret, supersedes its engram, and starts a new one
  at the foot. A different secret, a different memory.

  attach makes a verifier on this machine for the engram that is already
  current, and leaves its record untouched. The same secret, a machine that lost
  its copy — which is what a restored flagship looks like, since a verifier is
  never backed up.

  rote cannot tell an attach from a rotate by looking at what was typed. It
  holds nothing to check a claim against, so an attach has no failure mode for
  the wrong secret: it records which claim was made and verifies neither. That
  is a better trade than the alternative, where losing a laptop costs the whole
  record of a memory you still hold. Before it asks, it names what it is about
  to continue, so attaching the wrong lineage has a moment to be noticed.

  The lineage name matches the 1Password item title by convention and not by
  integration. Nothing is stored, nothing goes out of sync, no vault reference
  is added, and the drill does not fail in exactly the world escrow exists for.";

/// Assemble a guide: the frame, the topics asked for, and the usage line.
///
/// # Errors
///
/// When a topic was asked for that does not exist.
pub fn render(asked: &[String]) -> Result<String> {
    for name in asked {
        if !TOPICS.iter().any(|(topic, _)| topic == name) {
            bail!(
                "no guide topic called {name:?}. Topics: {}",
                TOPICS
                    .iter()
                    .map(|(topic, _)| *topic)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
        }
    }
    let mut parts = vec![INTRO.to_owned()];
    for (name, body) in TOPICS {
        if asked.is_empty() || asked.iter().any(|wanted| wanted == name) {
            parts.push((*body).to_owned());
        }
    }
    parts.push(USAGE.to_owned());
    Ok(parts.join("\n\n"))
}

#[cfg(test)]
mod tests {
    use super::{INTRO, TOPICS, USAGE, render};

    #[test]
    fn a_bare_guide_is_the_whole_document() {
        let text = render(&[]).unwrap();
        assert!(text.starts_with(INTRO));
        assert!(text.ends_with(USAGE));
        for (name, _) in TOPICS {
            assert!(text.contains(&name.to_uppercase()), "{name}");
        }
    }

    #[test]
    fn topics_come_back_in_canonical_order_however_they_were_asked_for() {
        let forwards = render(&["ladder".to_owned(), "custody".to_owned()]).unwrap();
        let backwards = render(&["custody".to_owned(), "ladder".to_owned()]).unwrap();
        assert_eq!(forwards, backwards);
        let ladder = forwards.find("THE LADDER").unwrap();
        let custody = forwards.find("CUSTODY").unwrap();
        assert!(ladder < custody);
    }

    #[test]
    fn a_topic_still_arrives_inside_the_frame() {
        let text = render(&["custody".to_owned()]).unwrap();
        assert!(text.starts_with(INTRO));
        assert!(text.contains("CUSTODY"));
        assert!(!text.contains("THE LADDER"));
        assert!(text.ends_with(USAGE));
    }

    #[test]
    fn an_unknown_topic_names_the_ones_that_exist() {
        let error = render(&["nonsense".to_owned()]).unwrap_err().to_string();
        assert!(error.contains("ladder"));
        assert!(error.contains("nonsense"));
    }

    #[test]
    fn no_topic_uses_backticks() {
        for (name, body) in TOPICS {
            assert!(!body.contains('`'), "{name}");
        }
    }
}
