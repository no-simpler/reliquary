//! The screen, and everything that reaches a person through it.
//!
//! Two commands take a secret and one drills them, and all three are worthless
//! to a script: they are modal dialogs that have to be typed at. What they share
//! lives here — the key mapping, the card, the terminal, and the one loop that
//! reads a secret — so that the drill and an enrollment cannot drift apart in how
//! they look or in what they will accept.

pub mod card;
pub mod term;

use anyhow::Result;
use relic_core::style::{Style, Tint};

pub use card::Card;

use crate::secret::{Pasted, Secret};

/// A key, as a dialog understands it. Anything else the terminal sends is
/// nothing: only a character reaches the buffer, so an escape sequence cannot be
/// typed into a secret.
///
/// Neither `Copy` nor `Clone`, because one variant carries what was pasted and
/// a secret that can be duplicated in passing is a secret with copies nothing
/// wipes.
#[derive(Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable character.
    Char(char),
    /// Go and look it up, then type it as an aided entry.
    Lookup,
    /// Remove the last character.
    Backspace,
    /// Submit.
    Enter,
    /// Pass over this prompt.
    Escape,
    /// Abandon.
    Interrupt,
    /// Start the entry over.
    Clear,
    /// A bracketed paste arrived, carrying what it carried. Whether the text
    /// reaches the field is the prompt's to say: it is taken wherever nothing
    /// is being measured, and refused at a cold try, which is the one prompt
    /// that is. Held so that dropping it wipes it.
    Paste(Pasted),
}

/// Where keys come from, and the stopwatch that runs beside them.
pub trait Input {
    /// Start the stopwatch for a fresh prompt.
    fn arm(&mut self);

    /// The next key, or `None` when the input has ended.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be read.
    fn next(&mut self) -> Result<Option<Key>>;

    /// Milliseconds since [`Input::arm`].
    fn elapsed_ms(&self) -> u64;
}

/// Where the card is drawn.
pub trait Screen {
    /// Whether colour is wanted, so a caller can build its pieces.
    fn style(&self) -> Style;

    /// Declare how tall the card will ordinarily be, so the dialog can be
    /// placed once and stay there.
    ///
    /// A card taller than this grows downward from the same top edge rather
    /// than re-centering, because a dialog that moves as its content changes is
    /// the vertical form of the width jitter a fixed width exists to remove.
    fn anchor(&mut self, height: usize);

    /// Draw a card.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be written to.
    fn paint(&mut self, card: &Card) -> Result<()>;

    /// Pulse a card, and return when it has been seen or interrupted.
    ///
    /// What a refusal's *colour* gets instead of a mark that stays on the
    /// screen: the eye catches the change, and the next glance is not
    /// re-reading it. What the refusal said is not on this clock — see
    /// `refuse`.
    ///
    /// It is a ceiling and never a wait. Anything typed into it ends it and is
    /// handed on rather than swallowed, because the program is idle here and
    /// could have acted on that key.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be written to.
    fn flash(&mut self, card: &Card) -> Result<()>;

    /// Throw away whatever was typed while the program could not act.
    ///
    /// The other half of the policy [`Screen::flash`] holds. Where the program
    /// is idle a keystroke is delivered; where it is busy — minting or checking
    /// a verifier, which is deliberately slow — it is dropped. Otherwise a key
    /// struck into the pause arrives at whatever comes next and dismisses it
    /// before it has been read, or lands in the field for a different secret.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be read.
    fn drain(&mut self) -> Result<()>;

    /// Hold the finished card until a key is pressed.
    ///
    /// A dialog leaves no scrollback, so this is the only chance to read it.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be read.
    fn hold(&mut self) -> Result<()>;
}

/// Both halves of a terminal, so a loop borrows it once.
pub trait Console: Input + Screen {}

impl<T: Input + Screen> Console for T {}

/// What one prompt produced.
pub enum Typed {
    /// Submitted, for whatever it is worth.
    Submitted(Entry),
    /// Passed over.
    Skipped,
    /// Abandoned, or the input ended.
    Aborted,
    /// The reader asked for the lookup, which only a caller that has already
    /// recorded a cold attempt puts on offer.
    Lookup,
}

/// One submitted entry, before anything has judged it.
pub struct Entry {
    /// What was typed.
    pub secret: Secret,
    /// Everything about the entry that is not the secret.
    pub timings: Timings,
}

impl Entry {
    /// Drop the secret and keep what is safe to carry around.
    ///
    /// The one way to part an entry from its secret, so the buffer is gone
    /// before anything records what typing it looked like.
    pub fn forget(self) -> Timings {
        drop(self.secret);
        self.timings
    }
}

/// What one entry cost, none of which is the secret.
#[derive(Clone, Copy, Debug, Default)]
pub struct Timings {
    /// Milliseconds to the first keystroke.
    pub ttfk_ms: Option<u64>,
    /// Milliseconds from the first keystroke to submission.
    pub total_ms: Option<u64>,
    /// Backspaces and clears that removed something.
    pub corrections: u32,
    /// Pastes that reached the field.
    pub paste_accepted: u32,
    /// Pastes that did not.
    pub paste_refused: u32,
}

/// Why the card is being redrawn mid-entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Nothing was refused; this is the resting card.
    None,
    /// A paste arrived and was refused.
    Paste,
    /// The buffer is full and a character was refused. Said rather than
    /// swallowed: a secret silently cut short would enroll a verifier for
    /// something nobody typed, which is exactly what the pipe path refuses.
    Full,
}

impl Refusal {
    /// What the line under the field says about it, or nothing.
    pub fn status(self) -> Option<&'static str> {
        match self {
            Self::None => None,
            Self::Paste => Some("type it, do not paste it"),
            Self::Full => Some("that is longer than a secret this tool will take"),
        }
    }
}

/// Read one secret, blind.
///
/// The only loop in the binary that accepts a typed secret. `card` is asked for
/// what to draw whenever the screen has to change, which is what lets a drill
/// and an enrollment share this while looking like themselves.
///
/// `lookup` says whether the vault is on offer here; `paste` says whether text
/// from one is taken. They are separate because one prompt has both and one has
/// neither.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn read_secret(
    console: &mut dyn Console,
    lookup: bool,
    paste: bool,
    resting: card::Tone,
    card: &mut dyn FnMut(Refusal, card::Tone) -> Card,
) -> Result<Typed> {
    let mut secret = Secret::new();
    let mut ttfk: Option<u64> = None;
    let mut corrections = 0u32;
    let mut accepted = 0u32;
    let mut refused = 0u32;
    let mut standing = Refusal::None;
    console.arm();
    loop {
        let Some(key) = console.next()? else {
            return Ok(Typed::Aborted);
        };
        match key {
            Key::Char(character) => {
                if ttfk.is_none() {
                    ttfk = Some(console.elapsed_ms());
                }
                if secret.push(character) {
                    moved_on(console, card, &mut standing, resting)?;
                } else {
                    standing = Refusal::Full;
                    refuse(console, card, Refusal::Full, resting)?;
                }
            }
            Key::Backspace => {
                if secret.pop() {
                    corrections = corrections.saturating_add(1);
                }
                moved_on(console, card, &mut standing, resting)?;
            }
            Key::Clear => {
                if !secret.is_empty() {
                    secret.clear();
                    corrections = corrections.saturating_add(1);
                }
                moved_on(console, card, &mut standing, resting)?;
            }
            Key::Enter => {
                let elapsed = console.elapsed_ms();
                let total = ttfk.map(|start| elapsed.saturating_sub(start));
                return Ok(Typed::Submitted(Entry {
                    secret,
                    timings: Timings {
                        ttfk_ms: ttfk,
                        total_ms: total,
                        corrections,
                        paste_accepted: accepted,
                        paste_refused: refused,
                    },
                }));
            }
            Key::Escape => return Ok(Typed::Skipped),
            Key::Interrupt => return Ok(Typed::Aborted),
            Key::Lookup if lookup => return Ok(Typed::Lookup),
            // Unoffered, so it is nothing at all rather than a key that
            // sometimes works.
            Key::Lookup => {}
            // Not a keystroke, so it starts no stopwatch and stops none. The
            // text is dropped at the end of either arm, which wipes it.
            Key::Paste(text) if paste => {
                if secret.push_str(text.expose()) {
                    accepted = accepted.saturating_add(1);
                    moved_on(console, card, &mut standing, resting)?;
                } else {
                    refused = refused.saturating_add(1);
                    standing = Refusal::Full;
                    refuse(console, card, Refusal::Full, resting)?;
                }
            }
            Key::Paste(_) => {
                refused = refused.saturating_add(1);
                standing = Refusal::Paste;
                refuse(console, card, Refusal::Paste, resting)?;
            }
        }
    }
}

/// The shape every refusal takes: a pulse, and something that stays said.
///
/// The two have different lifetimes on purpose. The red border is a pulse
/// because nothing red may sit on the screen making every later glance re-read
/// it. What the refusal *said* stays, because it is the half a person has to
/// read, and a message timed out from under a reader is a message that was not
/// delivered. [`moved_on`] takes it down.
fn refuse(
    console: &mut dyn Console,
    card: &mut dyn FnMut(Refusal, card::Tone) -> Card,
    refusal: Refusal,
    resting: card::Tone,
) -> Result<()> {
    let alarmed = card(refusal, card::Tone::Alarm);
    console.flash(&alarmed)?;
    let settled = card(refusal, resting);
    console.paint(&settled)
}

/// Take down what a refusal said, the moment the reader carries on typing.
///
/// Starting to type is the signal: it says the message was read, or that it no
/// longer matters, and either way it is no longer what the reader is doing. No
/// timer can know that, which is why there is not one.
///
/// The repaint is explicit because the field is blind — an ordinary keystroke
/// changes nothing on the screen, so nothing else would redraw it.
fn moved_on(
    console: &mut dyn Console,
    card: &mut dyn FnMut(Refusal, card::Tone) -> Card,
    standing: &mut Refusal,
    resting: card::Tone,
) -> Result<()> {
    if *standing == Refusal::None {
        return Ok(());
    }
    *standing = Refusal::None;
    let settled = card(Refusal::None, resting);
    console.paint(&settled)
}

/// Lines a one-secret dialog holds: a heading, air, the intention, the three of
/// the field, air, and the line under it.
const ASK_SLOTS: usize = 8;

/// One prompt, as the person in front of it reads it.
pub struct Ask<'a> {
    /// What the card is titled, on the border above the heading.
    ///
    /// A title carries where colour cannot: it survives a terminal with none,
    /// and a reader who arrived here by accident reads it before anything else.
    pub title: &'static str,
    /// Whether anything here can check what is typed. See [`card::Tone`].
    pub resting: card::Tone,
    /// What is being asked about.
    pub heading: &'a str,
    /// What this prompt is for, said where it applies.
    pub intention: &'a str,
    /// What the line under the field says while nothing is being refused: the
    /// last round's verdict, or nothing.
    pub status: Option<&'a str>,
}

/// Put one prompt on the screen and read the answer.
///
/// The same three lines wherever a secret is typed — a heading, an intention
/// and the shared entry line — so an enrollment and a drill are recognizably the
/// same instrument.
///
/// Every prompt that arrives here takes a paste and offers no lookup: nothing
/// typed at one is measured, and there is nothing here to consult a vault
/// against.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn ask(console: &mut dyn Console, today: jiff::civil::Date, prompt: &Ask<'_>) -> Result<Typed> {
    let style = console.style();
    let intention = if prompt.resting == card::Tone::Unchecked {
        Tint::Yellow
    } else {
        Tint::Dim
    };
    let mut build = |refusal: Refusal, tone: card::Tone| {
        let mut drawn = Card::new(prompt.title, card::stamp(today), style);
        drawn.reserve(ASK_SLOTS);
        drawn.say(prompt.heading, Tint::Bold).gap();
        // Unconditional, so an empty intention pads its line rather than moving
        // the field up into it.
        drawn.say(prompt.intention, intention);
        drawn.entry(0, card::Reveal::Blind, tone);
        drawn.gap();
        let under = refusal.status().or(prompt.status).unwrap_or("");
        drawn.say(under, Tint::Dim);
        drawn
    };
    let opening = build(Refusal::None, prompt.resting);
    console.anchor(opening.height());
    console.paint(&opening)?;
    read_secret(console, false, true, prompt.resting, &mut build)
}

/// What the field says while a verifier is being minted or checked.
pub const WORKING: &str = "checking";

/// Hold the screen still while something slow happens, and swallow what is
/// typed at it.
///
/// argon2id at these parameters is half a second by design — a verifier that
/// is cheap to check is a verifier that is cheap to attack. The work is right;
/// only its presentation was wrong. Two things are owed to a reader here: the
/// input box goes *before* the wait rather than after it, and nothing struck
/// into the wait survives it.
///
/// # Errors
///
/// When the terminal cannot be written to, or the work itself fails.
pub fn while_working<T>(
    console: &mut dyn Console,
    card: &Card,
    work: impl FnOnce() -> Result<T>,
) -> Result<T> {
    console.paint(card)?;
    let done = work();
    // Drained whether the work succeeded or not: what follows a failure is a
    // card to read just as much as what follows a success.
    console.drain()?;
    done
}

/// The card a prompt shows while it is waiting on its own verifier.
///
/// Built from the same parts and the same reservation as [`ask`], so the box
/// does not move between the two.
pub fn waiting(console: &dyn Console, today: jiff::civil::Date, prompt: &Ask<'_>) -> Card {
    let mut drawn = Card::new(prompt.title, card::stamp(today), console.style());
    drawn.reserve(ASK_SLOTS);
    drawn.say(prompt.heading, Tint::Bold).gap();
    drawn.say(prompt.intention, Tint::Dim);
    drawn.waiting(WORKING);
    drawn.gap();
    drawn.say("", Tint::Dim);
    drawn
}

/// Ask a yes-or-no question and read one keystroke.
///
/// One key, no enter: this is asked on the most frequent path there is, and a
/// question that wants a word and a return is a question that will be resented
/// daily. Only `y` says yes, so a stray key never starts something.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn offer(
    console: &mut dyn Console,
    today: jiff::civil::Date,
    heading: &str,
    question: &str,
) -> Result<bool> {
    let style = console.style();
    let mut drawn = Card::new("rote", card::stamp(today), style);
    drawn.reserve(ASK_SLOTS);
    drawn.say(heading, Tint::Bold).gap();
    drawn.say(question, Tint::Dim);
    drawn.pad_to(ASK_SLOTS.saturating_sub(1));
    drawn.say("y to practice · any other key to leave it", Tint::Dim);
    console.anchor(drawn.height());
    console.paint(&drawn)?;
    console.arm();
    Ok(match console.next()? {
        Some(Key::Char('y' | 'Y')) => true,
        None
        | Some(
            Key::Char(_)
            | Key::Enter
            | Key::Escape
            | Key::Interrupt
            | Key::Backspace
            | Key::Clear
            | Key::Lookup
            | Key::Paste(_),
        ) => false,
    })
}

/// The closing card: one outcome, held until it has been read.
///
/// A dialog leaves nothing in scrollback, so this is where an outcome is read
/// or it is not read at all.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn outcome(
    console: &mut dyn Console,
    today: jiff::civil::Date,
    heading: &str,
    text: &str,
    tint: Tint,
) -> Result<()> {
    let style = console.style();
    let mut drawn = Card::new("done", card::stamp(today), style);
    drawn.reserve(ASK_SLOTS);
    // The heading is where it always was, and what happened is where everything
    // else that changes has been: on the line under the field.
    drawn.say(heading, Tint::Bold);
    drawn.pad_to(ASK_SLOTS.saturating_sub(1));
    drawn.split(text, "press any key", tint);
    console.paint(&drawn)?;
    console.hold()
}

#[cfg(test)]
mod tests {
    use jiff::civil::date;
    use relic_core::style::Style;

    use super::{Ask, Card, Input, Key, Refusal, Screen, Typed, ask, read_secret};
    use crate::secret::{CAPACITY, Pasted};

    /// A scripted terminal that records every card it is handed.
    ///
    /// The keys are consumed rather than copied, because one of them carries a
    /// secret and [`Key`] is deliberately neither `Copy` nor `Clone`.
    struct Fake {
        keys: std::vec::IntoIter<Key>,
        frames: Vec<Vec<String>>,
        flashes: usize,
        drains: usize,
        anchored: usize,
    }

    impl Fake {
        fn new(keys: Vec<Key>) -> Self {
            Self {
                keys: keys.into_iter(),
                frames: Vec::new(),
                flashes: 0,
                drains: 0,
                anchored: 0,
            }
        }
    }

    impl Input for Fake {
        fn arm(&mut self) {}

        fn next(&mut self) -> anyhow::Result<Option<Key>> {
            Ok(self.keys.next())
        }

        fn elapsed_ms(&self) -> u64 {
            0
        }
    }

    impl Screen for Fake {
        fn style(&self) -> Style {
            Style::PLAIN
        }

        fn anchor(&mut self, height: usize) {
            self.anchored = height;
        }

        fn paint(&mut self, card: &Card) -> anyhow::Result<()> {
            self.frames.push(card.render());
            Ok(())
        }

        fn flash(&mut self, card: &Card) -> anyhow::Result<()> {
            self.flashes = self.flashes.saturating_add(1);
            self.paint(card)
        }

        fn drain(&mut self) -> anyhow::Result<()> {
            self.drains = self.drains.saturating_add(1);
            Ok(())
        }

        fn hold(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn typing(text: &str) -> Vec<Key> {
        let mut keys: Vec<Key> = text.chars().map(Key::Char).collect();
        keys.push(Key::Enter);
        keys
    }

    fn paste(text: &str) -> Key {
        Key::Paste(Pasted::new(text.to_owned()))
    }

    /// Read one secret from a scripted terminal under one paste policy.
    fn read(keys: Vec<Key>, allowed: bool) -> (Typed, Vec<Refusal>, usize) {
        let (typed, drawn, flashes) = read_drawing(keys, allowed);
        let refusals = drawn.into_iter().map(|(refusal, _)| refusal).collect();
        (typed, refusals, flashes)
    }

    /// Every card the loop asked for, with the tone it asked for it in.
    fn read_drawing(
        keys: Vec<Key>,
        allowed: bool,
    ) -> (Typed, Vec<(Refusal, super::card::Tone)>, usize) {
        let mut console = Fake::new(keys);
        let mut drawn = Vec::new();
        let typed = read_secret(
            &mut console,
            false,
            allowed,
            super::card::Tone::Calm,
            &mut |refusal, tone| {
                drawn.push((refusal, tone));
                Card::new("rote", "x", Style::PLAIN)
            },
        )
        .unwrap();
        (typed, drawn, console.flashes)
    }

    fn submitted(typed: Typed) -> super::Entry {
        match typed {
            Typed::Submitted(entry) => entry,
            Typed::Skipped | Typed::Aborted | Typed::Lookup => panic!("submitted"),
        }
    }

    #[test]
    fn a_full_buffer_refuses_the_character_and_says_so() {
        let mut keys: Vec<Key> = (0..CAPACITY + 3).map(|_| Key::Char('x')).collect();
        keys.push(Key::Enter);
        let (typed, refusals, flashes) = read(keys, false);
        let entry = submitted(typed);
        assert_eq!(
            entry.secret.expose().len(),
            CAPACITY,
            "nothing past the capacity"
        );
        assert_eq!(flashes, 3, "each refused character is a flash");
        // Nothing here ever lands, so nothing ever says the reader moved on:
        // what was refused stays said until something does.
        assert!(refusals.iter().all(|refusal| *refusal == Refusal::Full));
    }

    #[test]
    fn a_paste_is_refused_where_something_is_being_measured() {
        let (typed, refusals, flashes) = read(vec![paste("hunter2"), Key::Enter], false);
        let entry = submitted(typed);
        assert!(
            entry.secret.is_empty(),
            "nothing pasted reaches a field that refuses a paste"
        );
        let timings = entry.forget();
        assert_eq!(timings.paste_refused, 1);
        assert_eq!(timings.paste_accepted, 0);
        assert_eq!(flashes, 1, "a refusal is said rather than swallowed");
        assert!(refusals.contains(&Refusal::Paste));
    }

    #[test]
    fn a_paste_reaches_the_field_where_nothing_is() {
        let (typed, refusals, flashes) = read(vec![paste("hunter2"), Key::Enter], true);
        let entry = submitted(typed);
        assert_eq!(entry.secret.expose(), b"hunter2");
        let timings = entry.forget();
        assert_eq!(timings.paste_accepted, 1);
        assert_eq!(timings.paste_refused, 0);
        assert_eq!(flashes, 0);
        assert!(
            refusals.is_empty(),
            "nothing was refused, so nothing is said"
        );
    }

    #[test]
    fn a_paste_is_not_a_keystroke_and_times_nothing() {
        let entry = submitted(read(vec![paste("hunter2"), Key::Enter], true).0);
        let timings = entry.forget();
        assert_eq!(
            timings.ttfk_ms, None,
            "an entry with no keystroke in it claims no latency"
        );
        assert_eq!(timings.total_ms, None);
        assert_eq!(timings.corrections, 0);
    }

    #[test]
    fn a_paste_lands_beside_what_was_already_typed() {
        let entry = submitted(read(vec![Key::Char('a'), paste("bc"), Key::Enter], true).0);
        assert_eq!(entry.secret.expose(), b"abc");
    }

    #[test]
    fn a_paste_that_will_not_fit_is_refused_whole_rather_than_half_entered() {
        let long = "x".repeat(CAPACITY);
        let (typed, refusals, flashes) = read(vec![Key::Char('a'), paste(&long), Key::Enter], true);
        let entry = submitted(typed);
        assert_eq!(
            entry.secret.expose(),
            b"a",
            "half a secret in the field is the one outcome worse than none"
        );
        let timings = entry.forget();
        assert_eq!(timings.paste_accepted, 0);
        assert_eq!(
            timings.paste_refused, 1,
            "it is still a paste that was seen"
        );
        assert_eq!(flashes, 1);
        assert!(refusals.contains(&Refusal::Full));
    }

    #[test]
    fn an_empty_intention_does_not_move_the_field() {
        let prompt = |intention: &'static str| {
            let mut console = Fake::new(typing("x"));
            let ask_for = Ask {
                title: "rote",
                resting: super::card::Tone::Calm,
                heading: "enroll a",
                intention,
                status: None,
            };
            ask(&mut console, date(2026, 9, 10), &ask_for).unwrap();
            let first = console.frames.first().unwrap().clone();
            first
                .iter()
                .enumerate()
                .skip(1)
                .find(|(_, line)| line.contains('╭'))
                .map(|(index, _)| index)
                .unwrap()
        };
        assert_eq!(prompt("type it twice"), prompt(""));
    }

    #[test]
    fn the_status_sits_under_the_field_and_a_refusal_overrides_it() {
        let long = "x".repeat(CAPACITY + 1);
        let mut console = Fake::new(vec![paste(&long), Key::Char('x'), Key::Enter]);
        let ask_for = Ask {
            title: "rote",
            resting: super::card::Tone::Calm,
            heading: "enroll a",
            intention: "again",
            status: Some("the two entries differ"),
        };
        ask(&mut console, date(2026, 9, 10), &ask_for).unwrap();
        let opening = console.frames.first().unwrap().join("\n");
        assert!(opening.contains("the two entries differ"), "{opening}");
        let flashed = console.frames.get(1).unwrap().join("\n");
        assert!(flashed.contains("longer than a secret"), "{flashed}");
        assert!(!flashed.contains("entries differ"), "{flashed}");
        // The border calms down and the refusal stays said: it is the half a
        // reader has to read, and no clock can know when they have.
        let settled = console.frames.get(2).unwrap().join("\n");
        assert!(settled.contains("longer than a secret"), "{settled}");
        // Typing says they moved on. Only then does the standing status — which
        // is a counter and not a refusal — come back.
        let after = console.frames.get(3).unwrap().join("\n");
        assert!(after.contains("the two entries differ"), "{after}");
        assert!(!after.contains("longer than a secret"), "{after}");
    }

    #[test]
    fn a_prompt_that_takes_a_secret_twice_takes_a_paste() {
        let mut console = Fake::new(vec![paste("hunter2"), Key::Enter]);
        let ask_for = Ask {
            title: "rote",
            resting: super::card::Tone::Calm,
            heading: "enroll a",
            intention: "type it twice",
            status: None,
        };
        let typed = ask(&mut console, date(2026, 9, 10), &ask_for).unwrap();
        let entry = submitted(typed);
        assert_eq!(entry.secret.expose(), b"hunter2");
        assert_eq!(console.flashes, 0);
    }

    #[test]
    fn a_refusal_pulses_the_border_and_leaves_what_it_said_standing() {
        let (_, drawn, _) = read_drawing(vec![paste("hunter2"), Key::Enter], false);
        let tones: Vec<super::card::Tone> = drawn
            .iter()
            .filter(|(refusal, _)| *refusal == Refusal::Paste)
            .map(|(_, tone)| *tone)
            .collect();
        // Said twice: once in the alarm the eye catches, and again in the calm
        // the reader is left to read it in.
        assert_eq!(
            tones,
            vec![super::card::Tone::Alarm, super::card::Tone::Calm],
            "the border comes back and the saying does not go with it"
        );
    }

    #[test]
    fn what_a_refusal_said_is_taken_down_by_the_next_keystroke_and_not_before() {
        let (_, drawn, _) = read_drawing(
            vec![paste("hunter2"), Key::Char('a'), Key::Char('b'), Key::Enter],
            false,
        );
        let refusals: Vec<Refusal> = drawn.into_iter().map(|(refusal, _)| refusal).collect();
        // Refused, said, still said, then the first character takes it down —
        // and the second changes nothing, because there is nothing left to say.
        // Refused, said in alarm, said again in calm, and taken down by the
        // first character that lands. The second draws nothing: there is
        // nothing left to take down.
        assert_eq!(
            refusals,
            vec![Refusal::Paste, Refusal::Paste, Refusal::None]
        );
    }

    #[test]
    fn nothing_typed_at_a_verifier_survives_it() {
        let mut console = Fake::new(Vec::new());
        let card = Card::new("rote", "x", Style::PLAIN);
        let out: usize = super::while_working(&mut console, &card, || Ok(7)).unwrap();
        assert_eq!(out, 7);
        assert_eq!(console.drains, 1);
    }

    #[test]
    fn a_verifier_that_fails_drains_the_keyboard_all_the_same() {
        // What follows a failure is a card to read just as much as what follows
        // a success, and it is dismissed by the same stray keystroke.
        let mut console = Fake::new(Vec::new());
        let card = Card::new("rote", "x", Style::PLAIN);
        let out: anyhow::Result<usize> =
            super::while_working(&mut console, &card, || Err(anyhow::anyhow!("no")));
        assert!(out.is_err());
        assert_eq!(console.drains, 1);
    }
}
