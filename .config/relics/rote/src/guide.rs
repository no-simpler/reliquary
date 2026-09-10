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
    ("probes", PROBES),
];

/// The frame every guide arrives in.
pub const INTRO: &str = "\
ROTE

  A spaced-repetition drill for the small set of passwords that have to live in
  a head rather than in a vault. It measures whether they are still there.";

// Deliberately not opened with a continuation: that would swallow this block's
// leading indentation along with the newline.
pub const USAGE: &str = "  rote help   CLI usage";

const LADDER: &str = "\
THE LADDER

  Expanding intervals, 1, 1, 2, 4, 7 days, held at a week. A first-attempt pass
  moves up a step; a first-attempt failure returns to the foot, which keeps a
  shaky secret at daily intervals without a separate learning mode.

  The shape is from Bonneau and Schechter, USENIX Security 2014, who held
  56-bit secrets at roughly 88% unaided recall on a schedule of this kind.

  rote records rather than gates. It will not refuse an entry that is not due,
  it will not refuse a second entry the same day, and it does not make anything
  else in the system conditional on a drill being current. Those were all
  available and were declined: the ladder is what rote expects, not what it
  requires, and a person who wants to drill more should not have to argue with
  the instrument that measures them.

  The one place it is strict is the cutover gate, and that is a verdict rather
  than a permission: it will not report a slug ready on evidence that does not
  meet the bar.

  Do not change a lock until the new key has survived spacing. Read that off
  rote status: three consecutive first-attempt passes at a cold seven-day
  interval, which is about five weeks from a fresh enrolment.

  One entry per sitting scores. A second entry a minute later is primed by the
  first and measures transcription rather than recall, so the retries exist to
  tell a slipped key from a real loss, and nothing more.

  The drill runs before the lookup. The card says so at the prompt, because that
  is where the rule applies, and it offers the lookup afterwards rather than
  pretending nobody needs one.
";

const IRREGULARITY: &str = "\
IRREGULARITY

  A human schedule will not hold, and rote is built for that rather than
  against it.

  Drilling more often than the ladder asks costs nothing. The extra entries are
  practice: recorded in full, kept out of the retention figures, and unable to
  move the ladder in either direction. What they do move is the effective
  interval, and because the gate reads that rather than the schedule, a week of
  daily practice simply means the next review is one-day evidence and is
  reported as such.

  Drilling less often costs nothing either. An entry after a fortnight scores
  normally, and because its effective interval is twice the scheduled one it is
  marked a stretch and lands in the long-interval band. A holiday becomes decay
  data rather than a gap.

  A failure after a long absence is still a failure and still resets the
  ladder, but the statistics keep it apart from a failure inside the schedule.
  One is expected decay; the other is not.

  The daily session is composed from what is due plus, as practice, whatever is
  still climbing the ladder. A slug at the cap drops out of that filler so it
  can earn a cold entry at the cap interval. Set filler to all in the config to
  drill everything every day, and accept that the gate then has nothing cold to
  read.
";

const CUSTODY: &str = "\
CUSTODY

  The secret is held as an argon2id verifier and in no recoverable form. rote
  can say wrong. It can never say what the right answer was, which is the
  property that makes it safe to run every morning.

  It never renders a typed secret. No echo, no reveal, no confirmation display,
  no error that quotes what was entered. The sitting runs on the alternate
  screen and leaves nothing in scrollback.

  A paste is refused rather than accepted, because a drill answered from a
  vault measures nothing. That is a low bar and the only one there is: rote
  cannot tell that the answer was on a second screen, and does not pretend to.
  It assumes good faith, which is reasonable for a tool with one user who wants
  the measurement to be true.

  What it owes that person is a clear intention and somewhere to put the truth.
  The prompt asks for memory only, and for an empty entry when memory gives
  nothing, so that conceding is a keystroke rather than a reason to type
  something to get past the screen. A blank is a failure of recall and counts as
  one; typed nonsense would count as the same thing while looking like an
  attempt.

  After a miss the card offers the lookup. Take it and the next entry is aided:
  recorded in full, out of retention, out of the latency series, out of the
  buckets, and unable to move the ladder. Week one is aided almost entirely, and
  that is the honest shape of week one rather than a fault in it. rote status
  reports the day a slug first stood alone, which is where its memory actually
  starts. rote --aided says the same about a sitting begun after a lookup.

  An aided entry earns its keep twice. Typing what the vault shows and being
  told wrong is the one routine signal that the item and the verifier have
  drifted apart, which otherwise goes unnoticed until the drill is rehearsing a
  secret nothing else uses.

  The slug is the title of the matching 1Password item, by convention and not
  by integration. Nothing is stored, nothing goes out of sync, and the drill
  does not fail in exactly the world an escrow exists for.

  A rotation is rote rekey, and it proves the current secret before accepting a
  new one. Without that, a verifier could be replaced by one somebody else
  knows, and the reading would still say memorised.
";

const PROBES: &str = "\
STRETCH PROBES

  The ladder holds at a week, so nothing on it says whether a phrase survives
  three months. Nobody has published that figure for an artifact of this kind,
  which makes the log the only source of it.

  rote probe holds a slug out of the reminder until a chosen day, thirty to
  ninety out, and labels the entry taken on or after it. The horizon is spent
  by that entry, and the schedule counts from it.

  A probe does not move the ladder in either direction. The horizon was chosen,
  so failing it is a reading rather than a lapse in the schedule. It does count
  toward the cutover gate when it passes cold at the cap interval or beyond,
  because that is exactly the evidence the gate wants.

  A long absence produces the same evidence without being asked for. Probes are
  for when the absence has to be deliberate.
";

/// The intro, then whichever topics were asked for, then the usage block. Every
/// invocation is a whole document, so a topic never arrives without the frame
/// that makes it legible.
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
        let text = render(&["probes".to_owned()]).unwrap();
        assert!(text.starts_with(INTRO));
        assert!(text.contains("STRETCH PROBES"));
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
