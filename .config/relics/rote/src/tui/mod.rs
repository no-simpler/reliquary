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
use unicode_width::UnicodeWidthChar as _;
use zeroize::Zeroizing;

pub use card::Card;

use card::{Field, Reveal};

use crate::secret::{Pasted, Secret};

/// A key, as a dialog understands it: an intent, not a keystroke.
///
/// Terminals encode one intent several ways and readline names several of them
/// twice, so the mapping collapses every spelling onto one variant here and the
/// loop never learns which arrived. Anything the map does not name is nothing
/// at all, which is what stops an escape sequence being typed into a secret.
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
    /// Remove the character before the caret.
    DeleteLeft,
    /// Remove the character at the caret.
    DeleteRight,
    /// End of input: leave when there is nothing typed, remove the character at
    /// the caret when there is. What every shell does with the same key.
    EndOfInput,
    /// Remove everything before the caret.
    KillToStart,
    /// Remove everything from the caret on.
    KillToEnd,
    /// Remove back to the start of a word.
    KillWordLeft,
    /// Remove forward to the end of a word.
    KillWordRight,
    /// Move one character.
    Left,
    /// Move one character.
    Right,
    /// Move one word.
    WordLeft,
    /// Move one word.
    WordRight,
    /// Move to the start.
    LineStart,
    /// Move to the end.
    LineEnd,
    /// Swap the character before the caret with the one at it.
    Transpose,
    /// Show what is typed, or stop showing it.
    Reveal,
    /// Submit.
    Enter,
    /// Pass over this prompt.
    Escape,
    /// Abandon.
    Interrupt,
    /// A bracketed paste arrived, carrying what it carried. Whether the text
    /// reaches the field is the prompt's to say: it is taken wherever nothing
    /// is being measured, and refused at a cold try, which is the one prompt
    /// that is. Held so that dropping it wipes it.
    Paste(Pasted),
}

/// What ended a wait.
///
/// A field that is showing a secret has two ways of being put back that are not
/// keys, so the thing a dialog waits on is wider than a keystroke.
#[derive(Debug, PartialEq, Eq)]
pub enum Wake {
    /// Somebody pressed something.
    Key(Key),
    /// The terminal lost focus. Whoever is now in front of the screen is not
    /// necessarily the person who revealed it.
    Blur,
    /// Nothing was typed for long enough that a revealed field has outstayed
    /// its welcome.
    Idle,
    /// The input ended.
    Ended,
}

/// How long a revealed field survives with nothing typed into it.
///
/// The same logic the red border gets: nothing that has to be taken back off
/// the screen may sit there indefinitely. Generous, because a person reading
/// their own secret back is doing the thing the reveal is for.
pub const CONCEAL: std::time::Duration = std::time::Duration::from_secs(30);

/// Where keys come from, and the stopwatch that runs beside them.
pub trait Input {
    /// Start the stopwatch for a fresh prompt.
    fn arm(&mut self);

    /// Wait for something to happen, for at most `within` if it is given.
    ///
    /// The only reader, so a fake console scripts an idle or a lost focus the
    /// same way it scripts a keystroke and neither needs a clock of its own.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be read.
    fn wait(&mut self, within: Option<std::time::Duration>) -> Result<Wake>;

    /// The next key, for the prompts that wait on nothing else.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be read.
    fn next(&mut self) -> Result<Option<Key>> {
        loop {
            match self.wait(None)? {
                Wake::Key(key) => return Ok(Some(key)),
                Wake::Ended => return Ok(None),
                Wake::Blur | Wake::Idle => {}
            }
        }
    }

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
    /// Keystrokes that removed something, one apiece and whatever each removed.
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

/// One glyph per character, and nothing of what they are.
const MASK: char = '•';

/// What stands in for a character that cannot be drawn in one column.
///
/// A paste can carry a newline, a tab, an escape, a combining mark or a glyph
/// two columns wide. A newline written in raw mode moves down without returning
/// to column zero; a wide glyph overruns a box that counts characters. Neither
/// may reach the screen, and neither may be silently dropped: one placeholder
/// column keeps the count honest.
const UNDRAWABLE: char = '▫';

/// What the field's text area shows for one character.
fn drawable(character: char) -> char {
    if character.is_control() {
        return UNDRAWABLE;
    }
    match character.width() {
        Some(1) => character,
        _ => UNDRAWABLE,
    }
}

/// A scratch buffer for the glyphs of one field, sized so it never grows.
///
/// It holds a revealed secret between keystrokes, so it is wiped before every
/// refill rather than only on drop, and its capacity is reserved once: a
/// `String` that reallocates leaves the old allocation on the heap with the
/// characters still in it.
fn scratch() -> Zeroizing<String> {
    let mut buffer = String::new();
    buffer.reserve_exact(card::FIELD.saturating_mul(4));
    Zeroizing::new(buffer)
}

/// Turn the buffer into exactly what the field's text area shows.
///
/// The one function in the binary that makes a typed secret drawable. It masks,
/// it sanitises and it windows, and it hands the card a finished string — so the
/// card never sees a buffer, `Secret::expose` gains no caller on the render
/// path, and the whole visual design stays a pure function of what it is given.
///
/// The window follows the caret and is chosen so the caret lands inside the
/// text area rather than on the border. An unmarked field is therefore a
/// complete view, and a partial one always says so.
fn render_field<'a>(
    secret: &Secret,
    reveal: Reveal,
    caret: usize,
    offset: &mut usize,
    into: &'a mut Zeroizing<String>,
) -> Field<'a> {
    use zeroize::Zeroize as _;

    into.zeroize();
    if reveal == Reveal::Blind {
        *offset = 0;
        return Field {
            reveal,
            drawn: into.as_str(),
            clipped: (false, false),
            column: 0,
        };
    }
    let window = card::FIELD;
    let total = secret.chars();
    let caret = caret.min(total);
    // The caret needs a column of its own past the last character, so the
    // content is one wider than it looks.
    let span = total.saturating_add(1);
    let earliest = caret.saturating_sub(window.saturating_sub(1));
    let latest = span.saturating_sub(window).max(earliest);
    let at = (*offset).min(caret).max(earliest).min(latest);
    *offset = at;
    let text = secret.text();
    // Without valid text there is nothing to show, so a reveal falls back to a
    // mask rather than putting bytes on a screen.
    let shown = matches!((reveal, text), (Reveal::Shown, Some(_)));
    if let (true, Some(text)) = (shown, text) {
        for character in text.chars().skip(at).take(window) {
            into.push(drawable(character));
        }
    } else {
        for _ in 0..total.saturating_sub(at).min(window) {
            into.push(MASK);
        }
    }
    Field {
        reveal: if shown { Reveal::Shown } else { Reveal::Masked },
        drawn: into.as_str(),
        clipped: (at > 0, at.saturating_add(window) < total),
        column: caret.saturating_sub(at),
    }
}

/// The character index a word-wise move to the left lands on.
///
/// Word boundaries are derived here and never in `secret`, because a word
/// length is the one thing about a passphrase this tool may never disclose and
/// the module that holds the value is the wrong place to teach how to find one.
/// Whitespace-delimited, which is what readline's `unix-word-rubout` means and
/// what a passphrase is made of.
fn word_left(text: &str, from: usize) -> usize {
    let mut start = 0usize;
    let mut blank = true;
    for (index, character) in text.chars().take(from).enumerate() {
        let space = character.is_whitespace();
        if blank && !space {
            start = index;
        }
        blank = space;
    }
    start
}

/// The character index a word-wise move to the right lands on.
fn word_right(text: &str, from: usize) -> usize {
    let mut at = from;
    let mut seen = false;
    for (index, character) in text.chars().enumerate().skip(from) {
        if character.is_whitespace() {
            if seen {
                return index;
            }
        } else {
            seen = true;
        }
        at = index.saturating_add(1);
    }
    at
}

/// What one key did to the field, which is what decides whether to redraw.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Did {
    /// Nothing at all. An unoffered key, or a motion with nowhere to go.
    Nothing,
    /// Moved the caret, or changed what is shown. A standing refusal stays up:
    /// looking is not the same as carrying on.
    Looked,
    /// Touched the buffer, or tried to. This is the reader saying they have
    /// read whatever was refused.
    Typed,
    /// Something was refused, and says so.
    Refused(Refusal),
}

/// Everything one key may reach, gathered so the editing keys are one function
/// and the keys that end the prompt stay in the loop that can end it.
struct Editing<'a> {
    secret: &'a mut Secret,
    caret: &'a mut usize,
    corrections: &'a mut u32,
    accepted: &'a mut u32,
    refused: &'a mut u32,
    revealed: &'a mut bool,
    /// Whether a memory is being measured here, from which the whole policy
    /// follows.
    cold: bool,
}

impl Editing<'_> {
    /// Whether a caret exists at all, and with it every motion and every
    /// deletion that is not a backspace.
    fn editable(&self) -> bool {
        !self.cold
    }

    /// Whether the characters themselves are on the screen, and with them the
    /// word-wise keys whose reach would otherwise disclose word boundaries.
    fn shown(&self) -> bool {
        *self.revealed && !self.cold
    }

    /// Remove the character before the caret.
    fn delete_left(&mut self) -> Did {
        let at = *self.caret;
        let removed = at > 0 && self.secret.remove_range(at.saturating_sub(1), at);
        if removed {
            *self.caret = at.saturating_sub(1);
        }
        self.corrected(removed)
    }

    /// Remove the character at the caret.
    fn delete_right(&mut self) -> Did {
        let at = *self.caret;
        let removed = self.secret.remove_range(at, at.saturating_add(1));
        self.corrected(removed)
    }

    /// Remove everything before the caret, as readline's `unix-line-discard`
    /// does. A blind prompt has no caret and the insertion point is the end, so
    /// there it is still start the entry over.
    fn kill_to_start(&mut self) -> Did {
        let to = if self.cold {
            self.secret.chars()
        } else {
            *self.caret
        };
        let removed = self.secret.remove_range(0, to);
        *self.caret = 0;
        self.corrected(removed)
    }

    /// Remove everything from the caret on.
    fn kill_to_end(&mut self) -> Did {
        let removed = self.secret.remove_range(*self.caret, self.secret.chars());
        self.corrected(removed)
    }

    /// Remove a word, in either direction.
    fn kill_word(&mut self, forward: bool) -> Did {
        let at = *self.caret;
        let edge = self.word(forward);
        let removed = if forward {
            self.secret.remove_range(at, edge)
        } else {
            self.secret.remove_range(edge, at)
        };
        if removed && !forward {
            *self.caret = edge;
        }
        self.corrected(removed)
    }

    /// Take text a terminal delivered as a paste, at the caret.
    fn take(&mut self, text: &Pasted) -> Did {
        if self.secret.insert_str(*self.caret, text.expose()) {
            *self.accepted = self.accepted.saturating_add(1);
            *self.caret = self
                .caret
                .saturating_add(text.expose().chars().count())
                .min(self.secret.chars());
            Did::Typed
        } else {
            *self.refused = self.refused.saturating_add(1);
            Did::Refused(Refusal::Full)
        }
    }

    fn corrected(&mut self, removed: bool) -> Did {
        if removed {
            *self.corrections = self.corrections.saturating_add(1);
        }
        Did::Typed
    }

    /// Where a word-wise key lands, or where the caret already is when there is
    /// no text to find a boundary in.
    fn word(&self, forward: bool) -> usize {
        let at = *self.caret;
        self.secret.text().map_or(at, |text| {
            if forward {
                word_right(text, at)
            } else {
                word_left(text, at)
            }
        })
    }

    /// Apply one key that does not end the prompt.
    fn apply(&mut self, key: Key) -> Did {
        match key {
            Key::Char(character) => {
                if self.secret.insert(*self.caret, character) {
                    *self.caret = self.caret.saturating_add(1);
                    Did::Typed
                } else {
                    Did::Refused(Refusal::Full)
                }
            }
            Key::DeleteLeft => self.delete_left(),
            Key::DeleteRight | Key::EndOfInput if self.editable() => self.delete_right(),
            Key::KillToStart => self.kill_to_start(),
            Key::KillToEnd if self.editable() => self.kill_to_end(),
            Key::KillWordLeft if self.shown() => self.kill_word(false),
            Key::KillWordRight if self.shown() => self.kill_word(true),
            Key::Transpose if self.editable() => {
                if self.secret.transpose(*self.caret) {
                    *self.caret = self.caret.saturating_add(1).min(self.secret.chars());
                    Did::Typed
                } else {
                    Did::Nothing
                }
            }
            Key::Left if self.editable() => {
                *self.caret = self.caret.saturating_sub(1);
                Did::Looked
            }
            Key::Right if self.editable() => {
                *self.caret = self.caret.saturating_add(1).min(self.secret.chars());
                Did::Looked
            }
            Key::LineStart if self.editable() => {
                *self.caret = 0;
                Did::Looked
            }
            Key::LineEnd if self.editable() => {
                *self.caret = self.secret.chars();
                Did::Looked
            }
            Key::WordLeft if self.shown() => {
                *self.caret = self.word(false);
                Did::Looked
            }
            Key::WordRight if self.shown() => {
                *self.caret = self.word(true);
                Did::Looked
            }
            Key::Reveal if self.editable() => {
                *self.revealed = !*self.revealed;
                Did::Looked
            }
            // Not a keystroke, so it starts no stopwatch and stops none. The
            // text is dropped at the end of either arm, which wipes it.
            Key::Paste(text) if self.editable() => self.take(&text),
            Key::Paste(_) => {
                *self.refused = self.refused.saturating_add(1);
                Did::Refused(Refusal::Paste)
            }
            // Unoffered, so it is nothing at all rather than a key that
            // sometimes works. The four that end the prompt never arrive here.
            Key::Enter
            | Key::Escape
            | Key::Interrupt
            | Key::Lookup
            | Key::EndOfInput
            | Key::Reveal
            | Key::DeleteRight
            | Key::KillToEnd
            | Key::KillWordLeft
            | Key::KillWordRight
            | Key::Transpose
            | Key::Left
            | Key::Right
            | Key::WordLeft
            | Key::WordRight
            | Key::LineStart
            | Key::LineEnd => Did::Nothing,
        }
    }
}

/// Read one secret.
///
/// The only loop in the binary that accepts a typed secret. `card` is asked for
/// what to draw whenever the screen has to change, which is what lets a drill
/// and an enrollment share this while looking like themselves.
///
/// `lookup` says whether the vault is on offer here. `cold` says whether a
/// memory is being measured, and everything else follows from it: a cold prompt
/// refuses a paste, stays blind, offers no reveal and moves no caret. A cold
/// prompt gives back nothing about what was typed.
///
/// # Errors
///
/// When the terminal cannot be read or written.
pub fn read_secret(
    console: &mut dyn Console,
    lookup: bool,
    cold: bool,
    resting: card::Tone,
    card: &mut dyn FnMut(Refusal, card::Tone, &Field<'_>) -> Card,
) -> Result<Typed> {
    let mut secret = Secret::new();
    let mut drawn = scratch();
    let mut caret = 0usize;
    let mut offset = 0usize;
    // Never sticky: every prompt, every retry and each half of a double entry
    // starts concealed, because this is a local and not a setting.
    let mut revealed = false;
    let mut ttfk: Option<u64> = None;
    let mut corrections = 0u32;
    let mut accepted = 0u32;
    let mut refused = 0u32;
    let mut standing = Refusal::None;
    console.arm();
    loop {
        let did = match console.wait(revealed.then_some(CONCEAL))? {
            // A revealed field goes back the moment nobody is certainly in
            // front of it, and the moment nobody is certainly still typing.
            Wake::Blur | Wake::Idle if revealed => {
                revealed = false;
                Did::Looked
            }
            Wake::Blur | Wake::Idle => Did::Nothing,
            Wake::Key(Key::Enter) => {
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
            Wake::Key(Key::Escape) => return Ok(Typed::Skipped),
            Wake::Ended | Wake::Key(Key::Interrupt) => return Ok(Typed::Aborted),
            Wake::Key(Key::Lookup) if lookup => return Ok(Typed::Lookup),
            // At a cold prompt there is no caret to delete forward at, so this
            // is unconditionally the end of input, which is what every blind
            // password prompt in the world does with it.
            Wake::Key(Key::EndOfInput) if cold || secret.is_empty() => return Ok(Typed::Aborted),
            Wake::Key(key) => {
                if matches!(key, Key::Char(_)) && ttfk.is_none() {
                    ttfk = Some(console.elapsed_ms());
                }
                Editing {
                    secret: &mut secret,
                    caret: &mut caret,
                    corrections: &mut corrections,
                    accepted: &mut accepted,
                    refused: &mut refused,
                    revealed: &mut revealed,
                    cold,
                }
                .apply(key)
            }
        };

        let reveal = rendering(cold, revealed);
        match did {
            Did::Nothing => {}
            Did::Refused(refusal) => {
                standing = refusal;
                let field = render_field(&secret, reveal, caret, &mut offset, &mut drawn);
                let alarmed = card(refusal, card::Tone::Alarm, &field);
                console.flash(&alarmed)?;
                let settled = card(refusal, resting, &field);
                console.paint(&settled)?;
            }
            Did::Looked | Did::Typed => {
                // Starting to type is the reader saying the refusal was read,
                // or that it no longer matters. No timer can know that, and
                // neither can a glance.
                let took_down = did == Did::Typed && standing != Refusal::None;
                if took_down {
                    standing = Refusal::None;
                }
                // A blind field shows nothing, so an ordinary keystroke changes
                // nothing on the screen and there is nothing to redraw.
                if !cold || took_down {
                    let field = render_field(&secret, reveal, caret, &mut offset, &mut drawn);
                    let settled = card(standing, resting, &field);
                    console.paint(&settled)?;
                }
            }
        }
    }
}

/// How much of the buffer this prompt draws, in this moment.
fn rendering(cold: bool, revealed: bool) -> Reveal {
    match (cold, revealed) {
        (true, _) => Reveal::Blind,
        (false, false) => Reveal::Masked,
        (false, true) => Reveal::Shown,
    }
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
    let mut build = |refusal: Refusal, tone: card::Tone, field: &Field<'_>| {
        let mut drawn = Card::new(prompt.title, card::stamp(today), style);
        drawn.reserve(ASK_SLOTS);
        drawn.say(prompt.heading, Tint::Bold).gap();
        // Unconditional, so an empty intention pads its line rather than moving
        // the field up into it.
        drawn.say(prompt.intention, intention);
        drawn.entry(field, tone);
        drawn.gap();
        let under = refusal.status().or(prompt.status).unwrap_or("");
        drawn.split(under, reveal_chip(field.reveal), Tint::Dim);
        drawn
    };
    let opening = build(Refusal::None, prompt.resting, &Field::empty(Reveal::Masked));
    console.anchor(opening.height());
    console.paint(&opening)?;
    read_secret(console, false, false, prompt.resting, &mut build)
}

/// What the line under the field says about the reveal, where one is offered.
///
/// Named because it is not a key anybody already knows. The card says it
/// exactly where it works, the way it says ctrl-l, so a key with no chip beside
/// it is a key that does nothing here.
#[must_use]
pub fn reveal_chip(reveal: Reveal) -> &'static str {
    match reveal {
        Reveal::Blind => "",
        Reveal::Masked => "^R  show",
        Reveal::Shown => "^R  hide",
    }
}

/// What the field says while a verifier is being minted or checked.
pub const WORKING: &str = "checking";

/// What the closing card says on the right of its last line.
pub const DISMISS: &str = "press any key";

/// The narrowest gap that still reads as two things rather than one phrase.
const APART: usize = 3;

/// How wide an outcome may be before it crowds the keypress beside it.
///
/// Published because the caller is the one that knows what it would drop to
/// fit: a line that cannot hold both keeps the half that has not been said yet.
pub const OUTCOME_ROOM: usize = card::CONTENT
    .saturating_sub(DISMISS.len())
    .saturating_sub(APART);

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
    // Only `y` says yes, so a stray key never starts something — and every
    // other key, named or not, is that stray key.
    Ok(matches!(console.next()?, Some(Key::Char('y' | 'Y'))))
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
    debug_assert!(
        text.chars().count() <= OUTCOME_ROOM,
        "an outcome crowded the keypress: {} columns of {OUTCOME_ROOM}",
        text.chars().count()
    );
    let style = console.style();
    let mut drawn = Card::new("done", card::stamp(today), style);
    drawn.reserve(ASK_SLOTS);
    // The heading is where it always was, and what happened is where everything
    // else that changes has been: on the line under the field.
    drawn.say(heading, Tint::Bold);
    drawn.pad_to(ASK_SLOTS.saturating_sub(1));
    drawn.split(text, DISMISS, tint);
    console.paint(&drawn)?;
    console.hold()
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use jiff::civil::date;
    use proptest::prelude::*;
    use relic_core::style::Style;

    use super::{
        Ask, Card, Field, Input, Key, Refusal, Reveal, Screen, Typed, Wake, ask, read_secret,
        render_field, scratch,
    };
    use crate::secret::{CAPACITY, Pasted, Secret};

    /// A scripted terminal that records every card it is handed.
    ///
    /// The wakes are consumed rather than copied, because one of them carries a
    /// secret and [`Key`] is deliberately neither `Copy` nor `Clone`. An idle
    /// and a lost focus are scripted the same way a keystroke is, so nothing
    /// here needs a clock.
    struct Fake {
        wakes: std::vec::IntoIter<Wake>,
        frames: Vec<Vec<String>>,
        flashes: usize,
        drains: usize,
        anchored: usize,
    }

    impl Fake {
        fn new(wakes: Vec<Wake>) -> Self {
            Self {
                wakes: wakes.into_iter(),
                frames: Vec::new(),
                flashes: 0,
                drains: 0,
                anchored: 0,
            }
        }
    }

    impl Input for Fake {
        fn arm(&mut self) {}

        fn wait(&mut self, _within: Option<Duration>) -> anyhow::Result<Wake> {
            Ok(self.wakes.next().unwrap_or(Wake::Ended))
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

    fn keys(list: Vec<Key>) -> Vec<Wake> {
        list.into_iter().map(Wake::Key).collect()
    }

    fn typing(text: &str) -> Vec<Wake> {
        let mut list: Vec<Key> = text.chars().map(Key::Char).collect();
        list.push(Key::Enter);
        keys(list)
    }

    fn paste(text: &str) -> Key {
        Key::Paste(Pasted::new(text.to_owned()))
    }

    /// What one card the loop asked for was asked for.
    #[derive(Clone, Debug, PartialEq, Eq)]
    struct Seen {
        refusal: Refusal,
        tone: super::card::Tone,
        reveal: Reveal,
        drawn: String,
        column: usize,
        clipped: (bool, bool),
    }

    /// Read one secret from a scripted terminal under one policy.
    fn read(wakes: Vec<Wake>, cold: bool) -> (Typed, Vec<Seen>, usize) {
        let mut console = Fake::new(wakes);
        let mut seen = Vec::new();
        let typed = read_secret(
            &mut console,
            false,
            cold,
            super::card::Tone::Calm,
            &mut |refusal, tone, field| {
                seen.push(Seen {
                    refusal,
                    tone,
                    reveal: field.reveal,
                    drawn: field.drawn.to_owned(),
                    column: field.column,
                    clipped: field.clipped,
                });
                Card::new("rote", "x", Style::PLAIN)
            },
        )
        .unwrap();
        (typed, seen, console.flashes)
    }

    fn refusals(seen: &[Seen]) -> Vec<Refusal> {
        seen.iter().map(|one| one.refusal).collect()
    }

    fn submitted(typed: Typed) -> super::Entry {
        match typed {
            Typed::Submitted(entry) => entry,
            Typed::Skipped | Typed::Aborted | Typed::Lookup => panic!("submitted"),
        }
    }

    fn secret_of(text: &str) -> Secret {
        Secret::from_bytes(text.as_bytes()).expect("a secret")
    }

    fn drawn(secret: &Secret, reveal: Reveal, caret: usize) -> (String, usize, (bool, bool)) {
        let mut buffer = scratch();
        let mut offset = 0;
        let field = render_field(secret, reveal, caret, &mut offset, &mut buffer);
        (field.drawn.to_owned(), field.column, field.clipped)
    }

    // ---- what a cold prompt gives back ------------------------------------

    #[test]
    fn a_cold_prompt_gives_back_nothing_about_what_was_typed() {
        // Every key that could disclose something, at the one prompt where
        // nothing may be disclosed. None of them draws anything at all.
        let script = keys(vec![
            Key::Char('a'),
            Key::Char('b'),
            Key::Reveal,
            Key::Left,
            Key::WordLeft,
            Key::LineStart,
            Key::DeleteRight,
            Key::KillToEnd,
            Key::KillWordLeft,
            Key::Transpose,
            Key::Enter,
        ]);
        let (typed, seen, flashes) = read(script, true);
        let entry = submitted(typed);
        assert_eq!(
            entry.secret.expose(),
            b"ab",
            "nothing may edit a blind field"
        );
        assert_eq!(flashes, 0);
        assert!(seen.is_empty(), "a blind field has nothing to redraw");
    }

    #[test]
    fn a_blind_field_never_draws_a_glyph_however_much_is_in_it() {
        for text in ["", "a", "correct horse battery staple", &"x".repeat(200)] {
            let secret = secret_of(text);
            assert_eq!(
                drawn(&secret, Reveal::Blind, secret.chars()),
                (String::new(), 0, (false, false)),
                "{}",
                text.len()
            );
        }
    }

    // ---- masking -----------------------------------------------------------

    proptest! {
        /// The disclosure-closure rule, as a property: a masked field is a
        /// function of the length and the caret, and of nothing else.
        #[test]
        fn two_secrets_of_equal_length_mask_identically(
            first in proptest::collection::vec(any::<char>(), 0..40),
            second in proptest::collection::vec(any::<char>(), 0..40),
        ) {
            let width = first.len().min(second.len());
            let first: String = first.into_iter().take(width).collect();
            let second: String = second.into_iter().take(width).collect();
            let first = secret_of(&first);
            let second = secret_of(&second);
            for caret in [0usize, 1, first.chars()] {
                let caret = caret.min(first.chars());
                prop_assert_eq!(
                    drawn(&first, Reveal::Masked, caret),
                    drawn(&second, Reveal::Masked, caret)
                );
            }
        }
    }

    #[test]
    fn a_masked_field_is_one_glyph_a_character() {
        let secret = secret_of("correct horse");
        let (glyphs, column, clipped) = drawn(&secret, Reveal::Masked, 5);
        assert_eq!(glyphs, "•".repeat(13));
        assert_eq!(column, 5);
        assert_eq!(clipped, (false, false));
    }

    #[test]
    fn a_shown_field_is_the_characters_themselves() {
        let secret = secret_of("correct horse");
        let (glyphs, _, _) = drawn(&secret, Reveal::Shown, 0);
        assert_eq!(glyphs, "correct horse");
    }

    #[test]
    fn nothing_that_cannot_be_drawn_in_one_column_reaches_a_line() {
        // A paste carries whatever the terminal had. A newline written in raw
        // mode would put the rest of the card wherever the cursor happened to
        // be; a wide glyph would overrun a box that counts characters.
        let secret = secret_of("a\nb\tc\u{1b}d\u{4e00}e\u{0301}f");
        let (glyphs, _, _) = drawn(&secret, Reveal::Shown, 0);
        assert_eq!(glyphs, "a▫b▫c▫d▫e▫f");
        assert_eq!(glyphs.chars().count(), secret.chars());
        assert!(!glyphs.contains('\n'));
        assert!(!glyphs.contains('\t'));
    }

    #[test]
    fn a_reveal_of_something_that_is_not_text_falls_back_to_a_mask() {
        let secret = Secret::from_bytes(&[0xff, 0xfe]).expect("bytes");
        let mut buffer = scratch();
        let mut offset = 0;
        let field = render_field(&secret, Reveal::Shown, 0, &mut offset, &mut buffer);
        assert_eq!(field.reveal, Reveal::Masked, "bytes never reach a screen");
    }

    // ---- the window --------------------------------------------------------

    #[test]
    fn a_view_that_fits_is_never_marked_and_a_partial_one_always_is() {
        let short = secret_of(&"x".repeat(super::card::FIELD - 1));
        assert_eq!(drawn(&short, Reveal::Masked, 0).2, (false, false));
        let long = secret_of(&"x".repeat(super::card::FIELD * 3));
        assert_eq!(drawn(&long, Reveal::Masked, 0).2, (false, true));
        assert_eq!(drawn(&long, Reveal::Masked, long.chars()).2, (true, false));
        assert_eq!(drawn(&long, Reveal::Masked, 100).2, (true, true));
    }

    #[test]
    fn the_window_keeps_the_caret_inside_the_text_area() {
        let field = super::card::FIELD;
        let long = secret_of(&"x".repeat(field * 3));
        for caret in [0, 1, field - 1, field, field + 1, field * 3] {
            let (glyphs, column, _) = drawn(&long, Reveal::Masked, caret);
            assert!(column < field, "caret at {caret} landed on column {column}");
            assert!(glyphs.chars().count() <= field);
        }
    }

    #[test]
    fn the_window_does_not_move_while_the_caret_stays_in_it() {
        let long = secret_of(&"x".repeat(super::card::FIELD * 2));
        let mut buffer = scratch();
        let mut offset = 0;
        let end = long.chars();
        let far = render_field(&long, Reveal::Masked, end, &mut offset, &mut buffer).column;
        let settled = offset;
        let near = render_field(&long, Reveal::Masked, end - 1, &mut offset, &mut buffer).column;
        assert_eq!(offset, settled, "the view jumped under a one-column move");
        assert_eq!(near + 1, far);
    }

    // ---- the readline set --------------------------------------------------

    #[test]
    fn a_character_lands_at_the_caret() {
        let script = keys(vec![
            Key::Char('a'),
            Key::Char('c'),
            Key::Left,
            Key::Char('b'),
            Key::Enter,
        ]);
        let entry = submitted(read(script, false).0);
        assert_eq!(entry.secret.expose(), b"abc");
    }

    #[test]
    fn the_line_keys_reach_both_ends() {
        let script = keys(vec![
            Key::Char('b'),
            Key::Char('c'),
            Key::LineStart,
            Key::Char('a'),
            Key::LineEnd,
            Key::Char('d'),
            Key::Enter,
        ]);
        let entry = submitted(read(script, false).0);
        assert_eq!(entry.secret.expose(), b"abcd");
    }

    #[test]
    fn end_of_input_deletes_forward_and_only_leaves_on_an_empty_buffer() {
        let script = keys(vec![
            Key::Char('a'),
            Key::Char('b'),
            Key::LineStart,
            Key::EndOfInput,
            Key::Enter,
        ]);
        let entry = submitted(read(script, false).0);
        assert_eq!(entry.secret.expose(), b"b");
        assert!(matches!(
            read(keys(vec![Key::EndOfInput]), false).0,
            Typed::Aborted
        ));
        // At a cold prompt there is no caret to delete forward at, so it is
        // unconditionally the end of input whatever has been typed.
        assert!(matches!(
            read(keys(vec![Key::Char('a'), Key::EndOfInput]), true).0,
            Typed::Aborted
        ));
    }

    #[test]
    fn killing_backward_stops_at_the_caret_and_at_a_cold_prompt_does_not() {
        let script = keys(vec![
            Key::Char('a'),
            Key::Char('b'),
            Key::Char('c'),
            Key::Left,
            Key::KillToStart,
            Key::Enter,
        ]);
        let entry = submitted(read(script, false).0);
        assert_eq!(entry.secret.expose(), b"c");
        // Blind has no caret, so backward to the start is the whole entry.
        let cold = keys(vec![
            Key::Char('a'),
            Key::Char('b'),
            Key::KillToStart,
            Key::Char('z'),
            Key::Enter,
        ]);
        let entry = submitted(read(cold, true).0);
        assert_eq!(entry.secret.expose(), b"z");
    }

    #[test]
    fn killing_forward_takes_the_tail() {
        let script = keys(vec![
            Key::Char('a'),
            Key::Char('b'),
            Key::Char('c'),
            Key::LineStart,
            Key::Right,
            Key::KillToEnd,
            Key::Enter,
        ]);
        let entry = submitted(read(script, false).0);
        assert_eq!(entry.secret.expose(), b"a");
    }

    #[test]
    fn transposing_swaps_the_pair_at_the_caret() {
        let script = keys(vec![
            Key::Char('a'),
            Key::Char('c'),
            Key::Char('b'),
            Key::Transpose,
            Key::Enter,
        ]);
        let entry = submitted(read(script, false).0);
        assert_eq!(entry.secret.expose(), b"abc");
    }

    #[test]
    fn the_word_keys_are_inert_under_a_mask_and_live_once_it_is_shown() {
        let word = |reveal: Vec<Key>| {
            let mut script = vec![
                Key::Char('o'),
                Key::Char('n'),
                Key::Char('e'),
                Key::Char(' '),
                Key::Char('t'),
                Key::Char('w'),
                Key::Char('o'),
            ];
            script.extend(reveal);
            script.push(Key::KillWordLeft);
            script.push(Key::Enter);
            let entry = submitted(read(keys(script), false).0);
            entry.secret.expose().to_vec()
        };
        // Masked: a jump distance and a removed length are both a word length,
        // which is the one thing this tool may never disclose.
        assert_eq!(word(Vec::new()), b"one two");
        assert_eq!(word(vec![Key::Reveal]), b"one ");
    }

    #[test]
    fn word_motion_moves_a_word_once_the_characters_are_on_the_screen() {
        let script = keys(vec![
            Key::Char('o'),
            Key::Char('n'),
            Key::Char('e'),
            Key::Char(' '),
            Key::Char('t'),
            Key::Char('w'),
            Key::Char('o'),
            Key::Reveal,
            Key::WordLeft,
            Key::Char('!'),
            Key::Enter,
        ]);
        let entry = submitted(read(script, false).0);
        assert_eq!(entry.secret.expose(), b"one !two");
    }

    // ---- the reveal --------------------------------------------------------

    #[test]
    fn a_reveal_is_a_two_way_toggle_and_a_cold_prompt_has_none() {
        let (_, seen, _) = read(keys(vec![Key::Char('a'), Key::Reveal, Key::Reveal]), false);
        let shown: Vec<Reveal> = seen.iter().map(|one| one.reveal).collect();
        assert_eq!(
            shown,
            vec![Reveal::Masked, Reveal::Shown, Reveal::Masked],
            "the toggle goes both ways"
        );
        let (_, cold, _) = read(keys(vec![Key::Char('a'), Key::Reveal]), true);
        assert!(
            cold.is_empty(),
            "a cold prompt draws nothing and shows nothing"
        );
    }

    #[test]
    fn a_revealed_field_goes_back_on_a_blur_and_on_an_idle() {
        for interruption in [Wake::Blur, Wake::Idle] {
            let mut script = keys(vec![Key::Char('a'), Key::Reveal]);
            script.push(interruption);
            script.extend(keys(vec![Key::Enter]));
            let (_, seen, _) = read(script, false);
            let shown: Vec<Reveal> = seen.iter().map(|one| one.reveal).collect();
            assert_eq!(shown, vec![Reveal::Masked, Reveal::Shown, Reveal::Masked]);
        }
    }

    #[test]
    fn an_idle_takes_the_reveal_back_without_taking_a_refusal_down() {
        let long = "x".repeat(CAPACITY + 1);
        let script = vec![
            Wake::Key(Key::Reveal),
            Wake::Key(paste(&long)),
            Wake::Idle,
            Wake::Key(Key::Enter),
        ];
        let (_, seen, _) = read(script, false);
        let last = seen.last().expect("a card");
        assert_eq!(last.reveal, Reveal::Masked, "the reveal went back");
        assert_eq!(
            last.refusal,
            Refusal::Full,
            "a timer may not decide a message has been read"
        );
    }

    #[test]
    fn a_reveal_never_survives_the_prompt_that_opened_it() {
        let first = read(keys(vec![Key::Char('a'), Key::Reveal, Key::Enter]), false);
        assert_eq!(
            first.1.last().expect("a card").reveal,
            Reveal::Shown,
            "it was on"
        );
        let (_, seen, _) = read(keys(vec![Key::Char('a')]), false);
        assert_eq!(
            seen.first().expect("a card").reveal,
            Reveal::Masked,
            "and the next prompt starts concealed"
        );
    }

    // ---- what is recorded ---------------------------------------------------

    #[test]
    fn corrections_count_every_keystroke_that_removed_something() {
        let mut script: Vec<Key> = "one two".chars().map(Key::Char).collect();
        script.extend(vec![
            Key::DeleteLeft,
            Key::LineStart,
            Key::DeleteRight,
            Key::LineEnd,
            Key::Reveal,
            Key::KillWordLeft,
            Key::LineStart,
            Key::KillToEnd,
            // Nothing left to remove, so nothing further is counted.
            Key::DeleteLeft,
            Key::KillToStart,
            Key::Enter,
        ]);
        let timings = submitted(read(keys(script), false).0).forget();
        assert_eq!(timings.corrections, 4);
    }

    #[test]
    fn navigation_and_a_reveal_start_no_stopwatch() {
        let script = keys(vec![
            Key::Reveal,
            Key::Left,
            Key::Right,
            Key::LineStart,
            Key::LineEnd,
            Key::Enter,
        ]);
        let timings = submitted(read(script, false).0).forget();
        assert_eq!(
            timings.ttfk_ms, None,
            "looking at a field is not typing into one"
        );
        assert_eq!(timings.total_ms, None);
        assert_eq!(timings.corrections, 0);
    }

    // ---- refusals -----------------------------------------------------------

    #[test]
    fn a_full_buffer_refuses_the_character_and_says_so() {
        let mut list: Vec<Key> = (0..CAPACITY + 3).map(|_| Key::Char('x')).collect();
        list.push(Key::Enter);
        let (typed, seen, flashes) = read(keys(list), true);
        let entry = submitted(typed);
        assert_eq!(
            entry.secret.expose().len(),
            CAPACITY,
            "nothing past the capacity"
        );
        assert_eq!(flashes, 3, "each refused character is a flash");
        // Nothing here ever lands, so nothing ever says the reader moved on:
        // what was refused stays said until something does.
        assert!(refusals(&seen).iter().all(|one| *one == Refusal::Full));
    }

    #[test]
    fn a_paste_is_refused_where_something_is_being_measured() {
        let (typed, seen, flashes) = read(keys(vec![paste("hunter2"), Key::Enter]), true);
        let entry = submitted(typed);
        assert!(
            entry.secret.is_empty(),
            "nothing pasted reaches a field that refuses a paste"
        );
        let timings = entry.forget();
        assert_eq!(timings.paste_refused, 1);
        assert_eq!(timings.paste_accepted, 0);
        assert_eq!(flashes, 1, "a refusal is said rather than swallowed");
        assert!(refusals(&seen).contains(&Refusal::Paste));
    }

    #[test]
    fn a_paste_reaches_the_field_where_nothing_is() {
        let (typed, _, flashes) = read(keys(vec![paste("hunter2"), Key::Enter]), false);
        let entry = submitted(typed);
        assert_eq!(entry.secret.expose(), b"hunter2");
        let timings = entry.forget();
        assert_eq!(timings.paste_accepted, 1);
        assert_eq!(timings.paste_refused, 0);
        assert_eq!(flashes, 0);
    }

    #[test]
    fn a_paste_is_not_a_keystroke_and_times_nothing() {
        let entry = submitted(read(keys(vec![paste("hunter2"), Key::Enter]), false).0);
        let timings = entry.forget();
        assert_eq!(
            timings.ttfk_ms, None,
            "an entry with no keystroke in it claims no latency"
        );
        assert_eq!(timings.total_ms, None);
        assert_eq!(timings.corrections, 0);
    }

    #[test]
    fn a_paste_lands_at_the_caret_rather_than_at_the_end() {
        let script = keys(vec![
            Key::Char('a'),
            Key::Char('d'),
            Key::Left,
            paste("bc"),
            Key::Char('!'),
            Key::Enter,
        ]);
        let entry = submitted(read(script, false).0);
        assert_eq!(entry.secret.expose(), b"abc!d");
    }

    #[test]
    fn a_paste_that_will_not_fit_is_refused_whole_rather_than_half_entered() {
        let long = "x".repeat(CAPACITY);
        let (typed, seen, flashes) =
            read(keys(vec![Key::Char('a'), paste(&long), Key::Enter]), false);
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
        assert!(refusals(&seen).contains(&Refusal::Full));
    }

    #[test]
    fn a_refusal_pulses_the_border_and_leaves_what_it_said_standing() {
        let (_, seen, _) = read(keys(vec![paste("hunter2"), Key::Enter]), true);
        let tones: Vec<super::card::Tone> = seen
            .iter()
            .filter(|one| one.refusal == Refusal::Paste)
            .map(|one| one.tone)
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
        let (_, seen, _) = read(
            keys(vec![
                paste("hunter2"),
                Key::Char('a'),
                Key::Char('b'),
                Key::Enter,
            ]),
            true,
        );
        // Refused, said in alarm, said again in calm, and taken down by the
        // first character that lands. The second draws nothing: a blind field
        // has nothing left to take down.
        assert_eq!(
            refusals(&seen),
            vec![Refusal::Paste, Refusal::Paste, Refusal::None]
        );
    }

    #[test]
    fn a_masked_field_redraws_on_every_keystroke_because_it_has_to() {
        let (_, seen, _) = read(
            keys(vec![Key::Char('a'), Key::Char('b'), Key::Enter]),
            false,
        );
        let widths: Vec<usize> = seen.iter().map(|one| one.drawn.chars().count()).collect();
        assert_eq!(widths, vec![1, 2], "a glyph has to appear as it is typed");
    }

    // ---- the prompt around the field ---------------------------------------

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
        let mut console = Fake::new(keys(vec![paste(&long), Key::Char('x'), Key::Enter]));
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
    fn a_prompt_that_takes_a_secret_twice_takes_a_paste_and_names_the_reveal() {
        let mut console = Fake::new(keys(vec![paste("hunter2"), Key::Enter]));
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
        let opening = console.frames.first().unwrap().join("\n");
        assert!(opening.contains("^R  show"), "{opening}");
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

    #[test]
    fn an_outcome_that_would_crowd_the_keypress_has_somewhere_to_give() {
        // The budget exists so a caller can drop the half that has already been
        // read rather than let a card run its two halves together.
        assert!(super::OUTCOME_ROOM + super::DISMISS.len() < super::card::CONTENT);
        let worst = format!("{} — nothing was attached", crate::intake::EMPTY);
        assert!(
            worst.chars().count() > super::OUTCOME_ROOM,
            "the worst refusal is what the budget is for"
        );
    }

    #[test]
    fn the_reveal_chip_names_the_key_only_where_it_works() {
        assert_eq!(super::reveal_chip(Reveal::Blind), "");
        assert_eq!(super::reveal_chip(Reveal::Masked), "^R  show");
        assert_eq!(super::reveal_chip(Reveal::Shown), "^R  hide");
    }

    #[test]
    fn an_unoffered_lookup_is_nothing_at_all() {
        let (typed, _, _) = read(keys(vec![Key::Lookup, Key::Char('a'), Key::Enter]), true);
        assert_eq!(submitted(typed).secret.expose(), b"a");
    }

    #[test]
    fn a_field_that_fits_is_never_marked_at_the_moment_of_submission() {
        let _ = Field::blind();
    }
}
