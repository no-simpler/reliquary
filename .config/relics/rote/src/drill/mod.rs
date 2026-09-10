//! The session: what to ask for, and the loop that asks.
//!
//! The loop is generic over where keys come from, where the card is painted,
//! and where each entry is written, so every path through it — pass, lapse,
//! retry, refused paste, out of tries, skip, abort — is driven by a scripted
//! list of keys in the tests. Only raw mode, the alternate screen and the real
//! `event::read()` sit outside that, in `tui`.

pub mod screen;

use anyhow::Result;
use jiff::civil::Date;
use relic_core::style::Style;

use crate::config::Filler;
use crate::ladder::{self, Class, Standing, Step};
use crate::log::Outcome;
use crate::model::{SlugState, State};
use crate::secret::Secret;
use crate::slug::Slug;
use crate::store::Verifiers;
use crate::tui::{Card, Console, Typed, card::Tone, read_secret};
use crate::verifier::Verifier;

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
    /// Days since the schedule's anchor.
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
/// `only` narrows a practice sitting to named slugs; empty means all of them.
/// A named slug is always asked about, however it stands: the tool records
/// what a person did, it does not decline to watch.
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
        Mode::Practice => Some((slug, practice_class(standing, named)?)),
        Mode::Daily(filler) => match standing {
            Standing::Due => Some((slug, Class::Review)),
            Standing::Probe => Some((slug, Class::Probe)),
            // Held out so that the horizon arrives cold.
            Standing::Held { .. } => None,
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
    pub fn missed(&self) -> usize {
        self.taken
            .iter()
            .filter(|taken| taken.attempt == 1 && taken.class.unaided() && taken.outcome.lapsed())
            .count()
    }

    /// How each slug that was asked about landed, in the order they were asked.
    ///
    /// One landing per slug and no slug in two of them, so the counts add up to
    /// what was actually drilled. Only the first try is read: it is the one that
    /// scores, and a second entry is primed by the first.
    pub fn landings(&self) -> Vec<Landed> {
        self.taken
            .iter()
            .filter(|taken| taken.attempt == 1)
            .filter_map(|taken| {
                if self.was_aided(taken.item) {
                    return Some(Landed::Aided);
                }
                Landed::of(taken)
            })
            .collect()
    }

    /// Whether the lookup was taken on this slug at any point.
    ///
    /// It re-labels the whole slug rather than only the entry that followed it.
    /// The row already says so, and a sitting that reported the cold try while
    /// the row beside it said aided would be telling two stories about one
    /// slug. Week one is lookups almost throughout, and calling that a run of
    /// lapses would be punishing the honest shape of week one.
    fn was_aided(&self, item: usize) -> bool {
        self.taken
            .iter()
            .any(|taken| taken.item == item && !taken.class.unaided())
    }
}

/// Where one slug ended up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landed {
    /// Answered, cold, first try.
    Pass,
    /// Missed, and the ladder went back to the foot.
    Lapse,
    /// Missed, and nothing moved: practice and probes carry no schedule.
    Miss,
    /// Passed over.
    Skipped,
    /// The answer was looked up first, so it measured nothing either way.
    Aided,
}

impl Landed {
    /// Where a first try leaves its slug, or `None` when the sitting was
    /// abandoned there and the slug got no reading at all.
    fn of(taken: &Taken) -> Option<Self> {
        if !taken.class.unaided() {
            return Some(Self::Aided);
        }
        Some(match taken.outcome {
            Outcome::Pass => Self::Pass,
            // A lapse is the ladder going back to the foot. Practice and probes
            // move nothing, so a miss there is a reading and not a lapse — which
            // is what `rote guide probes` says and what `rote stats` counts.
            Outcome::Fail | Outcome::Blank => {
                if taken.class.scores() {
                    Self::Lapse
                } else {
                    Self::Miss
                }
            }
            Outcome::Skip => Self::Skipped,
            Outcome::Abort => return None,
        })
    }
}

/// What the line under the field says once the cold tries are spent.
const OUT_OF_TRIES: &str = "out of tries";

/// Where each entry goes the moment it is taken.
///
/// Called before the next prompt is drawn, so a sitting cut short by a closed
/// window or a signal keeps every reading it had already produced. The log is
/// the product; a reading held in memory until the end is one a crash can eat.
pub type Record<'a> = dyn FnMut(&Taken) -> Result<()> + 'a;

/// How a secret is judged. Passed in rather than reached for, so the loop can
/// be driven in a test without paying 256 MiB and half a second per key.
pub type Verify<'a> = dyn Fn(&Item, &Secret) -> Result<bool> + 'a;

/// Run the sitting.
///
/// # Errors
///
/// When the terminal cannot be read or written, an entry cannot be recorded, or
/// the verifier fails outright — which is different from refusing a secret, and
/// is not recorded as a lapse.
pub fn run(
    plan: &Plan,
    max_attempts: u8,
    console: &mut dyn Console,
    verify: &Verify<'_>,
    record: &mut Record<'_>,
    today: Date,
) -> Result<Sitting> {
    Session {
        plan,
        max_attempts,
        console,
        verify,
        record,
        today,
        rows: plan.items.iter().map(screen::Row::pending).collect(),
        sitting: Sitting::default(),
    }
    .run()
}

/// One sitting in progress: the plan, the card as it stands, and where each
/// entry goes.
struct Session<'a> {
    plan: &'a Plan,
    max_attempts: u8,
    console: &'a mut dyn Console,
    verify: &'a Verify<'a>,
    record: &'a mut Record<'a>,
    today: Date,
    rows: Vec<screen::Row>,
    sitting: Sitting,
}

/// What the loop decided about one prompt, beyond the entry it produced.
enum Next {
    /// Ask this slug again.
    Again,
    /// Move to the next slug.
    Done,
    /// Ask nothing further of anyone.
    Stop,
}

impl Session<'_> {
    fn run(mut self) -> Result<Sitting> {
        // The rectangle is the same for every state of the sitting, so the
        // opening card's height is the dialog's height. Read off a rendered
        // card rather than kept as a second constant that could drift from it.
        let opening = screen::card(
            &screen::Frame::running(self.today, &self.rows, Some(0)),
            Style::PLAIN,
        );
        self.console.anchor(opening.height());

        for index in 0..self.plan.items.len() {
            let Some(item) = self.plan.items.get(index) else {
                break;
            };
            if item.verifier.is_none() {
                self.set(index, screen::RowState::Unverifiable);
                continue;
            }
            if self.ask_about(index, item)? {
                break;
            }
        }
        self.sitting.rows = self.rows;
        Ok(self.sitting)
    }

    /// Ask about one slug until it is answered, passed over, or walked away
    /// from. `true` means the sitting was abandoned.
    fn ask_about(&mut self, index: usize, item: &Item) -> Result<bool> {
        let mut prompt = Prompt::new(item.class);
        loop {
            match self.prompt(index, item, &mut prompt)? {
                Next::Again => {}
                Next::Done => return Ok(false),
                Next::Stop => return Ok(true),
            }
        }
    }

    /// One prompt: draw, read, judge, record.
    fn prompt(&mut self, index: usize, item: &Item, prompt: &mut Prompt) -> Result<Next> {
        prompt.attempt = prompt.attempt.saturating_add(1);
        let aided = !prompt.class.unaided();
        let lookup = prompt.missed && !aided;
        let status = prompt.status(self.max_attempts);
        self.set(
            index,
            screen::RowState::Active {
                attempt: prompt.attempt,
            },
        );
        self.paint(index, Tone::Calm, status.clone(), lookup)?;

        let entry = match self.read(index, status.as_deref(), lookup)? {
            // The cold tries are spent: what the field will still take is the
            // lookup or the way out, and a typed answer is refused rather than
            // judged, since a fourth cold try would be a reading nobody asked
            // for.
            Typed::Submitted(entry) if prompt.exhausted && !aided => {
                let _ = entry.forget();
                self.flash(index, Some(OUT_OF_TRIES.to_owned()), lookup)?;
                prompt.attempt = prompt.attempt.saturating_sub(1);
                return Ok(Next::Again);
            }
            Typed::Submitted(entry) => entry,
            Typed::Lookup => {
                prompt.class = Class::Aided;
                if let Some(row) = self.rows.get_mut(index) {
                    row.class = Class::Aided;
                }
                prompt.attempt = prompt.attempt.saturating_sub(1);
                return Ok(Next::Again);
            }
            Typed::Skipped => {
                self.left_it(index, *prompt, item, Outcome::Skip)?;
                return Ok(Next::Done);
            }
            Typed::Aborted => {
                self.left_it(index, *prompt, item, Outcome::Abort)?;
                self.sitting.aborted = true;
                return Ok(Next::Stop);
            }
        };

        let outcome = self.judge(index, item, &entry.secret, status, lookup)?;
        let timings = entry.forget();
        // An aided entry cannot move the ladder, so it carries whatever this
        // sitting already decided rather than a position of its own.
        let step_after = if prompt.class.unaided() {
            *prompt
                .resolved
                .get_or_insert_with(|| item.step_after(outcome))
        } else {
            prompt.resolved.unwrap_or(item.step)
        };
        self.record(Taken {
            item: index,
            class: prompt.class,
            attempt: prompt.attempt,
            outcome,
            ttfk_ms: timings.ttfk_ms,
            total_ms: timings.total_ms,
            corrections: timings.corrections,
            paste_refused: timings.paste_refused,
            step_after,
        })?;

        if outcome == Outcome::Pass {
            self.set(
                index,
                screen::RowState::Passed {
                    total_ms: timings.total_ms,
                    retries: prompt.attempt.saturating_sub(1),
                },
            );
            return Ok(Next::Done);
        }
        self.set(
            index,
            screen::RowState::Failed {
                attempt: prompt.attempt,
            },
        );
        if aided {
            // The answer was in front of the person and the verifier refused it
            // anyway. Nothing further to ask, and something else to look at.
            return Ok(Next::Done);
        }
        // The refusal is a flash and a counter, not a menu. Enter submits and
        // escape leaves; the one branch nobody could guess at is the lookup, and
        // it goes on offer here because a cold attempt is now on record.
        prompt.missed = true;
        prompt.exhausted = prompt.attempt >= self.max_attempts;
        let next = Prompt {
            attempt: prompt.attempt.saturating_add(1),
            ..*prompt
        }
        .status(self.max_attempts);
        self.flash(index, next, true)?;
        Ok(Next::Again)
    }

    /// What the verifier makes of one entry.
    ///
    /// An empty entry is a concession, and the verifier has nothing to say
    /// about it. Conceding is what the card asks for in place of typing
    /// something to get past the prompt.
    fn judge(
        &mut self,
        index: usize,
        item: &Item,
        secret: &Secret,
        status: Option<String>,
        lookup: bool,
    ) -> Result<Outcome> {
        if secret.is_empty() {
            return Ok(Outcome::Blank);
        }
        self.set(index, screen::RowState::Checking);
        self.paint(index, Tone::Calm, status, lookup)?;
        Ok(if (self.verify)(item, secret)? {
            Outcome::Pass
        } else {
            Outcome::Fail
        })
    }

    /// Record a prompt somebody walked away from, and mark its row.
    ///
    /// The step written is where the sitting has already put the slug, which
    /// is not always where it started: leaving after a miss must not write a
    /// record claiming the step the miss knocked it off.
    fn left_it(
        &mut self,
        index: usize,
        prompt: Prompt,
        item: &Item,
        outcome: Outcome,
    ) -> Result<()> {
        let state = if outcome == Outcome::Skip {
            screen::RowState::Skipped
        } else {
            screen::RowState::Aborted
        };
        self.set(index, state);
        self.record(Taken {
            item: index,
            class: prompt.class,
            attempt: prompt.attempt,
            outcome,
            ttfk_ms: None,
            total_ms: None,
            corrections: 0,
            paste_refused: 0,
            step_after: prompt.resolved.unwrap_or(item.step),
        })
    }

    /// Keep one entry, and write it out before anything else happens.
    fn record(&mut self, taken: Taken) -> Result<()> {
        self.sitting.taken.push(taken);
        (self.record)(&taken)
    }

    fn set(&mut self, index: usize, state: screen::RowState) {
        if let Some(row) = self.rows.get_mut(index) {
            row.state = state;
        }
    }

    fn frame(&self, index: usize, tone: Tone, status: Option<String>, lookup: bool) -> Card {
        let mut frame = screen::Frame::running(self.today, &self.rows, Some(index));
        frame.tone = tone;
        frame.status = status;
        frame.lookup = lookup;
        screen::card(&frame, self.console.style())
    }

    fn paint(
        &mut self,
        index: usize,
        tone: Tone,
        status: Option<String>,
        lookup: bool,
    ) -> Result<()> {
        let card = self.frame(index, tone, status, lookup);
        self.console.paint(&card)
    }

    fn flash(&mut self, index: usize, status: Option<String>, lookup: bool) -> Result<()> {
        let card = self.frame(index, Tone::Alarm, status, lookup);
        self.console.flash(&card)
    }

    fn read(&mut self, index: usize, status: Option<&str>, lookup: bool) -> Result<Typed> {
        let style = self.console.style();
        let rows = &self.rows;
        let today = self.today;
        read_secret(self.console, lookup, &mut |refusal| {
            let mut frame = screen::Frame::running(today, rows, Some(index));
            frame.tone = refusal.tone();
            frame.lookup = lookup;
            frame.status = refusal.status().or(status).map(str::to_owned);
            screen::card(&frame, style)
        })
    }
}

/// Where one slug's prompt has got to.
#[derive(Clone, Copy)]
struct Prompt {
    /// What the next entry counts as.
    class: Class,
    /// Which try the next entry is.
    attempt: u8,
    /// Whether a cold miss is on record, which is what puts the lookup on offer.
    missed: bool,
    /// Whether the cold tries are spent.
    exhausted: bool,
    /// The step the first cold try decided, once it has.
    resolved: Option<Step>,
}

impl Prompt {
    fn new(class: Class) -> Self {
        Self {
            class,
            attempt: 0,
            missed: false,
            exhausted: false,
            resolved: None,
        }
    }

    /// What the line under the field says about this try.
    fn status(self, of: u8) -> Option<String> {
        if !self.class.unaided() {
            return None;
        }
        if self.exhausted {
            return Some(OUT_OF_TRIES.to_owned());
        }
        (self.attempt > 1).then(|| format!("try {} of {of}", self.attempt))
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use jiff::civil::{Date, date};
    use relic_core::style::Style;

    use super::{Item, Mode, Plan, Sitting, Taken, screen};
    use crate::config::Filler;
    use crate::ladder::{Class, Step};
    use crate::log::Outcome;
    use crate::secret::Secret;
    use crate::tui::{Card, Input, Key, Screen};
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
        flashes: usize,
        held: usize,
        anchored: usize,
        last: Vec<String>,
        flashed: Vec<String>,
    }

    impl Fake {
        fn new(keys: Vec<(Key, u64)>) -> Self {
            Self {
                keys,
                at: 0,
                elapsed: 0,
                frames: 0,
                flashes: 0,
                held: 0,
                anchored: 0,
                last: Vec::new(),
                flashed: Vec::new(),
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
        fn style(&self) -> Style {
            Style::PLAIN
        }

        fn anchor(&mut self, height: usize) {
            self.anchored = height;
        }

        fn paint(&mut self, card: &Card) -> Result<()> {
            self.frames = self.frames.saturating_add(1);
            self.last = card.render();
            Ok(())
        }

        /// No pause in a test: what matters is that a refusal was shown at all.
        fn flash(&mut self, card: &Card) -> Result<()> {
            self.flashes = self.flashes.saturating_add(1);
            self.flashed = card.render();
            self.paint(card)
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

    /// Run a sitting and paint its closing frame, as `cmd` does. The entries
    /// handed to the record callback come back beside the sitting, so a test
    /// can assert they are one and the same.
    fn drive(
        plan: &Plan,
        keys: Vec<(Key, u64)>,
        max_attempts: u8,
        accept: &dyn Fn(&Secret) -> bool,
    ) -> (Sitting, Fake, Vec<Taken>) {
        let mut console = Fake::new(keys);
        let mut recorded = Vec::new();
        let sitting = super::run(
            plan,
            max_attempts,
            &mut console,
            &|_item, secret| Ok(accept(secret)),
            &mut |taken| {
                recorded.push(*taken);
                Ok(())
            },
            date(2026, 9, 10),
        )
        .unwrap();
        super::screen::card(
            &screen::Frame::done(date(2026, 9, 10), &sitting.rows, &sitting, &[]),
            Style::PLAIN,
        );
        console.hold().unwrap();
        (sitting, console, recorded)
    }

    /// Answers to the offer that follows a miss.
    /// A miss puts the field straight back; there is nothing to press.
    const LOOK: (Key, u64) = (Key::Lookup, 0);
    const MOVE_ON: (Key, u64) = (Key::Escape, 0);

    fn one(class: Class) -> Plan {
        Plan {
            items: vec![item("a", class, Step::cap())],
        }
    }

    #[test]
    fn a_clean_entry_records_its_timings() {
        let plan = one(Class::Review);
        let (sitting, console, _) = drive(&plan, typing("abc", 1_200), 3, &|_| true);
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
    fn every_entry_is_written_the_moment_it_is_taken_and_in_order() {
        let plan = Plan {
            items: vec![
                item("a", Class::Review, Step::cap()),
                item("b", Class::Review, Step::cap()),
            ],
        };
        let mut keys = typing("wrong", 100);
        keys.extend(typing("right", 2_000));
        keys.push(MOVE_ON);
        let attempts = std::cell::Cell::new(0u32);
        let (sitting, _, recorded) = drive(&plan, keys, 3, &|_| {
            attempts.set(attempts.get().saturating_add(1));
            attempts.get() > 1
        });
        assert_eq!(
            recorded, sitting.taken,
            "what was recorded is what was kept"
        );
        assert_eq!(recorded.len(), 3, "a miss, a pass, and a skip");
    }

    #[test]
    fn a_record_that_cannot_be_written_stops_the_sitting() {
        let plan = one(Class::Review);
        let mut console = Fake::new(typing("x", 0));
        let result = super::run(
            &plan,
            3,
            &mut console,
            &|_, _| Ok(true),
            &mut |_| Err(anyhow::anyhow!("disk full")),
            date(2026, 9, 10),
        );
        assert!(result.is_err());
    }

    #[test]
    fn the_typed_secret_is_what_reaches_the_verifier() {
        let plan = one(Class::Review);
        let seen = std::cell::RefCell::new(Vec::new());
        drive(&plan, typing("hunter2", 0), 1, &|secret| {
            seen.borrow_mut().push(secret.expose().to_vec());
            true
        });
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
        let (sitting, _, _) = drive(&plan, keys, 1, &|secret| {
            seen.borrow_mut().push(secret.expose().to_vec());
            true
        });
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
        let (sitting, _, _) = drive(&plan, keys, 1, &|_| true);
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
        let (sitting, console, _) = drive(&plan, keys, 1, &|secret| {
            seen.borrow_mut().push(secret.expose().to_vec());
            true
        });
        assert_eq!(seen.borrow().first().unwrap(), b"ab");
        assert_eq!(sitting.taken.first().unwrap().paste_refused, 1);
        assert!(
            console.flashed.join("\n").contains("do not paste"),
            "{:?}",
            console.flashed
        );
    }

    #[test]
    fn a_lapse_resets_the_step_and_the_retry_does_not_undo_it() {
        let plan = one(Class::Review);
        let mut keys = typing("wrong", 500);
        keys.extend(typing("right", 2_000));
        let attempts = std::cell::Cell::new(0u32);
        let (sitting, _, _) = drive(&plan, keys, 3, &|_| {
            attempts.set(attempts.get().saturating_add(1));
            attempts.get() > 1
        });
        assert_eq!(sitting.taken.len(), 2);
        assert_eq!(sitting.missed(), 1);
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
    fn the_status_line_survives_the_check() {
        // The counter and the offer must not blink out while the verifier runs.
        let plan = one(Class::Review);
        let mut keys = typing("wrong", 100);
        keys.extend(typing("right", 2_000));
        let mut console = Fake::new(keys);
        let attempts = std::cell::Cell::new(0u32);
        super::run(
            &plan,
            3,
            &mut console,
            &|_, _| {
                attempts.set(attempts.get().saturating_add(1));
                Ok(attempts.get() > 1)
            },
            &mut |_| Ok(()),
            date(2026, 9, 10),
        )
        .unwrap();
        // Nothing is painted after a pass, so the last frame is the checking
        // frame of try 2.
        let text = console.last.join("\n");
        assert!(text.contains('·'), "the checking glyph: {text}");
        assert!(text.contains("try 2 of 3"), "{text}");
        assert!(text.contains("^L"), "{text}");
    }

    #[test]
    fn out_of_tries_keeps_the_lookup_on_offer_and_refuses_a_fourth_cold_try() {
        let plan = one(Class::Review);
        let mut keys = vec![];
        for _ in 0..3 {
            keys.extend(typing("nope", 100));
        }
        // A fourth typed answer is refused, not judged.
        keys.extend(typing("still-nope", 100));
        keys.push(LOOK);
        keys.extend(typing("from-the-vault", 100));
        let verified = std::cell::Cell::new(0u32);
        let (sitting, console, _) = drive(&plan, keys, 3, &|secret| {
            verified.set(verified.get().saturating_add(1));
            secret.expose() == b"from-the-vault"
        });
        assert_eq!(
            verified.get(),
            4,
            "three cold tries and the aided one; the refused fourth never reached the verifier"
        );
        assert_eq!(sitting.taken.len(), 4);
        let aided = sitting.taken.last().unwrap();
        assert_eq!(aided.class, Class::Aided);
        assert_eq!(aided.outcome, Outcome::Pass);
        assert_eq!(aided.attempt, 4);
        assert_eq!(sitting.landings(), vec![super::Landed::Aided]);
        assert!(
            console.flashed.join("\n").contains("out of tries"),
            "{:?}",
            console.flashed
        );
    }

    #[test]
    fn out_of_tries_can_be_left_with_escape() {
        let plan = one(Class::Review);
        let mut keys = vec![];
        for _ in 0..3 {
            keys.extend(typing("nope", 100));
        }
        keys.push(MOVE_ON);
        keys.extend(typing("never-reached", 100));
        let (sitting, _, _) = drive(&plan, keys, 3, &|_| false);
        assert_eq!(sitting.taken.len(), 4, "three misses and the leaving");
        assert_eq!(sitting.taken.last().unwrap().outcome, Outcome::Skip);
        assert!(!sitting.aborted);
        assert_eq!(sitting.landings(), vec![super::Landed::Lapse]);
    }

    #[test]
    fn an_empty_entry_is_a_blank_and_the_verifier_is_never_asked() {
        let plan = one(Class::Review);
        let asked = std::cell::Cell::new(0u32);
        let (sitting, _, _) = drive(&plan, vec![(Key::Enter, 100), MOVE_ON], 2, &|_| {
            asked.set(asked.get().saturating_add(1));
            true
        });
        let taken = sitting.taken.first().unwrap();
        assert_eq!(taken.outcome, Outcome::Blank);
        assert_eq!(taken.step_after, Step::FIRST, "conceding is a lapse");
        assert_eq!(asked.get(), 0, "there was nothing to check");
        assert_eq!(sitting.missed(), 1);
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
        let (sitting, _, _) = drive(&plan, keys, 3, &|_| {
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
        assert_eq!(sitting.missed(), 1, "an aided pass is not a rescue");
    }

    #[test]
    fn the_lookup_is_not_offered_on_the_aided_prompt_itself() {
        let plan = one(Class::Review);
        let mut keys = typing("wrong", 100);
        keys.push(LOOK);
        // Pressing it again on the aided prompt is nothing; the entry follows.
        keys.push(LOOK);
        keys.extend(typing("right", 2_000));
        let (sitting, _, _) = drive(&plan, keys, 3, &|secret| secret.expose() == b"right");
        assert_eq!(sitting.taken.len(), 2);
        assert_eq!(sitting.taken.get(1).unwrap().class, Class::Aided);
        assert_eq!(sitting.taken.get(1).unwrap().outcome, Outcome::Pass);
    }

    #[test]
    fn an_aided_entry_that_is_refused_says_so_and_asks_nothing_further() {
        let plan = one(Class::Review);
        let mut keys = typing("wrong", 100);
        keys.push(LOOK);
        keys.extend(typing("also-wrong", 2_000));
        keys.extend(typing("never-reached", 4_000));
        let (sitting, _, _) = drive(&plan, keys, 3, &|_| false);
        assert_eq!(sitting.taken.len(), 2);
        let aided = sitting.taken.get(1).unwrap();
        assert_eq!(aided.class, Class::Aided);
        assert_eq!(aided.outcome, Outcome::Fail);
        assert_eq!(
            sitting.missed(),
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
        let (sitting, _, _) = drive(&plan, keys, 3, &|_| false);
        assert_eq!(sitting.taken.len(), 2, "the miss, and the leaving");
        assert!(!sitting.aborted);
        let left = sitting.taken.get(1).unwrap();
        assert_eq!(left.outcome, Outcome::Skip);
        assert_eq!(
            left.step_after,
            Step::FIRST,
            "leaving must not put back the step the miss knocked it off"
        );
    }

    #[test]
    fn a_slug_that_was_looked_up_lands_as_aided_whatever_the_cold_try_did() {
        // Conceded, looked it up, typed it: the row says aided, and so does the
        // sitting. The cold blank is still a first-attempt failure in the log,
        // in the ladder and in the exit status.
        let plan = one(Class::Review);
        let mut keys = vec![(Key::Enter, 100), LOOK];
        keys.extend(typing("right", 2_000));
        let (sitting, _, _) = drive(&plan, keys, 3, &|_| true);
        assert_eq!(sitting.landings(), vec![super::Landed::Aided]);
        assert_eq!(
            sitting.missed(),
            1,
            "the cold blank is still what the exit status reports"
        );
        let cold = sitting.taken.first().unwrap();
        assert_eq!(cold.outcome, Outcome::Blank);
        assert_eq!(
            cold.step_after,
            Step::FIRST,
            "and the ladder still went back to the foot"
        );
    }

    #[test]
    fn a_miss_with_no_lookup_still_lands_as_a_lapse() {
        let plan = one(Class::Review);
        let mut keys = typing("wrong", 100);
        keys.extend(typing("wrong", 2_000));
        keys.extend(typing("wrong", 4_000));
        keys.push(MOVE_ON);
        let (sitting, _, _) = drive(&plan, keys, 3, &|_| false);
        assert_eq!(sitting.landings(), vec![super::Landed::Lapse]);
    }

    #[test]
    fn practice_never_moves_the_step_in_either_direction() {
        for outcome in [true, false] {
            let plan = one(Class::Practice);
            let mut keys = typing("x", 0);
            keys.push(MOVE_ON);
            let (sitting, _, _) = drive(&plan, keys, 1, &|_| outcome);
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
        let (sitting, _, _) = drive(&plan, keys, 3, &|_| true);
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
        let (sitting, console, _) = drive(&plan, vec![(Key::Interrupt, 100)], 3, &|_| true);
        assert!(sitting.aborted);
        assert_eq!(sitting.taken.len(), 1);
        assert_eq!(sitting.taken.first().unwrap().outcome, Outcome::Abort);
        assert_eq!(console.held, 1);
    }

    #[test]
    fn input_running_out_reads_as_an_abandoned_sitting() {
        let plan = one(Class::Review);
        let (sitting, _, _) = drive(&plan, vec![(Key::Char('a'), 10)], 3, &|_| true);
        assert!(sitting.aborted);
    }

    #[test]
    fn a_slug_with_no_verifier_is_shown_and_never_prompted() {
        let mut plan = one(Class::Review);
        if let Some(first) = plan.items.first_mut() {
            first.verifier = None;
        }
        let (sitting, console, _) = drive(&plan, typing("x", 0), 3, &|_| true);
        assert!(sitting.taken.is_empty());
        assert_eq!(plan.missing().count(), 1);
        let _ = console;
    }

    #[test]
    fn the_dialog_is_anchored_at_the_height_of_its_own_card() {
        let plan = Plan {
            items: vec![
                item("a", Class::Review, Step::cap()),
                item("b", Class::Review, Step::cap()),
            ],
        };
        let (sitting, console, _) = drive(&plan, vec![(Key::Interrupt, 0)], 3, &|_| true);
        let closing = screen::card(
            &screen::Frame::done(date(2026, 9, 10), &sitting.rows, &sitting, &[]),
            Style::PLAIN,
        );
        assert_eq!(console.anchored, closing.height());
    }

    #[test]
    fn no_frame_ever_carries_what_was_typed() {
        let plan = one(Class::Review);
        let (_, console, _) = drive(&plan, typing("hunter2", 0), 1, &|_| true);
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
            "enrollment today is today's exposure"
        );
        assert_eq!(
            classes(&on(date(2026, 9, 1), &state, &verifiers, Mode::Practice)),
            vec![("a", Class::Practice)],
            "asking outright still works"
        );
    }

    #[test]
    fn a_held_slug_stays_out_of_the_daily_sitting_until_its_horizon() {
        let (state, verifiers) = seeded(&[("a", false)]);
        let held = State::replay(&[
            added(date(2026, 9, 1), "a"),
            Line::Parsed(Box::new(Record {
                v: SCHEMA,
                at: "2026-09-02T00:00:00Z".parse().unwrap(),
                day: date(2026, 9, 2),
                host: "Mac".to_owned(),
                prev: Digest::GENESIS,
                event: Event::Probe(crate::log::Probed {
                    slug: "a".parse().unwrap(),
                    until: Some(date(2026, 10, 17)),
                }),
            })),
        ]);
        let _ = state;
        assert!(
            on(
                date(2026, 9, 20),
                &held,
                &verifiers,
                Mode::Daily(Filler::All)
            )
            .is_empty(),
            "held out even from the fullest filler"
        );
        assert_eq!(
            classes(&on(
                date(2026, 10, 17),
                &held,
                &verifiers,
                Mode::Daily(Filler::All)
            )),
            vec![("a", Class::Probe)]
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
