//! crossterm: raw mode, the alternate screen, placement, and restoration.
//!
//! A sitting runs on the **alternate screen** and leaves nothing in scrollback.
//! Blind input already keeps typed bytes off the display; what the alternate
//! screen additionally removes is a standing record of which slugs exist and
//! which were failed.
//!
//! Putting the terminal back is not one control but three, because each covers
//! a way of leaving that the others do not: a guard for the ordinary return, a
//! panic hook for the unwind, and a signal thread because a default-disposition
//! `SIGTERM` runs no destructor at all. A terminal left in raw mode with echo
//! off is one that shows nothing of what is typed into it next.
//!
//! Every line is placed with an explicit `MoveTo`, so no newline is ever written
//! while raw mode is held. That matters because raw mode takes the line
//! discipline away: a bare newline moves down without returning to column zero,
//! and everything after it starts under the end of the line before.
//!
//! Where the dialog sits is this module's business and nobody else's. A caller
//! hands over a card; centering, the terminal being resized under it, and the
//! terminal being too small to draw in at all are all answered here.

use std::io::{IsTerminal as _, Write as _};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::{Context as _, Result, anyhow};
use crossterm::cursor::SetCursorStyle;
use crossterm::event::{
    DisableBracketedPaste, DisableFocusChange, EnableBracketedPaste, EnableFocusChange, Event,
    KeyCode, KeyEvent, KeyEventKind, KeyModifiers,
};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, execute, queue};
use relic_core::style::Style;
use zeroize::Zeroize as _;

use super::card::{self, Card};
use super::{Input, Key, Screen, Wake};
use crate::secret::Pasted;

/// The narrowest terminal a card fits in, with a column either side.
const MIN_COLS: usize = card::WIDTH + 2;

/// The shortest terminal worth drawing a dialog in: a minimal card and a line
/// of air.
const MIN_ROWS: usize = 7;

/// How long the field border stays red. A ceiling, not a wait: the first thing
/// typed ends it sooner, and what the refusal *said* stays up either way.
const FLASH: std::time::Duration = std::time::Duration::from_millis(340);

/// Where the top of the dialog sits in the space available to it, as a
/// fraction. A box at the exact middle reads as low, so designed dialogs sit
/// above it — the optical center rather than the arithmetic one.
const OPTICAL_NUMERATOR: usize = 2;
const OPTICAL_DENOMINATOR: usize = 5;

/// Whether a dialog holds the screen.
///
/// Raw mode and the alternate screen are process-wide, so two dialogs cannot be
/// open at once: the second one's restoration would put the terminal back while
/// the first is still drawing on it, turning echo on under a prompt that is
/// about to be typed into. Refusing is the only safe answer, and it is loud.
static OPEN: AtomicBool = AtomicBool::new(false);

static RAW: AtomicBool = AtomicBool::new(false);
static ALT: AtomicBool = AtomicBool::new(false);
static HOOK: Once = Once::new();

/// Put the terminal back. Safe to call more than once, and safe to call when
/// only half of it was ever taken: the guard, the panic hook and the signal
/// thread all call it, and none of them knows whether another already has.
fn restore() {
    let mut out = std::io::stdout();
    if ALT.swap(false, Ordering::SeqCst) {
        let _ = execute!(
            out,
            SetCursorStyle::DefaultUserShape,
            LeaveAlternateScreen,
            cursor::Show
        );
    }
    if RAW.swap(false, Ordering::SeqCst) {
        // Both DECSET modes are global rather than per screen buffer, so
        // leaving the alternate screen clears neither. Paired with the one flag
        // that every restoration path already swaps, so the guard, the panic
        // hook and the signal thread all undo them.
        let _ = execute!(out, DisableFocusChange, DisableBracketedPaste);
        let _ = disable_raw_mode();
    }
    let _ = out.flush();
}

/// Take the screen, or say who has it.
fn claim() -> Result<()> {
    if OPEN.swap(true, Ordering::SeqCst) {
        return Err(anyhow!(
            "a dialog is already open on this terminal, and a second one would put it back under the first"
        ));
    }
    Ok(())
}

/// Give the screen up.
fn release() {
    OPEN.store(false, Ordering::SeqCst);
}

/// Arm the three ways of putting the terminal back. Idempotent.
fn arm_restoration() {
    HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            restore();
            previous(info);
        }));
        watch_signals();
    });
}

/// Raw mode and bracketed paste, put back when this is dropped.
struct RawMode;

impl RawMode {
    fn enter() -> Result<Self> {
        arm_restoration();
        enable_raw_mode().context("putting the terminal into raw mode")?;
        RAW.store(true, Ordering::SeqCst);
        execute!(std::io::stdout(), EnableBracketedPaste, EnableFocusChange)
            .context("asking the terminal to bracket pastes and report focus")?;
        Ok(Self)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        restore();
    }
}

/// Whether a dialog can run here at all.
///
/// Both halves of the exchange: a dialog that cannot be typed into is not a
/// dialog, and neither is one that cannot paint. Nothing is asked of stderr,
/// which carries no part of it — a run with its errors redirected still has a
/// terminal to hold a card.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal() && std::io::stdout().is_terminal()
}

/// The terminal, held in dialog mode for as long as this value lives.
pub struct Terminal {
    armed_at: Instant,
    style: Style,
    anchor: usize,
    last: Vec<String>,
    last_caret: Option<(usize, usize)>,
    /// What ended a pulse early, waiting to be handed on.
    ///
    /// A keystroke that cut a flash short is still a keystroke the reader
    /// meant, so it is delivered rather than swallowed. One slot is enough:
    /// only [`Screen::flash`] ever fills it, and it returns the moment it does.
    pending: Option<Event>,
    /// A focus loss seen while the program was not waiting for one.
    ///
    /// Focus is a state rather than a keystroke, so it is held as a flag rather
    /// than parked in the single-slot queue a key uses. Otherwise an alt-tab
    /// during a refusal's pulse would either be lost or would evict the
    /// keystroke that ended it.
    blurred: bool,
    _raw: RawMode,
}

impl Terminal {
    /// Enter dialog mode.
    ///
    /// # Errors
    ///
    /// When this is not a terminal, or the terminal refuses raw mode.
    pub fn enter(style: Style) -> Result<Self> {
        if !is_interactive() {
            return Err(anyhow!(
                "this has to be typed at a terminal, so there is nothing to run here"
            ));
        }
        claim()?;
        let raw = match RawMode::enter() {
            Ok(raw) => raw,
            Err(error) => {
                release();
                return Err(error);
            }
        };
        execute!(std::io::stdout(), EnterAlternateScreen, cursor::Hide)
            .context("entering the alternate screen")?;
        ALT.store(true, Ordering::SeqCst);
        Ok(Self {
            armed_at: Instant::now(),
            style,
            anchor: 0,
            last: Vec::new(),
            last_caret: None,
            pending: None,
            blurred: false,
            _raw: raw,
        })
    }

    /// Draw what was last painted, wherever the terminal is now.
    fn repaint(&mut self) -> Result<()> {
        let lines = std::mem::take(&mut self.last);
        let result = self.blit(&lines, self.last_caret);
        self.last = lines;
        result
    }

    /// Place a card on the alternate screen.
    ///
    /// The cursor is the terminal's own, moved into the entry field and shown
    /// there, so it blinks the way every other password field on the machine
    /// blinks and nothing has to animate it. Where there is nothing to type, it
    /// is hidden rather than parked somewhere meaningless.
    fn blit(&mut self, lines: &[String], caret: Option<(usize, usize)>) -> Result<()> {
        let (cols, rows) = size();
        if cols < MIN_COLS || rows < MIN_ROWS.max(lines.len()) {
            return cramped(cols, rows, lines.len());
        }
        let left = cols.saturating_sub(card::WIDTH) / 2;
        let top = place(rows, self.anchor, lines.len());
        let mut out = std::io::stdout();
        queue!(out, cursor::Hide, Clear(ClearType::All))?;
        for (offset, line) in lines.iter().enumerate() {
            let row = u16::try_from(top.saturating_add(offset)).unwrap_or(u16::MAX);
            let column = u16::try_from(left).unwrap_or(u16::MAX);
            queue!(
                out,
                cursor::MoveTo(column, row),
                crossterm::style::Print(line)
            )?;
        }
        if let Some((row, column)) = caret {
            let row = u16::try_from(top.saturating_add(row)).unwrap_or(u16::MAX);
            let column = u16::try_from(left.saturating_add(column)).unwrap_or(u16::MAX);
            queue!(
                out,
                cursor::MoveTo(column, row),
                SetCursorStyle::BlinkingBlock,
                cursor::Show
            )?;
        }
        out.flush()?;
        Ok(())
    }

    /// The next event, with a resize answered here rather than handed on.
    fn event(&mut self) -> Result<Event> {
        loop {
            let event = match self.pending.take() {
                Some(event) => event,
                None => crossterm::event::read().context("reading the terminal")?,
            };
            if matches!(event, Event::Resize(_, _)) {
                self.repaint()?;
                continue;
            }
            return Ok(event);
        }
    }

    /// What one event means to a dialog that is waiting, or nothing when it was
    /// answered here.
    fn sift(&mut self, event: Event) -> Result<Option<Wake>> {
        match event {
            Event::Resize(_, _) => {
                self.repaint()?;
                Ok(None)
            }
            Event::FocusLost => Ok(Some(Wake::Blur)),
            Event::FocusGained => Ok(None),
            Event::Key(_) | Event::Paste(_) | Event::Mouse(_) => Ok(key_of(event).map(Wake::Key)),
        }
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        restore();
        release();
    }
}

/// Say the terminal is too small, rather than drawing a broken box in it.
///
/// The dialog has a fixed shape and nothing about it degrades usefully, so this
/// is the honest answer rather than a smaller card that cannot hold what it has
/// to say.
fn cramped(cols: usize, rows: usize, wanted_rows: usize) -> Result<()> {
    let needed = format!("rote needs {MIN_COLS} x {}", MIN_ROWS.max(wanted_rows));
    let has = format!("this terminal is {cols} x {rows}");
    let mut out = std::io::stdout();
    queue!(out, Clear(ClearType::All))?;
    for (offset, line) in [needed, has].iter().enumerate() {
        let row = u16::try_from(offset).unwrap_or(u16::MAX);
        queue!(out, cursor::MoveTo(0, row), crossterm::style::Print(line))?;
    }
    out.flush()?;
    Ok(())
}

/// The terminal's size, or the smallest thing that will refuse to draw.
fn size() -> (usize, usize) {
    crossterm::terminal::size().map_or((0, 0), |(cols, rows)| {
        (usize::from(cols), usize::from(rows))
    })
}

/// Where the top edge goes: the optical center of the anchored height, moved up
/// only if what is actually being drawn would not otherwise fit.
fn place(rows: usize, anchored: usize, drawn: usize) -> usize {
    let free = rows.saturating_sub(anchored);
    let top = free.saturating_mul(OPTICAL_NUMERATOR) / OPTICAL_DENOMINATOR;
    top.min(rows.saturating_sub(drawn))
}

fn watch_signals() {
    let Ok(mut signals) = signal_hook::iterator::Signals::new([
        signal_hook::consts::SIGTERM,
        signal_hook::consts::SIGHUP,
        signal_hook::consts::SIGINT,
    ]) else {
        return;
    };
    std::thread::spawn(move || {
        if signals.forever().next().is_some() {
            restore();
            std::process::exit(i32::from(crate::exit::INCOMPLETE));
        }
    });
}

impl Input for Terminal {
    fn arm(&mut self) {
        self.armed_at = Instant::now();
    }

    /// The deadline is absolute. A relative one re-passed to `poll` after every
    /// resize repaint would let a window being dragged postpone a conceal
    /// indefinitely.
    fn wait(&mut self, within: Option<std::time::Duration>) -> Result<Wake> {
        if self.blurred {
            self.blurred = false;
            return Ok(Wake::Blur);
        }
        let deadline = within.map(|span| Instant::now() + span);
        loop {
            let event = if let Some(event) = self.pending.take() {
                event
            } else {
                if let Some(at) = deadline {
                    let left = at.saturating_duration_since(Instant::now());
                    if left.is_zero()
                        || !crossterm::event::poll(left).context("waiting at the terminal")?
                    {
                        return Ok(Wake::Idle);
                    }
                }
                crossterm::event::read().context("reading the terminal")?
            };
            if let Some(wake) = self.sift(event)? {
                return Ok(wake);
            }
        }
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.armed_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

impl Screen for Terminal {
    fn style(&self) -> Style {
        self.style
    }

    fn anchor(&mut self, height: usize) {
        self.anchor = height;
    }

    fn paint(&mut self, card: &Card) -> Result<()> {
        let lines = card.render();
        let caret = card.caret();
        let result = self.blit(&lines, caret);
        // The outgoing frame may be a revealed field's. Wiped unconditionally,
        // because a rule that applies to one kind of card is a rule somebody
        // has to remember which kind.
        for line in &mut self.last {
            line.zeroize();
        }
        self.last = lines;
        self.last_caret = caret;
        result
    }

    fn flash(&mut self, card: &Card) -> Result<()> {
        self.paint(card)?;
        let deadline = Instant::now() + FLASH;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            if !crossterm::event::poll(left).context("waiting at the terminal")? {
                return Ok(());
            }
            let event = crossterm::event::read().context("reading the terminal")?;
            match event {
                Event::Resize(_, _) => self.repaint()?,
                // Focus is remembered rather than parked, so an alt-tab neither
                // cuts the pulse short nor evicts the keystroke that would.
                Event::FocusLost => self.blurred = true,
                Event::FocusGained => {}
                // Typing is the reader saying they have moved on, so the pulse
                // ends with it — and the key goes on to the loop that was
                // waiting for it, because a flash that eats a keystroke is
                // worse than a short one.
                Event::Key(_) | Event::Paste(_) | Event::Mouse(_) => {
                    self.pending = Some(event);
                    return Ok(());
                }
            }
        }
    }

    fn drain(&mut self) -> Result<()> {
        // Wiped rather than dropped with the bytes still in it: a flash parks
        // whatever ended it, and what ended it may have been a paste.
        if let Some(Event::Paste(mut text)) = self.pending.take() {
            text.zeroize();
        }
        while crossterm::event::poll(std::time::Duration::ZERO)
            .context("waiting at the terminal")?
        {
            // A paste is wiped rather than dropped with the bytes still in it,
            // the same way `hold` does.
            if let Event::Paste(mut text) =
                crossterm::event::read().context("reading the terminal")?
            {
                text.zeroize();
            }
        }
        Ok(())
    }

    fn hold(&mut self) -> Result<()> {
        loop {
            match self.event()? {
                Event::Key(KeyEvent {
                    kind: KeyEventKind::Press,
                    ..
                }) => return Ok(()),
                // Nothing is being typed here, but a paste is a paste: it is
                // wiped rather than dropped with the bytes still in it.
                Event::Paste(mut text) => text.zeroize(),
                Event::Key(_)
                | Event::Mouse(_)
                | Event::Resize(_, _)
                | Event::FocusGained
                | Event::FocusLost => {}
            }
        }
    }
}

/// Map a terminal event onto a key.
///
/// Only a bare character reaches the buffer. A modified key is either one of the
/// three commands below or nothing at all, which is what stops an escape
/// sequence being typed into a secret.
///
/// The event is taken by value so that pasted text is moved into the key rather
/// than copied out of it. Whether that text ever reaches a field is the prompt's
/// to decide; dropping the key wipes it either way.
pub fn key_of(event: Event) -> Option<Key> {
    match event {
        Event::Paste(text) => Some(Key::Paste(Pasted::new(text))),
        Event::Key(key) => {
            if !matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) {
                return None;
            }
            code_of(key.code, key.modifiers)
        }
        Event::Mouse(_) | Event::Resize(_, _) | Event::FocusGained | Event::FocusLost => None,
    }
}

/// `KeyCode` is an upstream enum that gains variants between releases, so a
/// wildcard is the correct arm here rather than a missed one: anything this does
/// not name is nothing, which is the property that keeps an escape sequence out
/// of a secret. Scoped to this function so our own enums stay exhaustive.
///
/// The map is readline's, because readline is the muscle memory a terminal
/// already has. Where readline names one intent twice, both spellings arrive
/// here; where a terminal encodes one intent twice, both arrive here too. Which
/// of them a given terminal actually sends — `alt` in particular, which a macOS
/// terminal may keep for typing symbols — is the terminal's business and not
/// this table's.
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "upstream enum; anything unmapped is deliberately nothing"
)]
fn code_of(code: KeyCode, modifiers: KeyModifiers) -> Option<Key> {
    let control = modifiers.contains(KeyModifiers::CONTROL);
    let alt = modifiers.contains(KeyModifiers::ALT);
    let word = control || alt;
    match code {
        KeyCode::Char('c') if control => Some(Key::Interrupt),
        KeyCode::Char('d') if control => Some(Key::EndOfInput),
        KeyCode::Char('l') if control => Some(Key::Lookup),
        KeyCode::Char('r') if control => Some(Key::Reveal),
        KeyCode::Char('u') if control => Some(Key::KillToStart),
        KeyCode::Char('k') if control => Some(Key::KillToEnd),
        KeyCode::Char('w') if control => Some(Key::KillWordLeft),
        KeyCode::Char('a') if control => Some(Key::LineStart),
        KeyCode::Char('e') if control => Some(Key::LineEnd),
        KeyCode::Char('b') if control => Some(Key::Left),
        KeyCode::Char('f') if control => Some(Key::Right),
        KeyCode::Char('h') if control => Some(Key::DeleteLeft),
        KeyCode::Char('t') if control => Some(Key::Transpose),
        KeyCode::Char('b') if alt => Some(Key::WordLeft),
        KeyCode::Char('f') if alt => Some(Key::WordRight),
        KeyCode::Char('d') if alt => Some(Key::KillWordRight),
        KeyCode::Backspace if word => Some(Key::KillWordLeft),
        KeyCode::Left if word => Some(Key::WordLeft),
        KeyCode::Right if word => Some(Key::WordRight),
        KeyCode::Char(character) if modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            Some(Key::Char(character))
        }
        KeyCode::Backspace => Some(Key::DeleteLeft),
        KeyCode::Delete => Some(Key::DeleteRight),
        KeyCode::Left => Some(Key::Left),
        KeyCode::Right => Some(Key::Right),
        KeyCode::Home => Some(Key::LineStart),
        KeyCode::End => Some(Key::LineEnd),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Esc => Some(Key::Escape),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::{Key, claim, key_of, place, release};
    use crate::secret::Pasted;

    fn press(code: KeyCode, modifiers: KeyModifiers) -> Event {
        Event::Key(KeyEvent {
            code,
            modifiers,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        })
    }

    #[test]
    fn a_bare_character_reaches_the_buffer() {
        assert_eq!(
            key_of(press(KeyCode::Char('a'), KeyModifiers::NONE)),
            Some(Key::Char('a'))
        );
        assert_eq!(
            key_of(press(KeyCode::Char('A'), KeyModifiers::SHIFT)),
            Some(Key::Char('A'))
        );
        assert_eq!(
            key_of(press(KeyCode::Char(' '), KeyModifiers::NONE)),
            Some(Key::Char(' '))
        );
    }

    #[test]
    fn the_readline_set_arrives_under_every_spelling_a_terminal_sends() {
        let control = KeyModifiers::CONTROL;
        let alt = KeyModifiers::ALT;
        let none = KeyModifiers::NONE;
        for (code, modifiers, key) in [
            (KeyCode::Char('c'), control, Key::Interrupt),
            (KeyCode::Char('d'), control, Key::EndOfInput),
            (KeyCode::Char('u'), control, Key::KillToStart),
            (KeyCode::Char('k'), control, Key::KillToEnd),
            (KeyCode::Char('w'), control, Key::KillWordLeft),
            (KeyCode::Char('l'), control, Key::Lookup),
            (KeyCode::Char('r'), control, Key::Reveal),
            (KeyCode::Char('t'), control, Key::Transpose),
            // One intent, two readline spellings and two key caps.
            (KeyCode::Char('a'), control, Key::LineStart),
            (KeyCode::Home, none, Key::LineStart),
            (KeyCode::Char('e'), control, Key::LineEnd),
            (KeyCode::End, none, Key::LineEnd),
            (KeyCode::Char('b'), control, Key::Left),
            (KeyCode::Left, none, Key::Left),
            (KeyCode::Char('f'), control, Key::Right),
            (KeyCode::Right, none, Key::Right),
            (KeyCode::Char('h'), control, Key::DeleteLeft),
            (KeyCode::Backspace, none, Key::DeleteLeft),
            (KeyCode::Delete, none, Key::DeleteRight),
            // Word-wise, in all three encodings a terminal may send.
            (KeyCode::Char('b'), alt, Key::WordLeft),
            (KeyCode::Left, alt, Key::WordLeft),
            (KeyCode::Left, control, Key::WordLeft),
            (KeyCode::Char('f'), alt, Key::WordRight),
            (KeyCode::Right, alt, Key::WordRight),
            (KeyCode::Right, control, Key::WordRight),
            (KeyCode::Char('d'), alt, Key::KillWordRight),
            (KeyCode::Backspace, alt, Key::KillWordLeft),
            (KeyCode::Backspace, control, Key::KillWordLeft),
        ] {
            assert_eq!(key_of(press(code, modifiers)), Some(key), "{code:?}");
        }
    }

    #[test]
    fn an_unmapped_modified_key_is_nothing_rather_than_a_character() {
        assert_eq!(
            key_of(press(KeyCode::Char('z'), KeyModifiers::CONTROL)),
            None,
            "an unmapped control key must not become a character"
        );
        assert_eq!(key_of(press(KeyCode::Char('z'), KeyModifiers::ALT)), None);
        // A kill ring and an undo stack are plaintext stores of the thing being
        // killed, so readline's two yanking keys are deliberately absent.
        assert_eq!(
            key_of(press(KeyCode::Char('y'), KeyModifiers::CONTROL)),
            None
        );
        assert_eq!(
            key_of(press(KeyCode::Char('_'), KeyModifiers::CONTROL)),
            None
        );
    }

    #[test]
    fn submitting_skipping_and_correcting_map_across() {
        assert_eq!(
            key_of(press(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Key::Enter)
        );
        assert_eq!(
            key_of(press(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Key::Escape)
        );
        assert_eq!(
            key_of(press(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(Key::DeleteLeft)
        );
    }

    #[test]
    fn a_paste_arrives_as_a_paste_carrying_what_was_pasted() {
        assert_eq!(
            key_of(Event::Paste("hunter2".to_owned())),
            Some(Key::Paste(Pasted::new("hunter2".to_owned()))),
            "the text has to reach the prompt that decides what to do with it"
        );
    }

    #[test]
    fn a_pasted_secret_is_not_what_a_failing_assertion_prints() {
        let key = key_of(Event::Paste("hunter2".to_owned())).unwrap();
        assert_eq!(format!("{key:?}"), "Paste(Pasted(<redacted>))");
    }

    #[test]
    fn a_key_release_is_not_a_keystroke() {
        assert_eq!(
            key_of(Event::Key(KeyEvent {
                code: KeyCode::Char('a'),
                modifiers: KeyModifiers::NONE,
                kind: KeyEventKind::Release,
                state: KeyEventState::NONE,
            })),
            None
        );
    }

    #[test]
    fn everything_else_is_ignored_rather_than_guessed_at() {
        assert_eq!(key_of(Event::Resize(80, 24)), None);
        assert_eq!(key_of(Event::FocusGained), None);
        assert_eq!(key_of(press(KeyCode::F(1), KeyModifiers::NONE)), None);
        assert_eq!(key_of(press(KeyCode::Up, KeyModifiers::NONE)), None);
        assert_eq!(key_of(press(KeyCode::Tab, KeyModifiers::NONE)), None);
    }

    #[test]
    fn a_second_dialog_is_refused_rather_than_left_to_undo_the_first() {
        claim().expect("nothing holds the screen");
        assert!(
            claim().is_err(),
            "a second dialog would restore the terminal under the first"
        );
        release();
        claim().expect("the screen was given up");
        release();
    }

    #[test]
    fn the_dialog_sits_above_the_arithmetic_middle() {
        // Twenty spare rows: centered would be ten down, optical is eight.
        assert_eq!(place(30, 10, 10), 8);
        assert!(place(40, 12, 12) < (40 - 12) / 2);
    }

    #[test]
    fn a_card_taller_than_its_anchor_grows_downward_from_the_same_top() {
        let top = place(30, 10, 10);
        for drawn in [11, 12, 15, 20] {
            assert_eq!(place(30, 10, drawn), top, "the top edge must not move");
        }
    }

    #[test]
    fn a_card_shorter_than_its_anchor_keeps_the_same_top_too() {
        let top = place(30, 10, 10);
        for drawn in [5, 7, 9] {
            assert_eq!(place(30, 10, drawn), top, "the top edge must not move");
        }
    }

    #[test]
    fn a_card_that_would_run_off_the_bottom_is_lifted_rather_than_clipped() {
        assert_eq!(place(20, 10, 20), 0);
        assert_eq!(place(20, 10, 18), 2);
    }
}
