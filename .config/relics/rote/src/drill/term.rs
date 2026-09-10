//! The terminal, and everything that has to be put back.
//!
//! The sitting runs on the **alternate screen** and leaves nothing in
//! scrollback. Blind input already keeps typed bytes off the display; what the
//! alternate screen additionally removes is a standing record of which slugs
//! exist and which were failed. It is also what would make per-character masking
//! a question of taste later rather than a question of security.
//!
//! Putting the terminal back is not one control but three, because each covers
//! a way of leaving that the others do not: a guard for the ordinary return, a
//! panic hook for the unwind, and a signal thread because a default-disposition
//! `SIGTERM` runs no destructor at all. A terminal left in raw mode with echo
//! off is one that shows nothing of what is typed into it next.
//!
//! Raw mode also takes the line discipline away, so a bare newline moves down
//! without returning to column zero. Writing to the terminal while it is held
//! therefore goes through [`RawLines`], which is reachable only from the guard.

use std::io::{IsTerminal as _, Write as _};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::{Context as _, Result, anyhow};
use crossterm::event::{
    DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{cursor, execute, queue};

use super::{Input, Key, Screen, screen};
use crate::secret::Secret;

/// Exit status for a sitting cut short by a signal.
const INTERRUPTED: i32 = 2;

static RAW: AtomicBool = AtomicBool::new(false);
static ALT: AtomicBool = AtomicBool::new(false);
static HOOK: Once = Once::new();

/// Put the terminal back. Safe to call more than once, and safe to call when
/// only half of it was ever taken: the guard, the panic hook and the signal
/// thread all call it, and none of them knows whether another already has.
fn restore() {
    let mut out = std::io::stdout();
    if ALT.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, LeaveAlternateScreen, cursor::Show);
    }
    if RAW.swap(false, Ordering::SeqCst) {
        let _ = execute!(out, DisableBracketedPaste);
        let _ = disable_raw_mode();
    }
    let _ = out.flush();
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
        execute!(std::io::stdout(), EnableBracketedPaste)
            .context("asking the terminal to bracket pastes")?;
        Ok(Self)
    }

    /// Standard error, with the line discipline raw mode took away.
    ///
    /// The borrow is the constraint: a writer that ends lines this way is only
    /// correct while raw mode is held, so it cannot outlive the guard, and
    /// there is no way to reach one without holding it.
    fn err(&self) -> RawLines<'_, std::io::Stderr> {
        RawLines {
            inner: std::io::stderr(),
            _guard: self,
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        restore();
    }
}

/// A writer that ends every line the way raw mode needs it ended.
///
/// Idempotent, so a line already written with a carriage return is left alone:
/// whichever way a call site spells the break, what reaches the terminal is the
/// same, and the class of bug where the next line starts under the last one
/// cannot come back.
struct RawLines<'a, W: std::io::Write> {
    inner: W,
    _guard: &'a RawMode,
}

impl<W: std::io::Write> std::io::Write for RawLines<'_, W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        for chunk in buf.split_inclusive(|&byte| byte == b'\n') {
            if let Some((&b'\n', head)) = chunk.split_last() {
                self.inner
                    .write_all(head.strip_suffix(b"\r").unwrap_or(head))?;
                self.inner.write_all(b"\r\n")?;
            } else {
                self.inner.write_all(chunk)?;
            }
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

/// Read one secret at an inline prompt.
///
/// Blind: the prompt stays in scrollback and not one character of the answer
/// does. `None` means the prompt was abandoned rather than answered.
///
/// # Errors
///
/// When this is not a terminal, or the terminal cannot be read.
pub fn ask(prompt: &str) -> Result<Option<Secret>> {
    if !is_interactive() {
        return Err(anyhow!(
            "a secret has to be typed at a terminal; use --stdin to feed one from a pipe"
        ));
    }
    let raw = RawMode::enter()?;
    let mut err = raw.err();
    write!(err, "{prompt}")?;
    err.flush()?;
    let mut secret = Secret::new();
    let answer = loop {
        let event = crossterm::event::read().context("reading the terminal")?;
        match key_of(&event) {
            Some(Key::Char(character)) => {
                secret.push(character);
            }
            Some(Key::Backspace) => {
                secret.pop();
            }
            Some(Key::Clear) => secret.clear(),
            Some(Key::Enter) => break Some(secret),
            Some(Key::Escape | Key::Interrupt) => break None,
            Some(Key::Paste) => {
                write!(err, "\n  a paste was refused — type it\n{prompt}")?;
                err.flush()?;
            }
            None => {}
        }
    };
    writeln!(err)?;
    err.flush()?;
    Ok(answer)
}

/// Whether a drill can run here at all.
///
/// All three streams, not just stdin: a sitting that cannot paint is not a
/// sitting, and a redirected stream is the shape a piped secret would arrive in.
pub fn is_interactive() -> bool {
    std::io::stdin().is_terminal()
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
}

/// The terminal, held in drill mode for as long as this value lives.
pub struct Terminal {
    armed_at: Instant,
    color: bool,
    _raw: RawMode,
}

impl Terminal {
    /// Enter drill mode.
    ///
    /// # Errors
    ///
    /// When this is not a terminal, or the terminal refuses raw mode.
    pub fn enter(color: bool) -> Result<Self> {
        if !is_interactive() {
            return Err(anyhow!(
                "a drill has to be typed at a terminal, so there is nothing to run here"
            ));
        }
        let raw = RawMode::enter()?;
        execute!(std::io::stdout(), EnterAlternateScreen, cursor::Hide)
            .context("entering the alternate screen")?;
        ALT.store(true, Ordering::SeqCst);
        Ok(Self {
            armed_at: Instant::now(),
            color,
            _raw: raw,
        })
    }
}

impl Drop for Terminal {
    fn drop(&mut self) {
        restore();
    }
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
            std::process::exit(INTERRUPTED);
        }
    });
}

impl Input for Terminal {
    fn arm(&mut self) {
        self.armed_at = Instant::now();
    }

    fn next(&mut self) -> Result<Option<Key>> {
        loop {
            let event = crossterm::event::read().context("reading the terminal")?;
            if let Some(key) = key_of(&event) {
                return Ok(Some(key));
            }
        }
    }

    fn elapsed_ms(&self) -> u64 {
        u64::try_from(self.armed_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }
}

impl Screen for Terminal {
    fn paint(&mut self, frame: &screen::Frame<'_>) -> Result<()> {
        let mut out = std::io::stdout();
        queue!(out, Clear(ClearType::All), cursor::MoveTo(0, 0))?;
        for line in screen::render(frame, self.color) {
            queue!(
                out,
                crossterm::style::Print(line),
                cursor::MoveToNextLine(1)
            )?;
        }
        out.flush()?;
        Ok(())
    }

    fn hold(&mut self) -> Result<()> {
        loop {
            match crossterm::event::read().context("reading the terminal")? {
                Event::Key(KeyEvent {
                    kind: KeyEventKind::Press,
                    ..
                }) => return Ok(()),
                Event::Key(_)
                | Event::Paste(_)
                | Event::Mouse(_)
                | Event::Resize(_, _)
                | Event::FocusGained
                | Event::FocusLost => {}
            }
        }
    }
}

/// Map a terminal event onto a drill key.
///
/// Only a bare character reaches the buffer. A modified key is either one of the
/// three commands below or nothing at all, which is what stops an escape
/// sequence being typed into a secret.
pub fn key_of(event: &Event) -> Option<Key> {
    match event {
        Event::Paste(_) => Some(Key::Paste),
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
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "upstream enum; anything unmapped is deliberately nothing"
)]
fn code_of(code: KeyCode, modifiers: KeyModifiers) -> Option<Key> {
    let control = modifiers.contains(KeyModifiers::CONTROL);
    match code {
        KeyCode::Char('c' | 'd') if control => Some(Key::Interrupt),
        KeyCode::Char('u') if control => Some(Key::Clear),
        KeyCode::Char(character) if modifiers.difference(KeyModifiers::SHIFT).is_empty() => {
            Some(Key::Char(character))
        }
        KeyCode::Backspace => Some(Key::Backspace),
        KeyCode::Enter => Some(Key::Enter),
        KeyCode::Esc => Some(Key::Escape),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{Event, KeyCode, KeyEvent, KeyEventKind, KeyEventState, KeyModifiers};

    use super::{Key, RawLines, RawMode, key_of};

    /// A guard that took nothing, so putting it back is a flush and no more.
    /// Constructing one is what the borrow in `RawLines` asks for.
    fn through_raw_lines(input: &str) -> String {
        use std::io::Write as _;
        let guard = RawMode;
        let mut lines = RawLines {
            inner: Vec::new(),
            _guard: &guard,
        };
        lines
            .write_all(input.as_bytes())
            .expect("a vector cannot fail to be written to");
        String::from_utf8(lines.inner).expect("what went in was text")
    }

    #[test]
    fn a_line_written_in_raw_mode_returns_to_column_zero() {
        assert_eq!(through_raw_lines("secret: "), "secret: ");
        assert_eq!(through_raw_lines("\n"), "\r\n");
        assert_eq!(
            through_raw_lines("  a paste was refused\n  again: "),
            "  a paste was refused\r\n  again: "
        );
    }

    #[test]
    fn a_line_that_already_returns_is_left_alone() {
        assert_eq!(through_raw_lines("\r\n"), "\r\n");
        assert_eq!(through_raw_lines("one\r\ntwo\n"), "one\r\ntwo\r\n");
    }

    #[test]
    fn every_line_is_ended_rather_than_only_the_last() {
        assert_eq!(through_raw_lines("one\ntwo\nthree"), "one\r\ntwo\r\nthree");
    }

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
            key_of(&press(KeyCode::Char('a'), KeyModifiers::NONE)),
            Some(Key::Char('a'))
        );
        assert_eq!(
            key_of(&press(KeyCode::Char('A'), KeyModifiers::SHIFT)),
            Some(Key::Char('A'))
        );
        assert_eq!(
            key_of(&press(KeyCode::Char(' '), KeyModifiers::NONE)),
            Some(Key::Char(' '))
        );
    }

    #[test]
    fn the_three_commands_are_the_only_modified_keys_that_do_anything() {
        assert_eq!(
            key_of(&press(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(Key::Interrupt)
        );
        assert_eq!(
            key_of(&press(KeyCode::Char('d'), KeyModifiers::CONTROL)),
            Some(Key::Interrupt)
        );
        assert_eq!(
            key_of(&press(KeyCode::Char('u'), KeyModifiers::CONTROL)),
            Some(Key::Clear)
        );
        assert_eq!(
            key_of(&press(KeyCode::Char('a'), KeyModifiers::CONTROL)),
            None,
            "an unmapped control key must not become a character"
        );
        assert_eq!(key_of(&press(KeyCode::Char('a'), KeyModifiers::ALT)), None);
    }

    #[test]
    fn submitting_skipping_and_correcting_map_across() {
        assert_eq!(
            key_of(&press(KeyCode::Enter, KeyModifiers::NONE)),
            Some(Key::Enter)
        );
        assert_eq!(
            key_of(&press(KeyCode::Esc, KeyModifiers::NONE)),
            Some(Key::Escape)
        );
        assert_eq!(
            key_of(&press(KeyCode::Backspace, KeyModifiers::NONE)),
            Some(Key::Backspace)
        );
    }

    #[test]
    fn a_paste_arrives_as_a_paste_rather_than_as_characters() {
        assert_eq!(
            key_of(&Event::Paste("hunter2".to_owned())),
            Some(Key::Paste)
        );
    }

    #[test]
    fn a_key_release_is_not_a_keystroke() {
        assert_eq!(
            key_of(&Event::Key(KeyEvent {
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
        assert_eq!(key_of(&Event::Resize(80, 24)), None);
        assert_eq!(key_of(&Event::FocusGained), None);
        assert_eq!(key_of(&press(KeyCode::F(1), KeyModifiers::NONE)), None);
        assert_eq!(key_of(&press(KeyCode::Left, KeyModifiers::NONE)), None);
    }
}
