//! The screen, and everything that reaches a person through it.
//!
//! Two commands take a secret and one drills them, and all three are worthless
//! to a script: they are modal dialogs that have to be typed at. What they share
//! lives here — the key mapping, the card, the terminal, and the one loop that
//! reads a secret — so that the drill and an enrolment cannot drift apart in how
//! they look or in what they will accept.

pub mod card;
pub mod term;

use anyhow::Result;

pub use card::Card;

use crate::secret::Secret;

/// A key, as a dialog understands it. Anything else the terminal sends is
/// nothing: only a character reaches the buffer, so an escape sequence cannot be
/// typed into a secret.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    /// A printable character.
    Char(char),
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
    fn color(&self) -> bool;

    /// Declare how tall the card will ordinarily be, so the dialog can be
    /// placed once and stay there.
    ///
    /// A card taller than this grows downward from the same top edge rather
    /// than re-centring, because a dialog that moves as its content changes is
    /// the vertical form of the width jitter a fixed width exists to remove.
    fn anchor(&mut self, height: usize);

    /// Draw a card.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be written to.
    fn paint(&mut self, card: &Card) -> Result<()>;

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

/// Something the entry loop needs said before it can carry on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Nudge {
    /// A paste arrived and was refused.
    PasteRefused,
}

/// What one prompt produced.
pub enum Typed {
    /// Submitted, for whatever it is worth.
    Submitted(Entry),
    /// Passed over.
    Skipped,
    /// Abandoned, or the input ended.
    Aborted,
}

/// One submitted entry, before anything has judged it.
pub struct Entry {
    /// What was typed.
    pub secret: Secret,
    /// Milliseconds to the first keystroke.
    pub ttfk_ms: Option<u64>,
    /// Milliseconds from the first keystroke to submission.
    pub total_ms: Option<u64>,
    /// Backspaces and clears that removed something.
    pub corrections: u32,
    /// Pastes refused.
    pub paste_refused: u32,
}

/// Read one secret, blind.
///
/// The only loop in the binary that accepts a typed secret. `card` is asked for
/// what to draw whenever the screen has to change, which is what lets a drill
/// and an enrolment share this while looking like themselves.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn read_secret(
    console: &mut dyn Console,
    card: &mut dyn FnMut(Option<Nudge>) -> Card,
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
                secret.push(character);
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
                    ttfk_ms: ttfk,
                    total_ms: total,
                    corrections,
                    paste_refused: refused,
                }));
            }
            Key::Escape => return Ok(Typed::Skipped),
            Key::Interrupt => return Ok(Typed::Aborted),
            Key::Paste => {
                refused = refused.saturating_add(1);
                let drawn = card(Some(Nudge::PasteRefused));
                console.paint(&drawn)?;
            }
        }
    }
}

/// One prompt, as the person in front of it reads it.
pub struct Ask<'a> {
    /// What is being asked about.
    pub heading: &'a str,
    /// What this prompt is for, said where it applies.
    pub intention: &'a str,
}

/// Put one prompt on the screen and read the answer.
///
/// The same three lines wherever a secret is typed — a heading, an intention
/// and the shared entry line — so an enrolment and a drill are recognisably the
/// same instrument.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn ask(console: &mut dyn Console, today: jiff::civil::Date, prompt: &Ask<'_>) -> Result<Typed> {
    let color = console.color();
    let build = |nudge: Option<Nudge>| {
        let mut drawn = Card::new("rote", card::stamp(today), color);
        drawn.say(prompt.heading, card::BOLD);
        if !prompt.intention.is_empty() {
            drawn.say(prompt.intention, card::DIM);
        }
        drawn.line(card::entry_line(0, card::Reveal::Blind, "", color));
        if nudge == Some(Nudge::PasteRefused) {
            drawn
                .gap()
                .say("a paste was refused — it has to be typed", card::YELLOW);
        }
        drawn
    };
    let opening = build(None);
    console.anchor(opening.height());
    console.paint(&opening)?;
    read_secret(console, &mut |nudge| build(nudge))
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
    text: &str,
    code: &str,
) -> Result<()> {
    let color = console.color();
    let mut drawn = Card::new("done", card::stamp(today), color);
    drawn.say(text, code);
    drawn.gap().say("press any key", card::DIM);
    console.paint(&drawn)?;
    console.hold()
}
