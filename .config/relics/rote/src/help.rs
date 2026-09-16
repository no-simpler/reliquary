//! Reference. What the flags mean, what a record holds, what an exit code says.
//!
//! Disjoint from `guide`, which carries doctrine. Nothing that teaches the tool
//! lives in a file the binary does not ship.

/// Canonical order.
pub const TOPICS: &[(&str, &str)] = &[
    ("keys", KEYS),
    ("intervals", INTERVALS),
    ("records", RECORDS),
    ("files", FILES),
    ("machines", MACHINES),
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

const KEYS: &str = "\
KEYS

  Everywhere a secret is typed:

    enter       submit what is typed
    escape      pass this prompt over
    backspace   remove the character before the caret. Also ctrl-h
    ctrl-u      remove everything before the caret
    ctrl-c      abandon
    ctrl-d      abandon, when nothing is typed

  A cold drill try is the one prompt that measures a memory, and it gives back
  nothing about what was typed. The field stays blind — no count, no caret, no
  reveal — a paste is refused, and the keys above are the only ones that do
  anything at all. Submitting nothing concedes. ctrl-d abandons there whatever
  is typed, because there is no caret to delete at. ctrl-l goes and looks it up,
  then takes the entry as an aided one, and is offered once a cold try is
  already on record.

  Every other prompt — the aided entry after ctrl-l, an attachment, and every
  prompt in enroll, attach and rotate — masks what is typed, one glyph a
  character, takes a paste, and adds:

    ctrl-r          show the characters themselves, or stop showing them
    left right      move one character. Also ctrl-b and ctrl-f
    home end        move to either end. Also ctrl-a and ctrl-e
    delete          remove the character at the caret. Also ctrl-d
    ctrl-k          remove everything from the caret on
    ctrl-t          swap the character before the caret with the one at it
    alt-b alt-f     move one word. Also ctrl-left and ctrl-right
    ctrl-w          remove back to the start of a word. Also alt-backspace
    alt-d           remove forward to the end of a word

  That is readline's set, so it is the set your shell already has. Whether a
  terminal sends alt at all is its own setting; ctrl-left and ctrl-right arrive
  everywhere.

  A reveal is never on by itself and never survives the prompt that opened it.
  It goes back on its own when the window loses focus, and after thirty seconds
  with nothing typed. The card names ctrl-r wherever it works, so a key with no
  chip beside it does nothing there.

  The word-wise keys work only while the characters are showing. Over a masked
  field the distance a word jump travels is a word length, and word lengths are
  most of what a passphrase's strength is made of. There is no yank and no
  undo: both would keep a copy of what was removed for longer than the entry it
  came from.

  A view wider than the field says so on the border it is cut off at, so an
  unmarked field is the whole of what was typed and nothing else ever looks
  like one.

  At an attachment prompt, where a verifier is being made rather than checked,
  an empty entry is refused rather than conceded: there is nothing here to
  concede to. There is no lookup there either — offering one would imply that
  what is typed is being checked against something, and it is not.

  Anywhere a secret is typed twice, two entries that differ end the command:
  the card says so, and nothing is written. An empty entry is refused and the
  half already typed is kept.

  When nothing is due, bare rote asks whether you want to practice anyway. That
  is one keystroke and no return: y practices, any other key leaves it.

  Pastes accepted and pastes refused are both recorded against the sample, and
  neither is a count of lookups. What that refusal is and is not is in rote
  guide custody.";

const INTERVALS: &str = "\
INTERVALS

  The ladder is 1, 1, 2, 4, 7, 14, 30 days by default, held at the last of them.
  An unaided first pass on a due drill moves up a rung; a first miss returns to
  the foot. Set your own with the ladder key in the config.

  Three intervals are recorded against every sample.

    scheduled   what the ladder asked for
    actual      days since the schedule was last served
    effective   days since the secret was in front of a person at all

  Every figure in rote stats is computed from the whole record rather than read
  back off the line that carries it, so two machines that wrote without having
  seen each other cannot each claim a full interval for one real gap.

  The streak in rote stats counts unaided passes at the cap interval or longer,
  back from the most recent drill. A pass at a shorter interval is not evidence
  either way and is stepped over; a miss ends the run. It is reported and never
  judged.";

const RECORDS: &str = "\
RECORDS

  One JSON object per line, appended and never rewritten, one file per machine.
  Every record carries the schema, the instant, the drill day, the machine, the
  hostname it wore, its position in its own chain, the digest of the line before
  it, and one event.

  Five kinds of event: enroll, rotate, attach, retire, capture.

  A capture is one typed sample. It records the lineage and the engram, the
  sitting, which sample within the drill, the occasion, whether it was aided,
  the outcome, time to the first keystroke, time to submit, corrections, pastes
  accepted, pastes refused, the three intervals, and the rung either side.

  Corrections counts keystrokes that removed something, one apiece, whatever
  each of them removed. It is a fact about the typing and never about the
  secret: not its length, not a prefix, not a character class.

  The two paste counts are every paste the prompt saw, and neither is a count
  of lookups: a refused paste is one steered onto a channel that arrives as
  ordinary typing.

  Two occasions: review, which the schedule asked for, and practice, which it
  did not. Aided rides beside the occasion as a flag rather than replacing it,
  so an aided review is still a review.

  Five outcomes: pass, fail, blank, skip, abandoned.

  Nothing derived from the secret is recorded: not its length, not a prefix, not
  a character class. The digest chain is tamper evidence and corruption
  detection rather than a control. It catches edits and deletions inside a
  chain. It does not catch a chain being truncated at the end or deleted
  outright.";

const FILES: &str = "\
FILES

  ~/Trove/ark/rote/chains/           one append-only chain per machine
  ~/.local/state/rote/verifiers.toml the verifiers this machine holds
  ~/.local/state/rote/stamp          touched after any write; what the shell reminder keys on
  ~/.local/state/rote/chain.lock     guards this machine's chain
  ~/.local/state/reliquary/flagship  the marker rote reads and never writes
  ~/.config/rote/config.toml         optional

  The record is durable and goes offsite. The verifiers never leave the machine:
  a verifier is an offline-attackable confirmation oracle, and it is regenerable
  by typing the secret again, so it belongs where losing the local copy loses
  nothing.

  An engram with no verifier here is a first-class state, not a corruption. It
  is what a restore onto a new machine looks like, and rote attach is how it
  comes back.

  Config keys: root, state, rollover-hour, max-attempts, ladder.

  Environment: ROTE_ROOT, ROTE_STATE, ROTE_CONFIG, ROTE_UI, ROTE_HOST,
  ROTE_FLAGSHIP, ROTE_MACHINE, ROTE_NOW.";

const MACHINES: &str = "\
MACHINES

  A machine has an identity and a label. The identity is derived from the
  platform's own hardware identifier, hashed and truncated, so it is stable
  across a reinstall and there is no file holding it that could be copied onto a
  second computer. The label is the hostname, which is mutable and is never an
  identity. Both are recorded on every line.

  Each machine writes its own chain. An append-only hash chain is not a
  mergeable structure, so two machines never share one file: the corpus is the
  merge of all of them, ordered by instant, then machine, then position. A
  flagship handover is a new file rather than a fork in an old one.

  Only the flagship writes. The marker is a file rote reads and never creates.
  An empty marker authorises whoever holds it; a marker with a machine identity
  in it authorises only that machine, which is how a copied marker fails loudly
  instead of quietly admitting a second writer. It is an accident guard and not
  a lock.

  Attachment is per machine. The record carries the attach events, permanently;
  which verifiers this machine holds is a local fact. The two can legitimately
  disagree, and neither is authoritative over the other.

  rote machines shows every machine that has written, and which one this is.";

const STDIN: &str = "\
STDIN

  One rule: a pipe is asked for each distinct secret exactly once. Double entry
  exists to catch a typo at a keyboard, and a pipe cannot mistype.

    rote enroll --stdin  one line, the new secret
    rote attach --stdin  one line, the secret this engram holds
    rote rotate --stdin  one line, the new secret

  Refused when stdin is a terminal. The threat is an echoing read: a secret
  typed into one lands on the screen and in scrollback.

  What the pipe must not be is a command line. No stream check can see one, so
  that discipline is yours: a vault at the other end, never an echo.

  The drill accepts no stdin at all.";

const EXIT: &str = "\
EXIT

  0   nothing to report. Everything due was answered, or nothing was due
  1   something was missed, or the doctor found something soft
  2   a sitting was abandoned, or the doctor found something broken
  3   rote refused or could not run

  A sitting that only skipped dormant lineages exits 0: nothing was judged
  either way. Refusing to write because this machine is not the flagship is 3.";

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

    #[test]
    fn the_root_help_advertises_every_topic_that_exists() {
        for (name, _) in TOPICS {
            assert!(
                crate::cli::ROOT_AFTER_LONG_HELP.contains(name),
                "{name} is not advertised"
            );
        }
    }

    #[test]
    fn every_occasion_and_landing_is_spelled_in_the_records_topic() {
        let body = topic("records").unwrap();
        for occasion in [
            crate::ladder::Occasion::Review,
            crate::ladder::Occasion::Practice,
        ] {
            assert!(body.contains(occasion.word()), "{}", occasion.word());
        }
    }
}
