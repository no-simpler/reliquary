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

use crate::secret::Secret;

/// A key, as a dialog understands it. Anything else the terminal sends is
/// nothing: only a character reaches the buffer, so an escape sequence cannot be
/// typed into a secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// A bracketed paste arrived. It is refused: a secret must be typed, and a
    /// paste is a lookup wearing a drill's clothes.
    Paste,
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

    /// Show a card for a moment, then carry on.
    ///
    /// What a refusal gets instead of a mark that stays on the screen: the eye
    /// catches the change, and the next glance is not re-reading it.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be written to.
    fn flash(&mut self, card: &Card) -> Result<()>;

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
    /// Pastes refused.
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

    /// What the field looks like while it is being said.
    pub fn tone(self) -> card::Tone {
        match self {
            Self::None => card::Tone::Calm,
            Self::Paste | Self::Full => card::Tone::Alarm,
        }
    }
}

/// Read one secret, blind.
///
/// The only loop in the binary that accepts a typed secret. `card` is asked for
/// what to draw whenever the screen has to change, which is what lets a drill
/// and an enrollment share this while looking like themselves.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn read_secret(
    console: &mut dyn Console,
    lookup: bool,
    card: &mut dyn FnMut(Refusal) -> Card,
) -> Result<Typed> {
    let mut secret = Secret::new();
    let mut ttfk: Option<u64> = None;
    let mut corrections = 0u32;
    let mut refused = 0u32;
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
                if !secret.push(character) {
                    refuse(console, card, Refusal::Full)?;
                }
            }
            Key::Backspace => {
                if secret.pop() {
                    corrections = corrections.saturating_add(1);
                }
            }
            Key::Clear => {
                if !secret.is_empty() {
                    secret.clear();
                    corrections = corrections.saturating_add(1);
                }
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
            Key::Paste => {
                refused = refused.saturating_add(1);
                refuse(console, card, Refusal::Paste)?;
            }
        }
    }
}

/// A flash and back to calm: the shape every refusal takes.
fn refuse(
    console: &mut dyn Console,
    card: &mut dyn FnMut(Refusal) -> Card,
    refusal: Refusal,
) -> Result<()> {
    let drawn = card(refusal);
    console.flash(&drawn)?;
    let calm = card(Refusal::None);
    console.paint(&calm)
}

/// Lines a one-secret dialog holds: a heading, air, the intention, the three of
/// the field, air, and the line under it.
const ASK_SLOTS: usize = 8;

/// One prompt, as the person in front of it reads it.
pub struct Ask<'a> {
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
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn ask(console: &mut dyn Console, today: jiff::civil::Date, prompt: &Ask<'_>) -> Result<Typed> {
    let style = console.style();
    let mut build = |refusal: Refusal| {
        let mut drawn = Card::new("rote", card::stamp(today), style);
        drawn.reserve(ASK_SLOTS);
        drawn.say(prompt.heading, Tint::Bold).gap();
        // Unconditional, so an empty intention pads its line rather than moving
        // the field up into it.
        drawn.say(prompt.intention, Tint::Dim);
        drawn.entry(0, card::Reveal::Blind, refusal.tone());
        drawn.gap();
        let under = refusal.status().or(prompt.status).unwrap_or("");
        drawn.say(under, Tint::Dim);
        drawn
    };
    let opening = build(Refusal::None);
    console.anchor(opening.height());
    console.paint(&opening)?;
    read_secret(console, false, &mut build)
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
    use crate::secret::CAPACITY;

    /// A scripted terminal that records every card it is handed.
    struct Fake {
        keys: Vec<Key>,
        at: usize,
        frames: Vec<Vec<String>>,
        flashes: usize,
        anchored: usize,
    }

    impl Fake {
        fn new(keys: Vec<Key>) -> Self {
            Self {
                keys,
                at: 0,
                frames: Vec::new(),
                flashes: 0,
                anchored: 0,
            }
        }
    }

    impl Input for Fake {
        fn arm(&mut self) {}

        fn next(&mut self) -> anyhow::Result<Option<Key>> {
            let key = self.keys.get(self.at).copied();
            self.at = self.at.saturating_add(1);
            Ok(key)
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

        fn hold(&mut self) -> anyhow::Result<()> {
            Ok(())
        }
    }

    fn typing(text: &str) -> Vec<Key> {
        let mut keys: Vec<Key> = text.chars().map(Key::Char).collect();
        keys.push(Key::Enter);
        keys
    }

    #[test]
    fn a_full_buffer_refuses_the_character_and_says_so() {
        let mut keys: Vec<Key> = std::iter::repeat_n(Key::Char('x'), CAPACITY + 3).collect();
        keys.push(Key::Enter);
        let mut console = Fake::new(keys);
        let mut refusals = Vec::new();
        let typed = read_secret(&mut console, false, &mut |refusal| {
            refusals.push(refusal);
            Card::new("rote", "x", Style::PLAIN)
        })
        .unwrap();
        let Typed::Submitted(entry) = typed else {
            panic!("submitted");
        };
        assert_eq!(
            entry.secret.expose().len(),
            CAPACITY,
            "nothing past the capacity"
        );
        assert_eq!(console.flashes, 3, "each refused character is a flash");
        assert!(refusals.contains(&Refusal::Full));
        assert!(
            refusals.contains(&Refusal::None),
            "and the card calms down after"
        );
    }

    #[test]
    fn an_empty_intention_does_not_move_the_field() {
        let prompt = |intention: &'static str| {
            let mut console = Fake::new(typing("x"));
            let ask_for = Ask {
                heading: "enroll a",
                intention,
                status: None,
            };
            ask(&mut console, date(2026, 9, 10), &ask_for).unwrap();
            let first = console.frames.first().unwrap().clone();
            first
                .iter()
                .position(|line| line.contains('╭') && !line.contains("rote"))
                .unwrap()
        };
        assert_eq!(prompt("type it twice"), prompt(""));
    }

    #[test]
    fn the_status_sits_under_the_field_and_a_refusal_overrides_it() {
        let mut console = Fake::new(vec![Key::Paste, Key::Char('x'), Key::Enter]);
        let ask_for = Ask {
            heading: "enroll a",
            intention: "again",
            status: Some("the two entries differ"),
        };
        ask(&mut console, date(2026, 9, 10), &ask_for).unwrap();
        let opening = console.frames.first().unwrap().join("\n");
        assert!(opening.contains("the two entries differ"), "{opening}");
        let flashed = console.frames.get(1).unwrap().join("\n");
        assert!(flashed.contains("do not paste"), "{flashed}");
        assert!(!flashed.contains("entries differ"), "{flashed}");
        let calm = console.frames.get(2).unwrap().join("\n");
        assert!(calm.contains("the two entries differ"), "{calm}");
    }
}
