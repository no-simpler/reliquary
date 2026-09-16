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

  Expanding intervals, 1, 1, 2, 4, 7, 14, 30 days, held at a month. When the
  schedule asks, a cold pass moves up a rung; a fail returns to the foot, which
  keeps a shaky secret at daily intervals without a separate learning mode. The
  shape is from Bonneau and Schechter, USENIX Security 2014, who held 56-bit
  secrets at roughly 88% unaided recall on a schedule of this kind. The
  intervals are configurable; the shape is the default and not a rule.

  Three lines are the whole of the reconciliation. A cold pass on a day the
  engram is due climbs a rung and anchors the schedule to that day. A cold pass
  on any other day moves nothing: a drill nobody asked for cannot carry an
  engram up toward the cap. A fail on any day returns the engram to the foot,
  because forgetting is forgetting whichever day it was found out on.

  A drill on a day the engram is due is a review; a drill on any other day is a
  practice. Which it was is never declared and never stored. Replay reads it
  off the engram's standing on the day, from the ladder and the drills before,
  so a drill on a due engram is a review whoever asked for it — and a change to
  the ladder re-derives the whole history rather than leaving two schedules in
  one file.

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

  Three figures stand beside every drill, and none of them is written. The
  scheduled interval is what the ladder asked for at the rung the engram stood
  on. The lateness is how far past its due day a review came, and it is read
  over reviews only, because a practice was never due. The gap is how long it
  had been since the secret was in front of a person at all, by any route, and
  that is the honest retention interval: extra practice cannot dress a one-day
  recall up as a month-long one, and a fortnight away arrives as long-interval
  evidence rather than as a hole in the record.

  All three are read at replay, from the merged corpus, rather than off the
  line that carries the drill. A machine that writes what it could see at the
  time writes one machine's view, and two machines that wrote without having
  seen each other saw different things.

  A week away from the drill is therefore where long-interval readings come
  from, and they are banded with everything else in rote stats.";

const CUSTODY: &str = "\
CUSTODY

  The secret is kept as an argon2id verifier over a random salt, in no
  recoverable form. rote can say wrong. It can never say what the right answer
  was: a verifier answers yes or no and holds nothing to read back.

  The stance is good faith throughout. rote guards against accident and
  confusion and not against you: it records what it was told and what it saw,
  and it verifies only what it can. You could make the record say anything you
  liked. You are trusted not to, and at worst to do it by mistake.

  A cold capture gives back nothing about what was typed. It is the one prompt
  that measures a memory, and everything else follows from that one line: it
  refuses a paste, it draws nothing, it offers no reveal and it moves no caret.
  A count of characters is partial recognition feedback handed back in the
  middle of a retrieval, and showing the answer before it is submitted is the
  whole of one. Neither may reach the prompt that is measuring. Submitting
  nothing is a fail: that is a failure of recall and is recorded as one, which
  is more honest than typing something to get past the prompt.

  After a cold fail the drill goes on as follow-ups, masked and unbounded, and
  they measure nothing: the fail is already on the record. They exist for
  peace of mind — did I forget it, or did I mistype it — and because the
  corrective answer after an error is where the retention effect lives
  (Pashler et al. 2005), which is what makes the lookup worth offering at all.
  ctrl-l in a follow-up declares the drill aided and polices nothing: the chip
  says so, the key is withdrawn, and rote cannot see a vault typing into the
  window either way. A follow-up that passes is recovered, and the fail stands
  in every figure that claims to measure a memory. A first week is mostly
  fails recovered in a follow-up, and that is the shape of a first week
  rather than a run of failures.

  Every other prompt masks what is typed, one glyph a character, and ctrl-r
  shows the characters themselves until you press it again. NIST SP 800-63B-4
  asks verifiers to offer exactly that, and for the reason these prompts exist:
  a secret typed unseen into an enrollment is a verifier for something nobody
  knows, and nothing later can tell you so. That is also why every intake is a
  double entry, and why two entries that differ write nothing and end the
  command. A mask leaks a length, which the login screen on this machine leaks
  too. Word boundaries it does not, and never will: seven word lengths is most
  of a diceware phrase's search space, which is also why the word-wise keys
  work only once the characters are already showing. A jump that travels the
  width of a word discloses the width of a word.

  A reveal is never on by itself and never survives the prompt that opened it,
  and it goes back on its own when the window loses focus or thirty seconds
  pass with nothing typed. What it cannot do is decide who else is looking at
  the screen, or whether the session is being recorded. That part is yours.

  A cold capture is the one prompt that refuses a paste. A follow-up accepts
  one, and so does every intake. The line is measurement and not scripting: an
  answer pasted into a cold capture is a re-study rather than a retrieval, and
  retrieval is what strengthens a memory (Roediger and Karpicke 2006). So a
  paste there would weaken the very memory it claimed to test and write a pass
  that stands for none, after which the item most in need of drilling is the
  one the schedule asks for least.

  That refusal is a commitment device against your own hurry, and it is nothing
  more. Bracketed paste is advisory: the terminal is asked to mark a paste and
  there is no way to learn whether it agreed, so wherever it does not, a paste
  arrives as typing and is taken. Nor does refusing one prevent a lookup — it
  moves it to a vault typing into the window, which rote cannot see at all.

  Three verbs take a secret, and the difference between them is the whole point.

  enroll opens a lineage with its first engram.

  rotate takes a new secret, typed twice, supersedes the engram, and starts a
  new one at the foot. It proves nothing about the old secret: a different
  secret, a different memory, and nothing here to check the new one against.
  The card names the engram about to step down before it asks, and that is the
  whole of the guard.

  attach makes a verifier on this machine for the engram that is already
  current, and leaves its record untouched. The same secret, a machine that lost
  its copy — which is what a restored flagship looks like, since a verifier is
  never backed up. Before it asks, it names what it is about to continue, so
  attaching the wrong lineage has a moment to be noticed.

  rote cannot tell an attach from a rotate by looking at what was typed. It
  holds nothing to check a claim against, so an attach has no failure mode for
  the wrong secret: it records which claim was made and verifies neither. That
  is a better trade than the alternative, where losing a laptop costs the whole
  record of a memory you still hold.

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
