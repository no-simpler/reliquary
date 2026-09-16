//! The sitting: what to ask for, and the loop that asks.
//!
//! A sitting works through its **turns**. One turn is one engram, and it is
//! either a **drill** — the secret is asked for and the verifier judges it — or
//! **dormant**, where this machine has no verifier and there is nothing to ask.
//! A dormant turn is a row that is skipped over and named on the way out; the
//! sitting does not attach, because an attachment is a claim and a drill is a
//! measurement, and one prompt should not be both.
//!
//! The loop is generic over where keys come from, where the card is painted
//! and where each capture is written, so every path through it is driven by a
//! scripted list of keys in the tests. Only raw mode, the alternate screen and
//! the real `event::read()` sit outside that, in `tui`.

pub mod screen;

use anyhow::Result;
use jiff::civil::Date;
use relic_core::style::Style;

use crate::corpus::drill::Landing;
use crate::corpus::record::{EngramId, Outcome};
use crate::intake::NOT_IT;
use crate::ladder::{Ladder, Occasion, Rung, Standing};
use crate::secret::Secret;
use crate::slug::Slug;
use crate::tui::{
    Card, Console, Typed,
    card::{Field, Tone},
    read_secret,
};
use crate::verifier::Verifier;

/// One engram's slot in a sitting.
#[derive(Clone, Debug)]
pub struct Turn {
    /// The lineage.
    pub slug: Slug,
    /// The engram being asked about: always the current one.
    pub engram: EngramId,
    /// What this turn is for.
    pub task: Task,
}

/// The two things a turn can be.
#[derive(Clone, Debug)]
pub enum Task {
    /// Ask for the secret and let the verifier judge it.
    Drill(Box<Drilling>),
    /// No verifier here, so nothing to ask. The row is skipped over.
    Dormant,
}

/// A drill, as the roster planned it.
#[derive(Clone, Debug)]
pub struct Drilling {
    /// Whether the schedule asked.
    pub occasion: Occasion,
    /// Whether the whole sitting was declared aided up front.
    pub aided: bool,
    /// Where it sits on the ladder.
    pub rung: Rung,
    /// What the ladder asked for.
    pub scheduled_interval_days: u32,
    /// Days since the schedule's anchor.
    pub actual_interval_days: u32,
    /// Days since the secret was last in front of a person.
    pub effective_interval_days: u32,
    /// Whether the rung is the top of the ladder.
    pub at_cap: bool,
    /// The verifier. Not an option: **a missing verifier is not a drill**, it is
    /// a dormant turn, and the type says so rather than a branch inside the loop.
    pub verifier: Verifier,
}

impl Turn {
    /// Whether this machine holds nothing to drill this turn against.
    pub fn dormant(&self) -> bool {
        matches!(self.task, Task::Dormant)
    }
}

/// What kind of sitting this is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// What the schedule is asking for today, and nothing else.
    Due,
    /// Voluntary. Nothing here is a review, whatever the schedule says.
    Practice,
}

/// Decide what to ask for.
///
/// `only` narrows a practice sitting to named lineages; empty means all of them.
/// **A dormant turn appears wherever its lineage appears**, in either mode: an
/// engram with no verifier here cannot be drilled, and a row that says so is
/// how the sitting names the verb that puts one back.
pub fn roster(
    corpus: &crate::corpus::Corpus,
    verifiers: &crate::verifier::file::Verifiers,
    today: Date,
    ladder: &Ladder,
    mode: Mode,
    aided: bool,
    only: &[Slug],
) -> Vec<Turn> {
    let mut turns = Vec::new();
    for lineage in corpus.active() {
        if !only.is_empty() && !only.contains(&lineage.slug) {
            continue;
        }
        let Some(dossier) = lineage.current() else {
            continue;
        };
        let held = verifiers.get(&dossier.engram).ok().flatten();
        let Some(verifier) = held else {
            turns.push(Turn {
                slug: lineage.slug.clone(),
                engram: dossier.engram,
                task: Task::Dormant,
            });
            continue;
        };
        let occasion = match mode {
            Mode::Practice => Occasion::Practice,
            Mode::Due => match dossier.standing(today, ladder) {
                Standing::Due => Occasion::Review,
                Standing::Waiting { .. } => continue,
            },
        };
        turns.push(Turn {
            slug: lineage.slug.clone(),
            engram: dossier.engram,
            task: Task::Drill(Box::new(Drilling {
                occasion,
                aided,
                rung: dossier.rung,
                scheduled_interval_days: ladder.interval(dossier.rung),
                actual_interval_days: dossier.actual_interval(today),
                effective_interval_days: dossier.effective_interval(today),
                at_cap: ladder.at_cap(dossier.rung),
                verifier,
            })),
        });
    }
    turns
}

/// One sample, as the sitting saw it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Capture {
    /// Which turn in the roster.
    pub turn: usize,
    /// Whether the schedule asked.
    pub occasion: Occasion,
    /// Whether the answer was consulted before this one was typed.
    pub aided: bool,
    /// Which sample within this drill. Only the first is measured.
    pub ordinal: u8,
    /// How it ended.
    pub outcome: Outcome,
    /// Milliseconds from the prompt appearing to the first keystroke.
    pub ttfk_ms: Option<u64>,
    /// Milliseconds from the first keystroke to submission.
    pub total_ms: Option<u64>,
    /// Keystrokes that removed something, one apiece.
    pub corrections: u32,
    /// Pastes that reached the field.
    pub paste_accepted: u32,
    /// Pastes that did not.
    pub paste_refused: u32,
    /// Where the engram lands after this sample.
    pub rung_after: Rung,
}

/// How a whole sitting ended.
#[derive(Clone, Debug, Default)]
pub struct Outturn {
    /// Every sample, in the order they were typed.
    pub captures: Vec<Capture>,
    /// The turns skipped over because this machine holds no verifier for them.
    pub dormant: Vec<usize>,
    /// Whether the sitting was abandoned rather than finished.
    pub aborted: bool,
    /// The card as it stood at the end.
    pub rows: Vec<screen::Row>,
}

impl Outturn {
    /// How each turn landed, in roster order.
    ///
    /// One landing per turn and no turn in two of them, so the counts add up to
    /// what was actually in front of a person.
    pub fn landings(&self, turns: usize) -> Vec<Landing> {
        (0..turns)
            .filter_map(|index| {
                if self.dormant.contains(&index) {
                    return Some(Landing::Dormant);
                }
                let first = self
                    .captures
                    .iter()
                    .find(|capture| capture.turn == index && capture.ordinal == 1)?;
                Landing::from_parts(
                    first.aided,
                    first.outcome,
                    first.occasion.serves_the_schedule(),
                )
            })
            .collect()
    }

    /// First-sample failures on drills that measured something.
    pub fn missed(&self) -> usize {
        self.captures
            .iter()
            .filter(|capture| capture.ordinal == 1 && !capture.aided && capture.outcome.lapsed())
            .count()
    }
}

/// What the line under the field says once the cold tries are spent.
const OUT_OF_TRIES: &str = "out of tries";

/// What it says when a drill try was conceded: nothing was offered, so *not
/// it* would be false.
const CONCEDED: &str = "conceded";

/// What the line under the field says when a bounded retry has spent a round.
fn tried(reason: &str, attempt: u8, of: u8) -> String {
    format!("{reason} · try {} of {of}", attempt.clamp(1, of.max(1)))
}

/// Where each capture goes the moment it is taken.
///
/// Called before the next prompt is drawn, so a sitting cut short by a closed
/// window or a signal keeps every reading it had already produced.
pub type Record<'a> = dyn FnMut(&Capture) -> Result<()> + 'a;

/// How a secret is judged. Passed in rather than reached for, so the loop can be
/// driven in a test without paying 256 MiB and half a second per key. Told
/// which turn it is judging, because a secret proved against a verifier below
/// the cost floor is the one moment that verifier can be re-minted.
pub type Verify<'a> = dyn Fn(usize, &Verifier, &Secret) -> Result<bool> + 'a;

/// Where a sitting reads from and writes to.
///
/// Gathered rather than listed, so the loop takes its subject and its wiring and
/// nothing else.
pub struct Wiring<'a> {
    /// Where keys come from and the card is painted.
    pub console: &'a mut dyn Console,
    /// How a secret is judged.
    pub verify: &'a Verify<'a>,
    /// Where each capture goes.
    pub record: &'a mut Record<'a>,
}

/// Run the sitting.
///
/// # Errors
///
/// When the terminal cannot be read or written, a capture cannot be recorded,
/// or the verifier fails outright — which is different from refusing a secret,
/// and is not recorded as a lapse.
pub fn run(
    turns: &[Turn],
    max_attempts: u8,
    ladder: &Ladder,
    today: Date,
    wiring: Wiring<'_>,
) -> Result<Outturn> {
    let Wiring {
        console,
        verify,
        record,
    } = wiring;
    Session {
        turns,
        max_attempts,
        ladder,
        console,
        verify,
        record,
        today,
        rows: turns.iter().map(screen::Row::pending).collect(),
        outturn: Outturn::default(),
    }
    .run()
}

struct Session<'a> {
    turns: &'a [Turn],
    max_attempts: u8,
    ladder: &'a Ladder,
    console: &'a mut dyn Console,
    verify: &'a Verify<'a>,
    record: &'a mut Record<'a>,
    today: Date,
    rows: Vec<screen::Row>,
    outturn: Outturn,
}

/// What the loop decided about one prompt, beyond what it produced.
enum Next {
    /// Ask this engram again.
    Again,
    /// Move to the next turn.
    Done,
    /// Ask nothing further of anyone.
    Stop,
}

impl Session<'_> {
    fn run(mut self) -> Result<Outturn> {
        // The rectangle is the same for every state of the sitting, so the
        // opening card's height is the sitting's height. Read off a rendered
        // card rather than kept as a second constant that could drift from it.
        let opening = screen::card(
            &screen::Frame::running(self.today, &self.rows, Some(0)),
            &Field::blind(),
            Style::PLAIN,
        );
        self.console.anchor(opening.height());

        for index in 0..self.turns.len() {
            let Some(turn) = self.turns.get(index) else {
                break;
            };
            let stopped = match &turn.task {
                Task::Drill(drilling) => self.drill(index, drilling)?,
                Task::Dormant => {
                    // Nothing to ask and nothing to write: the row says so,
                    // and the closing card names the verb that puts a
                    // verifier back.
                    self.set(index, screen::RowState::Dormant);
                    self.outturn.dormant.push(index);
                    false
                }
            };
            if stopped {
                break;
            }
        }
        self.outturn.rows = self.rows;
        Ok(self.outturn)
    }

    /// Ask about one engram until it is answered, passed over, or walked away
    /// from. `true` means the sitting was abandoned.
    fn drill(&mut self, index: usize, drilling: &Drilling) -> Result<bool> {
        let mut prompt = Prompt::new(drilling.aided);
        loop {
            match self.one_sample(index, drilling, &mut prompt)? {
                Next::Again => {}
                Next::Done => return Ok(false),
                Next::Stop => return Ok(true),
            }
        }
    }

    /// One prompt: draw, read, judge, record.
    fn one_sample(
        &mut self,
        index: usize,
        drilling: &Drilling,
        prompt: &mut Prompt,
    ) -> Result<Next> {
        prompt.ordinal = prompt.ordinal.saturating_add(1);
        let lookup = prompt.missed && !prompt.aided;
        // A cold try is the one prompt in the tool that measures a memory, and
        // everything follows from that: it refuses a paste, stays blind, offers
        // no reveal and moves no caret. Once the entry is aided there is
        // nothing left here to protect.
        let cold = !prompt.aided;
        let status = prompt.status(self.max_attempts);
        self.set(
            index,
            screen::RowState::Active {
                attempt: prompt.ordinal,
            },
        );
        if let Some(row) = self.rows.get_mut(index) {
            row.set_aided(prompt.aided);
        }
        let resting = screen::resting(self.rows.get(index));
        self.paint(
            index,
            resting,
            status.clone(),
            lookup,
            &Field::resting(cold),
        )?;

        let entry = match self.read(index, status.as_deref(), lookup, cold)? {
            // The cold tries are spent: what the field will still take is the
            // lookup or the way out, and a typed answer is refused rather than
            // judged, since a fourth cold try would be a reading nobody asked
            // for.
            Typed::Submitted(entry) if prompt.exhausted && !prompt.aided => {
                let _ = entry.forget();
                self.flash(
                    index,
                    Some(OUT_OF_TRIES.to_owned()),
                    lookup,
                    &Field::resting(cold),
                )?;
                prompt.ordinal = prompt.ordinal.saturating_sub(1);
                return Ok(Next::Again);
            }
            Typed::Submitted(entry) => entry,
            Typed::Lookup => {
                prompt.aided = true;
                if let Some(row) = self.rows.get_mut(index) {
                    row.set_aided(true);
                }
                prompt.ordinal = prompt.ordinal.saturating_sub(1);
                return Ok(Next::Again);
            }
            Typed::Skipped => {
                self.left_it(index, *prompt, drilling, Outcome::Skip)?;
                return Ok(Next::Done);
            }
            Typed::Aborted => {
                self.left_it(index, *prompt, drilling, Outcome::Abort)?;
                self.outturn.aborted = true;
                return Ok(Next::Stop);
            }
        };

        let outcome = self.judge(index, drilling, &entry.secret, status, lookup)?;
        let timings = entry.forget();
        self.record(index, drilling, prompt, outcome, timings)?;

        if outcome == Outcome::Pass {
            self.set(
                index,
                screen::RowState::Passed {
                    // A latency is a claim about recall, so an aided entry
                    // publishes none — the row would otherwise carry a figure
                    // that a lookup, a reveal and a conceal all inflate, beside
                    // rows whose figure means something else entirely. `stats`
                    // draws the same line at `!drill.aided`.
                    total_ms: if prompt.aided { None } else { timings.total_ms },
                    retries: prompt.ordinal.saturating_sub(1),
                },
            );
            return Ok(Next::Done);
        }
        self.set(
            index,
            screen::RowState::Failed {
                attempt: prompt.ordinal,
            },
        );
        if prompt.aided {
            // The answer was in front of the person and the verifier refused it
            // anyway. Nothing further to ask, and something else to look at.
            return Ok(Next::Done);
        }
        prompt.missed = true;
        prompt.reason = if outcome == Outcome::Blank {
            CONCEDED
        } else {
            NOT_IT
        };
        prompt.exhausted = prompt.ordinal >= self.max_attempts;
        let next = Prompt {
            ordinal: prompt.ordinal.saturating_add(1),
            ..*prompt
        }
        .status(self.max_attempts);
        self.flash(index, next, true, &Field::resting(!prompt.aided))?;
        Ok(Next::Again)
    }

    /// Write one sample the moment it is taken, before the next prompt is
    /// drawn, so a closed window keeps every reading it had already produced.
    fn record(
        &mut self,
        index: usize,
        drilling: &Drilling,
        prompt: &mut Prompt,
        outcome: Outcome,
        timings: crate::tui::Timings,
    ) -> Result<()> {
        let rung_after = if prompt.aided {
            prompt.resolved.unwrap_or(drilling.rung)
        } else {
            *prompt
                .resolved
                .get_or_insert_with(|| self.settle(drilling, outcome))
        };
        self.keep(Capture {
            turn: index,
            occasion: drilling.occasion,
            aided: prompt.aided,
            ordinal: prompt.ordinal,
            outcome,
            ttfk_ms: timings.ttfk_ms,
            total_ms: timings.total_ms,
            corrections: timings.corrections,
            paste_accepted: timings.paste_accepted,
            paste_refused: timings.paste_refused,
            rung_after,
        })
    }

    /// Where the engram lands after an unaided sample on the first try.
    fn settle(&self, drilling: &Drilling, outcome: Outcome) -> Rung {
        if !drilling.occasion.serves_the_schedule() {
            return drilling.rung;
        }
        match outcome {
            Outcome::Pass => self.ladder.advanced(drilling.rung),
            Outcome::Fail | Outcome::Blank => Rung::FIRST,
            Outcome::Skip | Outcome::Abort => drilling.rung,
        }
    }

    /// What the verifier makes of one sample.
    ///
    /// An empty entry is a concession, and the verifier has nothing to say about
    /// it. Conceding is what the card asks for in place of typing something to
    /// get past the prompt.
    fn judge(
        &mut self,
        index: usize,
        drilling: &Drilling,
        secret: &Secret,
        status: Option<String>,
        lookup: bool,
    ) -> Result<Outcome> {
        if secret.is_empty() {
            return Ok(Outcome::Blank);
        }
        self.set(index, screen::RowState::Checking);
        // Checking: the box and its caret are taken down and no key does
        // anything.
        let card = self.frame(index, Tone::Calm, status, lookup, &Field::blind());
        let verify = &self.verify;
        let accepted = crate::tui::while_working(self.console, &card, || {
            verify(index, &drilling.verifier, secret)
        })?;
        Ok(if accepted {
            Outcome::Pass
        } else {
            Outcome::Fail
        })
    }

    /// Record a prompt somebody walked away from, and mark its row.
    fn left_it(
        &mut self,
        index: usize,
        prompt: Prompt,
        drilling: &Drilling,
        outcome: Outcome,
    ) -> Result<()> {
        let state = if outcome == Outcome::Skip {
            screen::RowState::Skipped
        } else {
            screen::RowState::Aborted
        };
        self.set(index, state);
        self.keep(Capture {
            turn: index,
            occasion: drilling.occasion,
            aided: prompt.aided,
            ordinal: prompt.ordinal,
            outcome,
            ttfk_ms: None,
            total_ms: None,
            corrections: 0,
            paste_accepted: 0,
            paste_refused: 0,
            rung_after: prompt.resolved.unwrap_or(drilling.rung),
        })
    }

    /// Keep one capture, and write it out before anything else happens.
    fn keep(&mut self, capture: Capture) -> Result<()> {
        self.outturn.captures.push(capture);
        (self.record)(&capture)
    }

    fn set(&mut self, index: usize, state: screen::RowState) {
        if let Some(row) = self.rows.get_mut(index) {
            row.state = state;
        }
    }

    fn frame(
        &self,
        index: usize,
        tone: Tone,
        status: Option<String>,
        lookup: bool,
        field: &Field<'_>,
    ) -> Card {
        let mut frame = screen::Frame::running(self.today, &self.rows, Some(index));
        frame.tone = tone;
        frame.status = status;
        frame.lookup = lookup;
        screen::card(&frame, field, self.console.style())
    }

    fn paint(
        &mut self,
        index: usize,
        tone: Tone,
        status: Option<String>,
        lookup: bool,
        field: &Field<'_>,
    ) -> Result<()> {
        let card = self.frame(index, tone, status, lookup, field);
        self.console.paint(&card)
    }

    /// A refusal is a pulse and a counter, not a paragraph.
    ///
    /// The border comes back; what was said does not go with it. See
    /// `tui::refuse`, which holds the same two lifetimes for the entry loop.
    fn flash(
        &mut self,
        index: usize,
        status: Option<String>,
        lookup: bool,
        field: &Field<'_>,
    ) -> Result<()> {
        let alarm = self.frame(index, Tone::Alarm, status.clone(), lookup, field);
        self.console.flash(&alarm)?;
        let resting = screen::resting(self.rows.get(index));
        let settled = self.frame(index, resting, status, lookup, field);
        self.console.paint(&settled)
    }

    fn read(
        &mut self,
        index: usize,
        status: Option<&str>,
        lookup: bool,
        cold: bool,
    ) -> Result<Typed> {
        let today = self.today;
        let rows = self.rows.clone();
        let style = self.console.style();
        let status = status.map(str::to_owned);
        let resting = screen::resting(rows.get(index));
        let mut build = |refusal: crate::tui::Refusal, tone: Tone, field: &Field<'_>| {
            let mut frame = screen::Frame::running(today, &rows, Some(index));
            frame.tone = tone;
            // What a refusal said outlives the tone it said it in, and a
            // standing status shows through again once it is taken down.
            frame.status = refusal
                .status()
                .map(str::to_owned)
                .or_else(|| status.clone());
            frame.lookup = lookup;
            screen::card(&frame, field, style)
        };
        read_secret(self.console, lookup, cold, resting, &mut build)
    }
}

/// Where one drill has got to, beyond what is on the card.
#[derive(Clone, Copy, Debug)]
struct Prompt {
    ordinal: u8,
    aided: bool,
    missed: bool,
    exhausted: bool,
    /// Why the last cold try was refused: a wrong answer, or none at all.
    reason: &'static str,
    /// Where the first sample put the engram. Later samples in the same drill
    /// carry it rather than deciding again.
    resolved: Option<Rung>,
}

impl Prompt {
    fn new(aided: bool) -> Self {
        Self {
            ordinal: 0,
            aided,
            missed: false,
            exhausted: false,
            reason: NOT_IT,
            resolved: None,
        }
    }

    /// What the line under the field says about how this drill is going.
    ///
    /// A standing status, so it is drawn on every card the prompt rests at
    /// and not only in the pulse that first said it. An aided entry is not
    /// bounded and carries no count; once the cold tries are spent the field
    /// says so for as long as it stays.
    fn status(self, max_attempts: u8) -> Option<String> {
        if self.aided {
            return None;
        }
        if self.exhausted {
            return Some(OUT_OF_TRIES.to_owned());
        }
        self.missed
            .then(|| tried(self.reason, self.ordinal, max_attempts))
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use jiff::civil::date;
    use relic_core::style::Style;

    use super::{Capture, Drilling, Outturn, Task, Turn, screen};
    use crate::corpus::drill::Landing;
    use crate::corpus::record::{EngramId, Outcome};
    use crate::ladder::{Ladder, Occasion, Rung};
    use crate::secret::Secret;
    use crate::tui::{Card, Input, Key, Screen};
    use crate::verifier::Verifier;

    const PHC: &str = "$argon2id$v=19$m=64,t=1,p=1$CQkJCQkJCQkJCQkJCQkJCQ$\
                       PdWLZDvNGYPMYWPfbSZq7yZO0eFRHmiGnLTuNBAxUC0";

    const RIGHT: &str = "right";

    /// A scripted terminal: keys with the elapsed time each arrived at, and a
    /// record of everything painted.
    ///
    /// The keys are consumed rather than copied, because one of them carries a
    /// secret and [`Key`] is deliberately neither `Copy` nor `Clone`.
    struct Fake {
        keys: std::vec::IntoIter<(Key, u64)>,
        elapsed: u64,
        flashes: usize,
        drains: usize,
        last: Vec<String>,
    }

    impl Fake {
        fn new(keys: Vec<(Key, u64)>) -> Self {
            Self {
                keys: keys.into_iter(),
                elapsed: 0,
                flashes: 0,
                drains: 0,
                last: Vec::new(),
            }
        }
    }

    impl Input for Fake {
        fn arm(&mut self) {
            self.elapsed = 0;
        }

        fn wait(&mut self, _within: Option<std::time::Duration>) -> Result<crate::tui::Wake> {
            let Some((key, at)) = self.keys.next() else {
                return Ok(crate::tui::Wake::Ended);
            };
            self.elapsed = at;
            Ok(crate::tui::Wake::Key(key))
        }

        fn elapsed_ms(&self) -> u64 {
            self.elapsed
        }
    }

    impl Screen for Fake {
        fn style(&self) -> Style {
            Style::PLAIN
        }

        fn anchor(&mut self, _height: usize) {}

        fn paint(&mut self, card: &Card) -> Result<()> {
            self.last = card.render();
            Ok(())
        }

        fn flash(&mut self, card: &Card) -> Result<()> {
            self.flashes = self.flashes.saturating_add(1);
            self.paint(card)
        }

        fn drain(&mut self) -> Result<()> {
            self.drains = self.drains.saturating_add(1);
            Ok(())
        }

        fn hold(&mut self) -> Result<()> {
            Ok(())
        }
    }

    fn drill_turn(name: &str, occasion: Occasion, aided: bool) -> Turn {
        Turn {
            slug: name.parse().unwrap(),
            engram: EngramId::mint().unwrap(),
            task: Task::Drill(Box::new(Drilling {
                occasion,
                aided,
                rung: Rung::FIRST,
                scheduled_interval_days: 1,
                actual_interval_days: 1,
                effective_interval_days: 1,
                at_cap: false,
                verifier: Verifier::parse(PHC).unwrap(),
            })),
        }
    }

    fn dormant_turn(name: &str) -> Turn {
        Turn {
            slug: name.parse().unwrap(),
            engram: EngramId::mint().unwrap(),
            task: Task::Dormant,
        }
    }

    fn pasting(text: &str, at: u64) -> (Key, u64) {
        (Key::Paste(crate::secret::Pasted::new(text.to_owned())), at)
    }

    fn typing(text: &str, from: u64) -> Vec<(Key, u64)> {
        let mut keys = Vec::new();
        let mut at = from;
        for character in text.chars() {
            keys.push((Key::Char(character), at));
            at = at.saturating_add(100);
        }
        keys.push((Key::Enter, at));
        keys
    }

    struct Ran {
        outturn: Outturn,
        written: Vec<Capture>,
        flashes: usize,
        drains: usize,
        /// The card as it last stood, joined into one string.
        last: String,
    }

    fn run(turns: &[Turn], keys: Vec<(Key, u64)>) -> Ran {
        let mut console = Fake::new(keys);
        let ladder = Ladder::default();
        let verify =
            |_: usize, _: &Verifier, secret: &Secret| Ok(secret.expose() == RIGHT.as_bytes());
        let written = std::cell::RefCell::new(Vec::new());
        let outturn = {
            let mut record = |capture: &Capture| -> Result<()> {
                written.borrow_mut().push(*capture);
                Ok(())
            };
            super::run(
                turns,
                3,
                &ladder,
                date(2026, 9, 13),
                super::Wiring {
                    console: &mut console,
                    verify: &verify,
                    record: &mut record,
                },
            )
            .unwrap()
        };
        Ran {
            outturn,
            written: written.into_inner(),
            flashes: console.flashes,
            drains: console.drains,
            last: console.last.join("\n"),
        }
    }

    #[test]
    fn a_passed_drill_records_one_capture_and_advances_the_rung() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let ran = run(&turns, typing(RIGHT, 400));
        assert_eq!(ran.written.len(), 1);
        let capture = ran.written.first().unwrap();
        assert_eq!(capture.outcome, Outcome::Pass);
        assert_eq!(capture.ordinal, 1);
        assert_eq!(capture.rung_after.get(), 1);
        assert_eq!(capture.ttfk_ms, Some(400));
        assert_eq!(ran.outturn.landings(1), vec![Landing::Pass]);
        assert_eq!(ran.outturn.missed(), 0);
    }

    #[test]
    fn a_missed_drill_goes_back_to_the_foot_and_the_retry_does_not_score_again() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let mut keys = typing("wrong", 300);
        keys.extend(typing(RIGHT, 900));
        let ran = run(&turns, keys);

        assert_eq!(ran.written.len(), 2);
        let first = ran.written.first().unwrap();
        let second = ran.written.get(1).unwrap();
        assert_eq!(first.outcome, Outcome::Fail);
        assert_eq!(first.rung_after, Rung::FIRST);
        assert_eq!(second.outcome, Outcome::Pass);
        assert_eq!(
            second.rung_after,
            Rung::FIRST,
            "the second sample carries what the first decided"
        );
        assert_eq!(ran.outturn.landings(1), vec![Landing::Lapse]);
        assert_eq!(ran.outturn.missed(), 1);
        assert!(ran.flashes > 0, "a refusal is flashed");
    }

    #[test]
    fn practice_never_moves_the_rung_in_either_direction() {
        let turns = vec![drill_turn("a", Occasion::Practice, false)];
        let ran = run(&turns, typing("wrong", 300));
        let capture = ran.written.first().unwrap();
        assert_eq!(capture.rung_after, Rung::FIRST);
        assert_eq!(ran.outturn.landings(1), vec![Landing::Miss]);
        assert_eq!(ran.outturn.missed(), 1);
    }

    #[test]
    fn submitting_nothing_concedes_rather_than_being_judged() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let ran = run(&turns, vec![(Key::Enter, 800)]);
        assert_eq!(ran.written.first().map(|c| c.outcome), Some(Outcome::Blank));
        assert_eq!(ran.outturn.landings(1), vec![Landing::Lapse]);
    }

    #[test]
    fn an_aided_pass_publishes_no_latency_on_the_row() {
        // A latency is a claim about recall, and an aided entry measures
        // transcription. `stats` draws the same line at `!drill.aided`; the row
        // beside it must not say otherwise, because a reveal and an idle
        // conceal both sit inside the figure it would have shown.
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let cold = run(&turns, typing(RIGHT, 400));
        assert!(
            matches!(
                cold.outturn.rows.first().map(|row| row.state),
                Some(screen::RowState::Passed {
                    total_ms: Some(_),
                    ..
                })
            ),
            "a cold pass still reports what it cost"
        );

        let mut keys = typing("wrong", 300);
        keys.push((Key::Lookup, 900));
        keys.extend(typing(RIGHT, 1_000));
        let aided = run(&turns, keys);
        assert_eq!(
            aided.outturn.rows.first().map(|row| row.state),
            Some(screen::RowState::Passed {
                total_ms: None,
                retries: 1,
            })
        );
        assert!(
            aided.written.get(1).unwrap().total_ms.is_some(),
            "the record still keeps what the record kept"
        );
    }

    #[test]
    fn the_lookup_re_labels_the_drill_and_is_only_offered_after_a_cold_try() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let mut keys = typing("wrong", 300);
        keys.push((Key::Lookup, 900));
        keys.extend(typing(RIGHT, 1_000));
        let ran = run(&turns, keys);

        assert_eq!(ran.written.len(), 2);
        assert!(!ran.written.first().unwrap().aided, "the cold try is cold");
        assert!(ran.written.get(1).unwrap().aided);
        assert_eq!(
            ran.outturn.landings(1),
            vec![Landing::Lapse],
            "the cold miss it opened with is still what was measured"
        );
    }

    #[test]
    fn a_sitting_declared_aided_measures_nothing_from_the_first_key() {
        let turns = vec![drill_turn("a", Occasion::Review, true)];
        let ran = run(&turns, typing(RIGHT, 400));
        let capture = ran.written.first().unwrap();
        assert!(capture.aided);
        assert_eq!(capture.rung_after, Rung::FIRST);
        assert_eq!(ran.outturn.landings(1), vec![Landing::Aided]);
        assert_eq!(ran.outturn.missed(), 0);
    }

    #[test]
    fn escape_passes_over_a_drill_and_records_that_nothing_was_typed() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let ran = run(&turns, vec![(Key::Escape, 200)]);
        assert_eq!(ran.written.first().map(|c| c.outcome), Some(Outcome::Skip));
        assert_eq!(ran.outturn.landings(1), vec![Landing::Skipped]);
        assert!(!ran.outturn.aborted);
    }

    #[test]
    fn an_interrupt_abandons_the_sitting_and_asks_nobody_else() {
        let turns = vec![
            drill_turn("a", Occasion::Review, false),
            drill_turn("b", Occasion::Review, false),
        ];
        let ran = run(&turns, vec![(Key::Interrupt, 200)]);
        assert!(ran.outturn.aborted);
        assert_eq!(ran.written.len(), 1);
        assert_eq!(
            ran.outturn.landings(2),
            Vec::new(),
            "an abandoned prompt is no reading at all"
        );
    }

    #[test]
    fn the_cold_tries_run_out_and_a_further_answer_is_refused_rather_than_judged() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let mut keys = Vec::new();
        for _ in 0..4 {
            keys.extend(typing("wrong", 300));
        }
        keys.push((Key::Escape, 9_000));
        let ran = run(&turns, keys);
        assert_eq!(
            ran.written
                .iter()
                .filter(|c| c.outcome == Outcome::Fail)
                .count(),
            3,
            "three tries, and the fourth is not a reading anybody asked for"
        );
    }

    #[test]
    fn once_the_cold_tries_are_spent_the_field_says_so_and_keeps_the_lookup_on_offer() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let mut keys = Vec::new();
        for _ in 0..3 {
            keys.extend(typing("wrong", 300));
        }
        keys.push((Key::Escape, 9_000));
        let ran = run(&turns, keys);
        // The card the prompt rests at after the third miss, not a pulse.
        assert!(ran.last.contains(super::OUT_OF_TRIES), "{}", ran.last);
        assert!(ran.last.contains("^L"), "{}", ran.last);
        assert!(!ran.last.contains("try 4"), "{}", ran.last);
    }

    #[test]
    fn a_blank_is_a_concession_and_the_next_try_says_so() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let ran = run(&turns, vec![(Key::Enter, 800), (Key::Escape, 1_200)]);
        assert!(
            ran.last.contains("conceded · try 2 of 3"),
            "nothing was offered, so it was not *not it*: {}",
            ran.last
        );
    }

    #[test]
    fn an_aided_entry_carries_no_try_count() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let mut keys = typing("wrong", 300);
        keys.push((Key::Lookup, 900));
        keys.push((Key::Escape, 1_000));
        let ran = run(&turns, keys);
        assert!(!ran.last.contains("try 2"), "{}", ran.last);
        assert!(ran.last.contains("measures nothing"), "{}", ran.last);
    }

    #[test]
    fn the_verifier_is_told_which_turn_it_is_judging() {
        let turns = vec![
            drill_turn("a", Occasion::Review, false),
            drill_turn("b", Occasion::Review, false),
        ];
        let mut keys = typing(RIGHT, 300);
        keys.extend(typing(RIGHT, 900));
        let mut console = Fake::new(keys);
        let seen = std::cell::RefCell::new(Vec::new());
        let verify = |index: usize, _: &Verifier, _: &Secret| {
            seen.borrow_mut().push(index);
            Ok(true)
        };
        let mut record = |_: &Capture| -> Result<()> { Ok(()) };
        super::run(
            &turns,
            3,
            &Ladder::default(),
            date(2026, 9, 13),
            super::Wiring {
                console: &mut console,
                verify: &verify,
                record: &mut record,
            },
        )
        .unwrap();
        assert_eq!(seen.into_inner(), vec![0, 1]);
    }

    #[test]
    fn a_dormant_lineage_is_a_row_and_never_a_prompt() {
        // Two turns, one key script: the dormant row asks for nothing, so the
        // only keys consumed are the drill's. Were a prompt drawn for it, the
        // typed answer would land there and the drill would go unanswered.
        let turns = vec![dormant_turn("a"), drill_turn("b", Occasion::Review, false)];
        let ran = run(&turns, typing(RIGHT, 300));
        assert_eq!(
            ran.written.len(),
            1,
            "nothing is written for a dormant turn"
        );
        assert_eq!(ran.written.first().map(|c| c.turn), Some(1));
        assert_eq!(ran.outturn.dormant, vec![0]);
        assert_eq!(
            ran.outturn.landings(2),
            vec![Landing::Dormant, Landing::Pass]
        );
        assert_eq!(ran.outturn.missed(), 0);
        assert!(!ran.outturn.aborted);
        assert!(matches!(
            ran.outturn.rows.first().map(|row| row.state),
            Some(screen::RowState::Dormant)
        ));
        assert!(ran.last.contains("dormant"), "{}", ran.last);
    }

    #[test]
    fn a_cold_try_refuses_a_paste_and_counts_it() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let mut keys = vec![pasting(RIGHT, 200)];
        keys.extend(typing(RIGHT, 400));
        let ran = run(&turns, keys);
        let capture = ran.written.first().unwrap();
        assert_eq!(capture.paste_refused, 1);
        assert_eq!(capture.paste_accepted, 0);
        assert_eq!(
            capture.outcome,
            Outcome::Pass,
            "the pass is the one that was typed"
        );
        assert!(ran.flashes > 0, "and the refusal was said");
    }

    #[test]
    fn the_aided_entry_after_a_lookup_takes_a_paste() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let mut keys = typing("wrong", 300);
        keys.push((Key::Lookup, 900));
        keys.push(pasting(RIGHT, 1_000));
        keys.push((Key::Enter, 1_100));
        let ran = run(&turns, keys);

        assert_eq!(ran.written.len(), 2);
        let cold = ran.written.first().unwrap();
        let aided = ran.written.get(1).unwrap();
        assert_eq!(cold.paste_accepted, 0);
        assert!(aided.aided);
        assert_eq!(aided.paste_accepted, 1);
        assert_eq!(aided.paste_refused, 0);
        assert_eq!(
            aided.outcome,
            Outcome::Pass,
            "what was pasted reached the field and the verifier saw it"
        );
        assert_eq!(
            ran.outturn.landings(1),
            vec![Landing::Lapse],
            "the cold miss it opened with is still what was measured"
        );
    }

    #[test]
    fn a_sitting_declared_aided_takes_a_paste_from_the_first_key() {
        let turns = vec![drill_turn("a", Occasion::Review, true)];
        let ran = run(&turns, vec![pasting(RIGHT, 200), (Key::Enter, 300)]);
        let capture = ran.written.first().unwrap();
        assert!(capture.aided, "nothing here was measured");
        assert_eq!(capture.paste_accepted, 1);
        assert_eq!(capture.outcome, Outcome::Pass);
        assert_eq!(ran.outturn.landings(1), vec![Landing::Aided]);
    }

    #[test]
    fn a_cold_try_with_its_tries_spent_still_refuses_a_paste() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let mut keys = Vec::new();
        for _ in 0..3 {
            keys.extend(typing("wrong", 300));
        }
        keys.push(pasting(RIGHT, 4_000));
        keys.push((Key::Escape, 5_000));
        let ran = run(&turns, keys);
        assert_eq!(
            ran.written.iter().filter(|c| c.paste_accepted > 0).count(),
            0,
            "out of tries is still cold"
        );
        assert_eq!(
            ran.written.last().map(|c| c.outcome),
            Some(Outcome::Skip),
            "what was pasted never reached the field, so nothing was judged"
        );
    }

    #[test]
    fn a_paste_too_long_for_the_field_enters_none_of_itself() {
        let turns = vec![drill_turn("a", Occasion::Review, true)];
        let long = "x".repeat(crate::secret::CAPACITY + 1);
        let mut keys = vec![pasting(&long, 200)];
        keys.extend(typing(RIGHT, 400));
        let ran = run(&turns, keys);
        let capture = ran.written.first().unwrap();
        assert_eq!(capture.paste_accepted, 0);
        assert_eq!(capture.paste_refused, 1);
        assert_eq!(
            capture.outcome,
            Outcome::Pass,
            "the field held what was typed after it and nothing of the paste"
        );
    }

    #[test]
    fn the_field_goes_before_the_verifier_runs_and_the_keyboard_is_drained_after() {
        let turns = vec![drill_turn("escrow-p", Occasion::Review, false)];
        let ran = run(&turns, typing(RIGHT, 400));
        assert!(
            ran.drains > 0,
            "a key struck at the verifier must not reach the card after it"
        );
    }
}
