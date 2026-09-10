//! The session: what to ask for, and the loop that asks.
//!
//! The loop is generic over where keys come from and where the card is painted,
//! so every path through it — pass, lapse, retry, refused paste, skip, abort —
//! is driven by a scripted list of keys in the tests. Only raw mode, the
//! alternate screen and the real `event::read()` sit outside that, in `term`.

pub mod screen;
pub mod term;

use anyhow::Result;
use jiff::civil::Date;

use crate::config::Filler;
use crate::ladder::{self, Class, Standing, Step};
use crate::log::Outcome;
use crate::model::{SlugState, State};
use crate::secret::Secret;
use crate::slug::Slug;
use crate::store::Verifiers;
use crate::verifier::Verifier;

/// A key, as the drill understands it. Anything else the terminal sends is
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
    /// Pass over this slug.
    Escape,
    /// Abandon the sitting.
    Interrupt,
    /// Start the entry over.
    Clear,
    /// A bracketed paste arrived. It is refused: the drill must be typed, and a
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

/// Both halves of a terminal, so the loop borrows it once.
pub trait Console: Input + Screen {}

impl<T: Input + Screen> Console for T {}

/// Where the card is painted.
pub trait Screen {
    /// Draw a frame.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be written to.
    fn paint(&mut self, frame: &screen::Frame<'_>) -> Result<()>;

    /// Hold the finished card until a key is pressed.
    ///
    /// The session leaves no scrollback, so this is the only chance to read it.
    ///
    /// # Errors
    ///
    /// When the terminal cannot be read.
    fn hold(&mut self) -> Result<()>;
}

/// One slug to ask about.
#[derive(Clone, Debug)]
pub struct Item {
    /// The slug.
    pub slug: Slug,
    /// What an entry on it will count as.
    pub class: Class,
    /// Which verifier generation is current.
    pub version: u32,
    /// Where it sits on the ladder.
    pub step: Step,
    /// What the ladder asked for.
    pub scheduled_interval_days: u32,
    /// Days since the last scheduled review.
    pub actual_interval_days: Option<u32>,
    /// Days since the last entry of any kind.
    pub effective_interval_days: Option<u32>,
    /// The verifier, when the machine still has one.
    pub verifier: Option<Verifier>,
}

impl Item {
    /// Whether the effective interval reaches long-horizon evidence.
    pub fn stretch(&self) -> bool {
        self.effective_interval_days
            .is_some_and(|days| ladder::is_stretch(self.scheduled_interval_days, days))
    }

    /// The step this slug lands on after an outcome on the first try.
    pub fn step_after(&self, outcome: Outcome) -> Step {
        if !self.class.scores() {
            return self.step;
        }
        match outcome {
            Outcome::Pass => self.step.advanced(),
            Outcome::Fail | Outcome::Blank => Step::FIRST,
            Outcome::Skip | Outcome::Abort => self.step,
        }
    }
}

/// What the session will ask for.
#[derive(Clone, Debug, Default)]
pub struct Plan {
    /// In the order they will be asked, criticals first.
    pub items: Vec<Item>,
}

impl Plan {
    /// Whether there is nothing to do.
    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// The slugs that would be asked about but have no verifier on this machine.
    pub fn missing(&self) -> impl Iterator<Item = &Item> {
        self.items.iter().filter(|item| item.verifier.is_none())
    }
}

/// What kind of sitting this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The daily call: what is due, plus whatever the filler policy adds.
    Daily(Filler),
    /// Voluntary extra. Nothing here scores, whatever the schedule says.
    Practice,
}

/// Decide what to ask for.
///
/// `only` narrows the sitting to named slugs; empty means all of them.
pub fn plan(state: &State, verifiers: &Verifiers, today: Date, mode: Mode, only: &[Slug]) -> Plan {
    let items = state
        .scheduled()
        .into_iter()
        .filter(|slug| only.is_empty() || only.contains(&slug.slug))
        .filter_map(|slug| classify(slug, today, mode, !only.is_empty()))
        .map(|(slug, class)| item(slug, class, today, verifiers))
        .collect();
    Plan { items }
}

fn classify(slug: &SlugState, today: Date, mode: Mode, named: bool) -> Option<(&SlugState, Class)> {
    let standing = slug.standing(today);
    match mode {
        // A named slug is always asked about, however it stands: the tool
        // records what a person did, it does not decline to watch.
        Mode::Practice => Some((slug, practice_class(standing, named)?)),
        Mode::Daily(filler) => match standing {
            Standing::Due => Some((slug, Class::Review)),
            Standing::Probe => Some((slug, Class::Probe)),
            Standing::Held { .. } => named.then_some((slug, Class::Practice)),
            Standing::Waiting { .. } if named => Some((slug, Class::Practice)),
            // The filler is once a day per slug. A slug enrolled or drilled
            // this morning has already been in front of a person, and a second
            // entry hours later measures transcription rather than recall.
            // Explicit practice is unlimited; this is the automatic path.
            Standing::Waiting { .. } if slug.seen_today(today) => None,
            Standing::Waiting { .. } => match filler {
                Filler::All => Some((slug, Class::Practice)),
                // A slug at the cap drops out of the filler, so that its next
                // review is a cold entry at the cap interval rather than a warm
                // one wearing the same label. Below the cap the ritual costs
                // nothing the gate will later want back.
                Filler::BelowCap => (!slug.step.at_cap()).then_some((slug, Class::Practice)),
                Filler::None => None,
            },
        },
    }
}

fn practice_class(standing: Standing, named: bool) -> Option<Class> {
    match standing {
        Standing::Probe => Some(Class::Probe),
        Standing::Due | Standing::Waiting { .. } => Some(Class::Practice),
        Standing::Held { .. } => named.then_some(Class::Practice),
    }
}

fn item(slug: &SlugState, class: Class, today: Date, verifiers: &Verifiers) -> Item {
    Item {
        slug: slug.slug.clone(),
        class,
        version: slug.version,
        step: slug.step,
        scheduled_interval_days: slug.step.interval(),
        actual_interval_days: slug.actual_interval(today),
        effective_interval_days: slug.effective_interval(today),
        verifier: verifiers.get(&slug.slug).ok().flatten(),
    }
}

/// One entry, as the session saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Taken {
    /// Which item in the plan.
    pub item: usize,
    /// What the entry counted as.
    pub class: Class,
    /// Which try within the sitting. Only the first scores.
    pub attempt: u8,
    /// How it ended.
    pub outcome: Outcome,
    /// Milliseconds from the prompt appearing to the first keystroke.
    pub ttfk_ms: Option<u64>,
    /// Milliseconds from the first keystroke to submission.
    pub total_ms: Option<u64>,
    /// Backspaces.
    pub corrections: u32,
    /// Pastes refused at this prompt.
    pub paste_refused: u32,
    /// Where the slug lands after this entry.
    pub step_after: Step,
}

/// How a whole sitting ended.
#[derive(Clone, Debug, Default)]
pub struct Sitting {
    /// Every entry, in the order they were taken.
    pub taken: Vec<Taken>,
    /// Whether the sitting was abandoned rather than finished.
    pub aborted: bool,
    /// The card as it stood at the end. The caller paints the closing frame,
    /// because what belongs under it — a gate that has just come ready — is a
    /// reading of the log after these entries, not of the sitting.
    pub rows: Vec<screen::Row>,
}

impl Sitting {
    /// First-attempt failures on entries that scored.
    pub fn lapses(&self) -> usize {
        self.taken
            .iter()
            .filter(|taken| taken.attempt == 1 && taken.class.unaided() && taken.outcome.lapsed())
            .count()
    }
}

/// Run the sitting.
///
/// `verify` is passed in rather than reached for, so the loop can be driven in a
/// test without paying 256 MiB and half a second per key.
///
/// # Errors
///
/// When the terminal cannot be read or written, or the verifier fails outright —
/// which is different from refusing a secret, and is not recorded as a lapse.
pub fn run(
    plan: &Plan,
    max_attempts: u8,
    console: &mut dyn Console,
    verify: &dyn Fn(&Item, &Secret) -> Result<bool>,
    today: Date,
) -> Result<Sitting> {
    let mut rows: Vec<screen::Row> = plan.items.iter().map(screen::Row::pending).collect();
    let mut sitting = Sitting::default();

    for (index, item) in plan.items.iter().enumerate() {
        if item.verifier.is_none() {
            set(&mut rows, index, screen::RowState::Unverifiable);
            continue;
        }
        let stop = ask_about(
            item,
            index,
            max_attempts,
            &mut rows,
            &mut sitting,
            console,
            verify,
            today,
        )?;
        if stop {
            sitting.rows = rows;
            return Ok(sitting);
        }
    }

    sitting.rows = rows;
    Ok(sitting)
}

/// Ask about one slug until it is answered, passed over, or out of tries.
///
/// `true` means the sitting was abandoned and nothing further should be asked.
#[expect(
    clippy::too_many_arguments,
    reason = "one prompt's worth of state, threaded rather than bundled into a struct that would exist only to be unpacked"
)]
fn ask_about(
    item: &Item,
    index: usize,
    max_attempts: u8,
    rows: &mut [screen::Row],
    sitting: &mut Sitting,
    console: &mut dyn Console,
    verify: &dyn Fn(&Item, &Secret) -> Result<bool>,
    today: Date,
) -> Result<bool> {
    let mut resolved: Option<Step> = None;
    let mut class = item.class;
    let mut attempt: u8 = 0;
    loop {
        attempt = attempt.saturating_add(1);
        let aided = !class.unaided();
        let left = if aided {
            0
        } else {
            max_attempts.saturating_sub(attempt)
        };
        set(rows, index, screen::RowState::Active { attempt, left });
        console.paint(&screen::Frame::running(today, rows, Some(index)))?;

        let entry = match read(console, today, rows, index)? {
            Typed::Submitted(entry) => entry,
            Typed::Skipped => {
                set(rows, index, screen::RowState::Skipped);
                sitting
                    .taken
                    .push(nothing(item, index, class, attempt, Outcome::Skip));
                return Ok(false);
            }
            Typed::Aborted => {
                set(rows, index, screen::RowState::Aborted);
                sitting
                    .taken
                    .push(nothing(item, index, class, attempt, Outcome::Abort));
                sitting.aborted = true;
                return Ok(true);
            }
        };

        // An empty entry is a concession, and the verifier has nothing to say
        // about it. Conceding is what the card asks for in place of typing
        // something to get past the prompt.
        let outcome = if entry.secret.is_empty() {
            Outcome::Blank
        } else {
            set(rows, index, screen::RowState::Checking);
            console.paint(&screen::Frame::running(today, rows, Some(index)))?;
            if verify(item, &entry.secret)? {
                Outcome::Pass
            } else {
                Outcome::Fail
            }
        };
        drop(entry.secret);

        // An aided entry cannot move the ladder, so it carries whatever this
        // sitting already decided rather than a position of its own.
        let step_after = if aided {
            resolved.unwrap_or(item.step)
        } else {
            *resolved.get_or_insert_with(|| item.step_after(outcome))
        };
        sitting.taken.push(Taken {
            item: index,
            class,
            attempt,
            outcome,
            ttfk_ms: entry.ttfk_ms,
            total_ms: entry.total_ms,
            corrections: entry.corrections,
            paste_refused: entry.paste_refused,
            step_after,
        });

        if outcome == Outcome::Pass {
            set(
                rows,
                index,
                screen::RowState::Passed {
                    total_ms: entry.total_ms,
                    retries: attempt.saturating_sub(1),
                },
            );
            return Ok(false);
        }
        set(
            rows,
            index,
            screen::RowState::Failed {
                attempt,
                left,
                scored: attempt == 1 && !aided,
            },
        );
        if aided {
            // The answer was in front of the person and the verifier refused it
            // anyway. Nothing further to ask, and something else to look at.
            return Ok(false);
        }

        match after_miss(console, today, rows, index, left > 0)? {
            Choice::Retry => {}
            Choice::Look => {
                class = Class::Aided;
                if let Some(row) = rows.get_mut(index) {
                    row.class = Class::Aided;
                }
            }
            Choice::Move => return Ok(false),
            Choice::Abort => {
                set(rows, index, screen::RowState::Aborted);
                sitting.aborted = true;
                return Ok(true);
            }
        }
    }
}

/// Offer the lookup, and read the answer.
fn after_miss(
    console: &mut dyn Console,
    today: Date,
    rows: &[screen::Row],
    index: usize,
    retry: bool,
) -> Result<Choice> {
    let mut frame = screen::Frame::running(today, rows, Some(index));
    frame.notice = Some(screen::Notice::Choose { retry });
    console.paint(&frame)?;
    choose(console, retry)
}

/// What to do after a miss.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Choice {
    /// Try again, from memory.
    Retry,
    /// Go and look it up, then type it as an aided entry.
    Look,
    /// Leave it there and go on to the next slug.
    Move,
    /// Abandon the sitting.
    Abort,
}

/// Read the one keystroke that decides what happens after a miss.
///
/// Anything unrecognised is ignored rather than guessed at: this prompt decides
/// what the next record claims to measure.
fn choose(console: &mut dyn Console, retry: bool) -> Result<Choice> {
    loop {
        let Some(key) = console.next()? else {
            return Ok(Choice::Abort);
        };
        match key {
            Key::Enter => return Ok(if retry { Choice::Retry } else { Choice::Move }),
            Key::Char('l' | 'L') => return Ok(Choice::Look),
            Key::Char('s' | 'S') | Key::Escape => return Ok(Choice::Move),
            Key::Interrupt => return Ok(Choice::Abort),
            Key::Char(_) | Key::Backspace | Key::Clear | Key::Paste => {}
        }
    }
}

/// An entry where nothing was typed, so nothing was measured.
fn nothing(item: &Item, index: usize, class: Class, attempt: u8, outcome: Outcome) -> Taken {
    Taken {
        item: index,
        class,
        attempt,
        outcome,
        ttfk_ms: None,
        total_ms: None,
        corrections: 0,
        paste_refused: 0,
        step_after: item.step,
    }
}

fn set(rows: &mut [screen::Row], index: usize, state: screen::RowState) {
    if let Some(row) = rows.get_mut(index) {
        row.state = state;
    }
}

/// What one prompt produced.
enum Typed {
    Submitted(Entry),
    Skipped,
    Aborted,
}

/// One submitted entry, before anything has judged it.
struct Entry {
    secret: Secret,
    ttfk_ms: Option<u64>,
    total_ms: Option<u64>,
    corrections: u32,
    paste_refused: u32,
}

fn read(
    console: &mut dyn Console,
    today: Date,
    rows: &[screen::Row],
    index: usize,
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
                let mut frame = screen::Frame::running(today, rows, Some(index));
                frame.notice = Some(screen::Notice::PasteRefused);
                console.paint(&frame)?;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use jiff::civil::{Date, date};

    use super::{Input, Item, Key, Mode, Plan, Screen, Sitting, screen};
    use crate::config::Filler;
    use crate::ladder::{Class, Step};
    use crate::log::Outcome;
    use crate::secret::Secret;
    use crate::verifier::Verifier;

    const PHC: &str = "$argon2id$v=19$m=64,t=1,p=1$CQkJCQkJCQkJCQkJCQkJCQ$\
                       PdWLZDvNGYPMYWPfbSZq7yZO0eFRHmiGnLTuNBAxUC0";

    /// A scripted terminal: keys with the elapsed time each arrived at, and a
    /// record of everything painted.
    struct Fake {
        keys: Vec<(Key, u64)>,
        at: usize,
        elapsed: u64,
        frames: usize,
        held: usize,
        last: Vec<String>,
    }

    impl Fake {
        fn new(keys: Vec<(Key, u64)>) -> Self {
            Self {
                keys,
                at: 0,
                elapsed: 0,
                frames: 0,
                held: 0,
                last: Vec::new(),
            }
        }
    }

    impl Input for Fake {
        fn arm(&mut self) {
            self.elapsed = 0;
        }

        fn next(&mut self) -> Result<Option<Key>> {
            let Some((key, at)) = self.keys.get(self.at).copied() else {
                return Ok(None);
            };
            self.at = self.at.saturating_add(1);
            self.elapsed = at;
            Ok(Some(key))
        }

        fn elapsed_ms(&self) -> u64 {
            self.elapsed
        }
    }

    impl Screen for Fake {
        fn paint(&mut self, frame: &screen::Frame<'_>) -> Result<()> {
            self.frames = self.frames.saturating_add(1);
            self.last = screen::render(frame, false);
            Ok(())
        }

        fn hold(&mut self) -> Result<()> {
            self.held = self.held.saturating_add(1);
            Ok(())
        }
    }

    fn item(name: &str, class: Class, step: Step) -> Item {
        Item {
            slug: name.parse().unwrap(),
            class,
            version: 1,
            step,
            scheduled_interval_days: step.interval(),
            actual_interval_days: Some(7),
            effective_interval_days: Some(7),
            verifier: Some(Verifier::parse(PHC).unwrap()),
        }
    }

    fn typing(text: &str, from: u64) -> Vec<(Key, u64)> {
        let mut keys = vec![];
        let mut at = from;
        for character in text.chars() {
            keys.push((Key::Char(character), at));
            at = at.saturating_add(100);
        }
        keys.push((Key::Enter, at));
        keys
    }

    fn drive(
        plan: &Plan,
        keys: Vec<(Key, u64)>,
        max_attempts: u8,
        accept: &dyn Fn(&Secret) -> bool,
    ) -> (Sitting, Fake) {
        let mut console = Fake::new(keys);
        let sitting = super::run(
            plan,
            max_attempts,
            &mut console,
            &|_item, secret| Ok(accept(secret)),
            date(2026, 9, 10),
        )
        .unwrap();
        // The caller paints the closing frame; do here what `cmd` does there.
        console
            .paint(&screen::Frame::done(
                date(2026, 9, 10),
                &sitting.rows,
                &sitting,
                &[],
            ))
            .unwrap();
        console.hold().unwrap();
        (sitting, console)
    }

    /// Answers to the offer that follows a miss.
    const RETRY: (Key, u64) = (Key::Enter, 0);
    const LOOK: (Key, u64) = (Key::Char('l'), 0);
    const MOVE_ON: (Key, u64) = (Key::Char('s'), 0);

    fn one(class: Class) -> Plan {
        Plan {
            items: vec![item("a", class, Step::cap())],
        }
    }

    #[test]
    fn a_clean_entry_records_its_timings() {
        let plan = one(Class::Review);
        let (sitting, console) = drive(&plan, typing("abc", 1_200), 3, &|_| true);
        let taken = sitting.taken.first().unwrap();
        assert_eq!(taken.outcome, Outcome::Pass);
        assert_eq!(taken.attempt, 1);
        assert_eq!(taken.ttfk_ms, Some(1_200));
        assert_eq!(
            taken.total_ms,
            Some(300),
            "three keys and a submit at 100ms apart"
        );
        assert_eq!(taken.corrections, 0);
        assert_eq!(taken.step_after, Step::cap());
        assert_eq!(console.held, 1, "the card waits before it disappears");
    }

    #[test]
    fn the_typed_secret_is_what_reaches_the_verifier() {
        let plan = one(Class::Review);
        let mut console = Fake::new(typing("hunter2", 0));
        let seen = std::cell::RefCell::new(Vec::new());
        super::run(
            &plan,
            1,
            &mut console,
            &|_item, secret| {
                seen.borrow_mut().push(secret.expose().to_vec());
                Ok(true)
            },
            date(2026, 9, 10),
        )
        .unwrap();
        assert_eq!(seen.borrow().first().unwrap(), b"hunter2");
    }

    #[test]
    fn a_correction_is_counted_and_applied() {
        let plan = one(Class::Review);
        let keys = vec![
            (Key::Char('a'), 500),
            (Key::Char('x'), 600),
            (Key::Backspace, 700),
            (Key::Char('b'), 800),
            (Key::Enter, 900),
        ];
        let seen = std::cell::RefCell::new(Vec::new());
        let mut console = Fake::new(keys);
        let sitting = super::run(
            &plan,
            1,
            &mut console,
            &|_item, secret| {
                seen.borrow_mut().push(secret.expose().to_vec());
                Ok(true)
            },
            date(2026, 9, 10),
        )
        .unwrap();
        assert_eq!(seen.borrow().first().unwrap(), b"ab");
        assert_eq!(sitting.taken.first().unwrap().corrections, 1);
    }

    #[test]
    fn a_correction_on_an_empty_buffer_is_not_counted() {
        let plan = one(Class::Review);
        let keys = vec![
            (Key::Backspace, 100),
            (Key::Char('a'), 200),
            (Key::Enter, 300),
        ];
        let (sitting, _) = drive(&plan, keys, 1, &|_| true);
        assert_eq!(sitting.taken.first().unwrap().corrections, 0);
    }

    #[test]
    fn a_paste_is_refused_and_counted_and_leaves_the_buffer_alone() {
        let plan = one(Class::Review);
        let keys = vec![
            (Key::Char('a'), 100),
            (Key::Paste, 200),
            (Key::Char('b'), 300),
            (Key::Enter, 400),
        ];
        let seen = std::cell::RefCell::new(Vec::new());
        let mut console = Fake::new(keys);
        let sitting = super::run(
            &plan,
            1,
            &mut console,
            &|_item, secret| {
                seen.borrow_mut().push(secret.expose().to_vec());
                Ok(true)
            },
            date(2026, 9, 10),
        )
        .unwrap();
        assert_eq!(seen.borrow().first().unwrap(), b"ab");
        assert_eq!(sitting.taken.first().unwrap().paste_refused, 1);
    }

    #[test]
    fn a_lapse_resets_the_step_and_the_retry_does_not_undo_it() {
        let plan = one(Class::Review);
        let mut keys = typing("wrong", 500);
        keys.push(RETRY);
        keys.extend(typing("right", 2_000));
        let attempts = std::cell::Cell::new(0u32);
        let (sitting, _) = drive(&plan, keys, 3, &|_| {
            attempts.set(attempts.get().saturating_add(1));
            attempts.get() > 1
        });
        assert_eq!(sitting.taken.len(), 2);
        assert_eq!(sitting.lapses(), 1);
        let first = sitting.taken.first().unwrap();
        let second = sitting.taken.get(1).unwrap();
        assert_eq!(first.outcome, Outcome::Fail);
        assert_eq!(second.outcome, Outcome::Pass);
        assert_eq!(second.attempt, 2);
        assert_eq!(
            second.step_after,
            Step::FIRST,
            "the first try decided it, and a second entry is primed by the first"
        );
    }

    #[test]
    fn tries_run_out_rather_than_looping_forever() {
        let plan = one(Class::Review);
        let mut keys = vec![];
        for _ in 0..5 {
            keys.extend(typing("nope", 100));
            keys.push(RETRY);
        }
        let (sitting, _) = drive(&plan, keys, 3, &|_| false);
        assert_eq!(sitting.taken.len(), 3);
        assert!(!sitting.aborted);
    }

    #[test]
    fn an_empty_entry_is_a_blank_and_the_verifier_is_never_asked() {
        let plan = one(Class::Review);
        let asked = std::cell::Cell::new(0u32);
        let mut console = Fake::new(vec![(Key::Enter, 100), MOVE_ON]);
        let sitting = super::run(
            &plan,
            2,
            &mut console,
            &|_item, _secret| {
                asked.set(asked.get().saturating_add(1));
                Ok(true)
            },
            date(2026, 9, 10),
        )
        .unwrap();
        let taken = sitting.taken.first().unwrap();
        assert_eq!(taken.outcome, Outcome::Blank);
        assert_eq!(taken.step_after, Step::FIRST, "conceding is a lapse");
        assert_eq!(asked.get(), 0, "there was nothing to check");
        assert_eq!(sitting.lapses(), 1);
    }

    #[test]
    fn looking_it_up_makes_the_next_entry_aided_and_ends_the_item() {
        let plan = one(Class::Review);
        let mut keys = typing("wrong", 100);
        keys.push(LOOK);
        keys.extend(typing("right", 2_000));
        // A third entry would be asked for if the aided one did not end it.
        keys.extend(typing("again", 4_000));
        let attempts = std::cell::Cell::new(0u32);
        let (sitting, _) = drive(&plan, keys, 3, &|_| {
            attempts.set(attempts.get().saturating_add(1));
            attempts.get() > 1
        });
        assert_eq!(sitting.taken.len(), 2);
        let cold = sitting.taken.first().unwrap();
        let aided = sitting.taken.get(1).unwrap();
        assert_eq!(cold.class, Class::Review);
        assert_eq!(aided.class, Class::Aided);
        assert_eq!(aided.outcome, Outcome::Pass);
        assert_eq!(
            aided.step_after,
            Step::FIRST,
            "the cold attempt decided the step, and the lookup cannot undo it"
        );
        assert_eq!(sitting.lapses(), 1, "an aided pass is not a rescue");
    }

    #[test]
    fn an_aided_entry_that_is_refused_says_so_and_asks_nothing_further() {
        let plan = one(Class::Review);
        let mut keys = typing("wrong", 100);
        keys.push(LOOK);
        keys.extend(typing("also-wrong", 2_000));
        keys.extend(typing("never-reached", 4_000));
        let (sitting, _) = drive(&plan, keys, 3, &|_| false);
        assert_eq!(sitting.taken.len(), 2);
        let aided = sitting.taken.get(1).unwrap();
        assert_eq!(aided.class, Class::Aided);
        assert_eq!(aided.outcome, Outcome::Fail);
        assert_eq!(
            sitting.lapses(),
            1,
            "the cold miss is the lapse; the aided one is a reading about the vault"
        );
    }

    #[test]
    fn moving_on_after_a_miss_leaves_the_slug_where_it_fell() {
        let plan = one(Class::Review);
        let mut keys = typing("wrong", 100);
        keys.push(MOVE_ON);
        keys.extend(typing("never-reached", 2_000));
        let (sitting, _) = drive(&plan, keys, 3, &|_| false);
        assert_eq!(sitting.taken.len(), 1);
        assert!(!sitting.aborted);
    }

    #[test]
    fn practice_never_moves_the_step_in_either_direction() {
        for outcome in [true, false] {
            let plan = one(Class::Practice);
            let (sitting, _) = drive(&plan, typing("x", 0), 1, &|_| outcome);
            assert_eq!(sitting.taken.first().unwrap().step_after, Step::cap());
        }
    }

    #[test]
    fn escape_passes_over_a_slug_and_the_sitting_carries_on() {
        let plan = Plan {
            items: vec![
                item("a", Class::Review, Step::cap()),
                item("b", Class::Review, Step::cap()),
            ],
        };
        let mut keys = vec![(Key::Escape, 100)];
        keys.extend(typing("x", 200));
        let (sitting, _) = drive(&plan, keys, 3, &|_| true);
        assert_eq!(sitting.taken.len(), 2);
        assert_eq!(sitting.taken.first().unwrap().outcome, Outcome::Skip);
        assert_eq!(sitting.taken.get(1).unwrap().outcome, Outcome::Pass);
        assert!(!sitting.aborted);
    }

    #[test]
    fn an_interrupt_abandons_the_sitting_and_asks_nothing_more() {
        let plan = Plan {
            items: vec![
                item("a", Class::Review, Step::cap()),
                item("b", Class::Review, Step::cap()),
            ],
        };
        let (sitting, console) = drive(&plan, vec![(Key::Interrupt, 100)], 3, &|_| true);
        assert!(sitting.aborted);
        assert_eq!(sitting.taken.len(), 1);
        assert_eq!(sitting.taken.first().unwrap().outcome, Outcome::Abort);
        assert_eq!(console.held, 1);
    }

    #[test]
    fn input_running_out_reads_as_an_abandoned_sitting() {
        let plan = one(Class::Review);
        let (sitting, _) = drive(&plan, vec![(Key::Char('a'), 10)], 3, &|_| true);
        assert!(sitting.aborted);
    }

    #[test]
    fn a_slug_with_no_verifier_is_shown_and_never_prompted() {
        let mut plan = one(Class::Review);
        if let Some(first) = plan.items.first_mut() {
            first.verifier = None;
        }
        let (sitting, console) = drive(&plan, typing("x", 0), 3, &|_| true);
        assert!(sitting.taken.is_empty());
        assert_eq!(plan.missing().count(), 1);
        assert!(
            console.last.iter().any(|line| line.contains("re-enrol")),
            "{:?}",
            console.last
        );
    }

    #[test]
    fn no_frame_ever_carries_what_was_typed() {
        let plan = one(Class::Review);
        let (_, console) = drive(&plan, typing("hunter2", 0), 1, &|_| true);
        for line in &console.last {
            assert!(!line.contains("hunter"), "{line}");
            assert!(!line.contains('h') || !line.contains('2'), "{line}");
        }
    }

    // Planning.

    use crate::log::{Added, Digest, Event, Line, Record, SCHEMA};
    use crate::model::State;
    use crate::store::Verifiers;

    fn added(day: Date, name: &str) -> Line {
        Line::Parsed(Box::new(Record {
            v: SCHEMA,
            at: day.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp(),
            day,
            host: "Mac".to_owned(),
            prev: Digest::GENESIS,
            event: Event::Add(Added {
                slug: name.parse().unwrap(),
                version: 1,
                critical: false,
            }),
        }))
    }

    fn seeded(names: &[(&str, bool)]) -> (State, Verifiers) {
        let lines: Vec<Line> = names
            .iter()
            .map(|(name, critical)| {
                Line::Parsed(Box::new(Record {
                    v: SCHEMA,
                    at: "2026-09-01T00:00:00Z".parse().unwrap(),
                    day: date(2026, 9, 1),
                    host: "Mac".to_owned(),
                    prev: Digest::GENESIS,
                    event: Event::Add(Added {
                        slug: name.parse().unwrap(),
                        version: 1,
                        critical: *critical,
                    }),
                }))
            })
            .collect();
        let mut verifiers = Verifiers::default();
        for (name, _) in names {
            verifiers.set(&name.parse().unwrap(), &Verifier::parse(PHC).unwrap());
        }
        (State::replay(&lines), verifiers)
    }

    fn classes(plan: &Plan) -> Vec<(&str, Class)> {
        plan.items
            .iter()
            .map(|item| (item.slug.as_str(), item.class))
            .collect()
    }

    fn on(day: Date, state: &State, verifiers: &Verifiers, mode: Mode) -> Plan {
        super::plan(state, verifiers, day, mode, &[])
    }

    #[test]
    fn a_due_slug_is_a_review_and_a_waiting_one_below_the_cap_is_filler() {
        let (_, verifiers) = seeded(&[("a", false)]);
        // Walked up to step 3, so the next review is four days out.
        let state = State::replay(&[
            added(date(2026, 9, 1), "a"),
            review(date(2026, 9, 2), "a", 0, 1),
            review(date(2026, 9, 3), "a", 1, 2),
            review(date(2026, 9, 5), "a", 2, 3),
        ]);
        assert_eq!(
            classes(&on(
                date(2026, 9, 6),
                &state,
                &verifiers,
                Mode::Daily(Filler::BelowCap)
            )),
            vec![("a", Class::Practice)],
            "still climbing, so the ritual costs nothing"
        );
        assert_eq!(
            classes(&on(
                date(2026, 9, 9),
                &state,
                &verifiers,
                Mode::Daily(Filler::BelowCap)
            )),
            vec![("a", Class::Review)]
        );
        assert!(
            on(
                date(2026, 9, 6),
                &state,
                &verifiers,
                Mode::Daily(Filler::None)
            )
            .is_empty(),
            "filler none asks for nothing beyond what is due"
        );
    }

    #[test]
    fn the_filler_asks_once_a_day_and_not_twice() {
        let (state, verifiers) = seeded(&[("a", false)]);
        assert!(
            on(
                date(2026, 9, 1),
                &state,
                &verifiers,
                Mode::Daily(Filler::BelowCap)
            )
            .is_empty(),
            "enrolment today is today's exposure"
        );
        assert_eq!(
            classes(&on(date(2026, 9, 1), &state, &verifiers, Mode::Practice)),
            vec![("a", Class::Practice)],
            "asking outright still works"
        );
    }

    #[test]
    fn a_slug_at_the_cap_drops_out_of_the_filler() {
        let (_, verifiers) = seeded(&[("a", false)]);
        // Walk it to the cap through the ladder.
        let mut records = vec![added(date(2026, 9, 1), "a")];
        let mut day = date(2026, 9, 2);
        for step in 0..4u8 {
            records.push(review(day, "a", step, step + 1));
            day = day
                .checked_add(
                    jiff::Span::new().days(i64::from(Step::from_recorded(step + 1).interval())),
                )
                .unwrap();
        }
        let state = State::replay(&records);
        assert!(state.get(&"a".parse().unwrap()).unwrap().step.at_cap());
        let before_due = day.checked_sub(jiff::Span::new().days(1)).unwrap();
        assert!(
            on(
                before_due,
                &state,
                &verifiers,
                Mode::Daily(Filler::BelowCap)
            )
            .is_empty(),
            "so it can earn a cold entry at the cap interval"
        );
        assert_eq!(
            classes(&on(
                before_due,
                &state,
                &verifiers,
                Mode::Daily(Filler::All)
            )),
            vec![("a", Class::Practice)],
            "unless the ritual was asked for outright"
        );
    }

    fn review(day: Date, name: &str, before: u8, after: u8) -> Line {
        Line::Parsed(Box::new(Record {
            v: SCHEMA,
            at: day.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp(),
            day,
            host: "Mac".to_owned(),
            prev: Digest::GENESIS,
            event: Event::Attempt(crate::log::Attempted {
                slug: name.parse().unwrap(),
                session: crate::log::SessionId::from_bytes([0; 8]),
                version: 1,
                class: Class::Review,
                attempt: 1,
                outcome: Outcome::Pass,
                ttfk_ms: Some(900),
                total_ms: Some(3_000),
                corrections: 0,
                paste_refused: 0,
                scheduled_interval_days: Step::from_recorded(before).interval(),
                actual_interval_days: Some(7),
                effective_interval_days: Some(7),
                stretch: false,
                step_before: before,
                step_after: after,
            }),
        }))
    }

    #[test]
    fn practice_mode_asks_about_everything_and_scores_none_of_it() {
        let (state, verifiers) = seeded(&[("a", false), ("b", false)]);
        let plan = on(date(2026, 9, 5), &state, &verifiers, Mode::Practice);
        assert_eq!(
            classes(&plan),
            vec![("a", Class::Practice), ("b", Class::Practice)]
        );
        assert!(plan.items.iter().all(|item| !item.class.scores()));
    }

    #[test]
    fn criticals_come_first_and_the_order_is_fixed() {
        let (state, verifiers) = seeded(&[("zeta", false), ("alpha", false), ("escrow-p", true)]);
        let plan = on(date(2026, 9, 5), &state, &verifiers, Mode::Practice);
        let order: Vec<&str> = plan.items.iter().map(|i| i.slug.as_str()).collect();
        assert_eq!(order, vec!["escrow-p", "alpha", "zeta"]);
    }

    #[test]
    fn naming_a_slug_narrows_the_sitting_to_it() {
        let (state, verifiers) = seeded(&[("a", false), ("b", false)]);
        let only = vec!["b".parse().unwrap()];
        let plan = super::plan(&state, &verifiers, date(2026, 9, 5), Mode::Practice, &only);
        assert_eq!(classes(&plan), vec![("b", Class::Practice)]);
    }

    #[test]
    fn a_slug_missing_its_verifier_still_reaches_the_plan_so_it_can_be_reported() {
        let (state, _) = seeded(&[("a", false)]);
        let plan = on(
            date(2026, 9, 5),
            &state,
            &Verifiers::default(),
            Mode::Practice,
        );
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.missing().count(), 1);
    }
}
