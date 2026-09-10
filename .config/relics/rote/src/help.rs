//! Reference. What the flags mean, what a record holds, what an exit code says.
//!
//! Disjoint from `guide`, which carries doctrine. Nothing that teaches the tool
//! lives in a file the binary does not ship.

/// Canonical order.
pub const TOPICS: &[(&str, &str)] = &[
    ("intervals", INTERVALS),
    ("records", RECORDS),
    ("files", FILES),
    ("stdin", STDIN),
    ("exit", EXIT),
];

/// A topic body, by name.
pub fn topic(name: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(topic, _)| *topic == name)
        .map(|(_, body)| *body)
}

/// The topic names, comma-joined.
pub fn topic_names() -> String {
    TOPICS
        .iter()
        .map(|(topic, _)| *topic)
        .collect::<Vec<_>>()
        .join(", ")
}

const INTERVALS: &str = "\
INTERVALS

  The ladder is 1, 1, 2, 4, 7 days, held at 7. A first-attempt pass moves one
  step up; a first-attempt failure returns to the foot.

  Three intervals are recorded against every entry, and they answer different
  questions.

    scheduled   what the ladder asked for
    actual      days since the previous scheduled review
    effective   days since the previous entry of any kind

  Effective is the honest one, and it is what every statistic and the cutover
  gate read. Practising a slug the day before its review does not make that
  review seven-day evidence, and the numbers say so.

  An entry whose effective interval reaches twice the scheduled one is marked a
  stretch, whether it was a deliberate probe or a fortnight away from the
  machine.
";

const RECORDS: &str = "\
RECORDS

  One JSON object per line, appended and never rewritten. Every record carries
  the schema version, the instant, the drill day it was credited to, the machine
  that wrote it, and the SHA-256 of the line before it.

  Five kinds: add, rekey, retire, probe, attempt.

  An attempt records the class, which try it was, how it ended, the time to the
  first keystroke, the time from there to submission, the corrections, any
  refused pastes, the three intervals, and the ladder step either side.

  It does not record the input, its length, or any prefix. Nothing in the log is
  a secret, and the verifiers are not in it.

  The chain of digests is tamper evidence, not a control: anyone who can edit
  the file can recompute it. It is there so that an edit or a truncation is
  visible instead of silent. rote doctor checks it.
";

const FILES: &str = "\
FILES

  ~/Trove/ark/rote/log.jsonl      the log. Durable, and restic's to keep
  ~/.local/state/rote/            the verifiers and the reminder cache
  ~/.config/rote/config.toml      optional

  The two homes are deliberate. An argon2id verifier is an offline-attackable
  confirmation oracle, and it is regenerable by re-enrolment, so it has no
  business in a tree that replicates to two providers and keeps prior versions.
  The log is the opposite: not regenerable, and carrying no secret material.

  A slug whose verifier is missing is a first-class state, not a corruption. It
  is what a restore onto a new machine looks like. The history survives; the
  oracle does not.

  Config keys, all optional: root, state, rollover-hour, max-attempts, filler.
  Filler is one of below-cap, all, none.

  Overrides for a trial run or a test: ROTE_ROOT, ROTE_STATE, ROTE_CONFIG,
  ROTE_UI, ROTE_HOST.
";

const STDIN: &str = "\
STDIN

  rote add --stdin and rote rekey --stdin read secrets from a pipe, one per
  line. rekey wants the current secret first, then the new one, unless --force
  is given, in which case it wants only the new one.

  Both refuse to run when any standard stream is a terminal. That is what stops
  the pipe becoming a habit, because a secret typed into a shell command lands
  in shell history.

  The drill itself never reads stdin. It has to be typed at a terminal, and a
  paste is refused.
";

const EXIT: &str = "\
EXIT CODES

  0   clean, or nothing was due
  1   a first-attempt failure, or a soft finding from doctor
  2   the sitting was abandoned, or doctor found something broken
  1   any other refusal, on stderr

  doctor exits on its grade, so a non-zero status there means it had something
  to report rather than that it failed to run.
";

#[cfg(test)]
mod tests {
    use super::{TOPICS, topic, topic_names};

    #[test]
    fn every_topic_resolves_and_is_headed() {
        for (name, _) in TOPICS {
            let body = topic(name).unwrap();
            assert!(!body.is_empty());
            assert_eq!(
                body.lines().next().unwrap(),
                body.lines().next().unwrap().to_uppercase(),
                "{name} should open with a heading"
            );
        }
    }

    #[test]
    fn an_unknown_topic_is_none_rather_than_empty() {
        assert!(topic("nonsense").is_none());
    }

    #[test]
    fn the_names_list_every_topic() {
        let names = topic_names();
        for (name, _) in TOPICS {
            assert!(names.contains(name), "{name}");
        }
    }

    #[test]
    fn no_topic_uses_backticks() {
        for (name, body) in TOPICS {
            assert!(!body.contains('`'), "{name}");
        }
    }
}
