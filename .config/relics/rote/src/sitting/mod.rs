//! The sitting: what to ask for, and the loop that asks.
//!
//! A sitting works through its **turns**. One turn is one engram, and it is
//! either a **drill** — the secret is asked for and the verifier judges it — or
//! an **attachment**, where this machine has no verifier and the secret is taken
//! so that one can be made. An attachment measures nothing and is judged by
//! nothing; it is how a restore onto a new machine comes back.
//!
//! The loop is generic over where keys come from, where the card is painted,
//! where each capture is written and where a verifier is minted, so every path
//! through it is driven by a scripted list of keys in the tests. Only raw mode,
//! the alternate screen and the real `event::read()` sit outside that, in `tui`.

pub mod screen;

use anyhow::Result;
use jiff::civil::Date;
use relic_core::style::Style;

use crate::corpus::drill::Landing;
use crate::corpus::record::{EngramId, Outcome};
use crate::intake::{DIFFERED, EMPTY, Pair, Pairing};
use crate::ladder::{Ladder, Occasion, Rung, Standing};
use crate::secret::Secret;
use crate::slug::Slug;
use crate::tui::{Card, Console, Typed, card::Tone, read_secret};
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
    /// Take the secret so a verifier can be made here.
    Attach(Box<Attaching>),
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
    /// an attachment, and the type says so rather than a branch inside the loop.
    pub verifier: Verifier,
}

/// An attachment, as the roster planned it.
#[derive(Clone, Debug)]
pub struct Attaching {
    /// What is being continued, in one line, so a person has a beat to notice
    /// they are about to attach the wrong thing.
    pub dossier: String,
    /// Whether a verifier is already held here and is being replaced.
    pub replacing: bool,
}

impl Turn {
    /// Whether this turn is an attachment.
    pub fn attaching(&self) -> bool {
        matches!(self.task, Task::Attach(_))
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
/// **An attachment turn appears wherever its lineage appears**, in either mode,
/// because an engram with no verifier here cannot be drilled at all and the
/// sitting is how one gets made.
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
            let ordinal = lineage.ordinal(&dossier.engram).unwrap_or(1);
            turns.push(Turn {
                slug: lineage.slug.clone(),
                engram: dossier.engram,
                task: Task::Attach(Box::new(Attaching {
                    dossier: describe(&lineage.label(ordinal), dossier, today, ladder),
                    replacing: false,
                })),
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

/// One line naming what an attachment would continue.
pub fn describe(
    label: &str,
    dossier: &crate::corpus::Dossier,
    today: Date,
    ladder: &Ladder,
) -> String {
    let mut parts = vec![
        label.to_owned(),
        format!("enrolled {}", dossier.minted),
        format!("rung {}", dossier.rung.get()),
    ];
    match dossier.last_review {
        Some(day) if day != dossier.minted => parts.push(format!(
            "last reviewed {} days ago",
            crate::ladder::days_between(day, today)
        )),
        Some(_) | None => parts.push("never reviewed".to_owned()),
    }
    let _ = ladder;
    parts.join(" · ")
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
    /// Backspaces.
    pub corrections: u32,
    /// Pastes refused at this prompt.
    pub paste_refused: u32,
    /// Where the engram lands after this sample.
    pub rung_after: Rung,
}

/// How a whole sitting ended.
#[derive(Clone, Debug, Default)]
pub struct Outturn {
    /// Every sample, in the order they were typed.
    pub captures: Vec<Capture>,
    /// The turns that ended in a verifier being minted here.
    pub attached: Vec<usize>,
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
                if self.attached.contains(&index) {
                    return Some(Landing::Attached);
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

/// Where each capture goes the moment it is taken.
///
/// Called before the next prompt is drawn, so a sitting cut short by a closed
/// window or a signal keeps every reading it had already produced.
pub type Record<'a> = dyn FnMut(&Capture) -> Result<()> + 'a;

/// Where a verifier is minted, for an attachment.
///
/// The verifier is written **before** the record of it, so a failure between the
/// two leaves a fact with no record rather than a record with no fact.
pub type Attach<'a> = dyn FnMut(usize, &Secret) -> Result<()> + 'a;

/// How a secret is judged. Passed in rather than reached for, so the loop can be
/// driven in a test without paying 256 MiB and half a second per key.
pub type Verify<'a> = dyn Fn(&Verifier, &Secret) -> Result<bool> + 'a;

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
    /// Where a verifier is minted.
    pub attach: &'a mut Attach<'a>,
}

/// Run the sitting.
///
/// # Errors
///
/// When the terminal cannot be read or written, a capture cannot be recorded, a
/// verifier cannot be minted, or the verifier fails outright — which is
/// different from refusing a secret, and is not recorded as a lapse.
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
        attach,
    } = wiring;
    Session {
        turns,
        max_attempts,
        ladder,
        console,
        verify,
        record,
        attach,
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
    attach: &'a mut Attach<'a>,
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
            Style::PLAIN,
        );
        self.console.anchor(opening.height());

        for index in 0..self.turns.len() {
            let Some(turn) = self.turns.get(index) else {
                break;
            };
            let stopped = match &turn.task {
                Task::Drill(drilling) => self.drill(index, drilling)?,
                Task::Attach(_) => self.attachment(index)?,
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
        let mut prompt = Prompt::new(drilling.aided, drilling.rung);
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
        self.paint(index, Tone::Calm, status.clone(), lookup)?;

        let entry = match self.read(index, status.as_deref(), lookup)? {
            // The cold tries are spent: what the field will still take is the
            // lookup or the way out, and a typed answer is refused rather than
            // judged, since a fourth cold try would be a reading nobody asked
            // for.
            Typed::Submitted(entry) if prompt.exhausted && !prompt.aided => {
                let _ = entry.forget();
                self.flash(index, Some(OUT_OF_TRIES.to_owned()), lookup)?;
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
            paste_refused: timings.paste_refused,
            rung_after,
        })?;

        if outcome == Outcome::Pass {
            self.set(
                index,
                screen::RowState::Passed {
                    total_ms: timings.total_ms,
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
        prompt.exhausted = prompt.ordinal >= self.max_attempts;
        let next = Prompt {
            ordinal: prompt.ordinal.saturating_add(1),
            ..*prompt
        }
        .status(self.max_attempts);
        self.flash(index, next, true)?;
        Ok(Next::Again)
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

    /// Take a secret twice and mint a verifier for an engram this machine is
    /// dormant on. `true` means the sitting was abandoned.
    ///
    /// Nothing here is judged, so nothing here is recorded as a capture. There
    /// is also no lookup on offer: offering one would imply that what is typed
    /// is being checked against something, and it is not.
    fn attachment(&mut self, index: usize) -> Result<bool> {
        let mut pair = Pair::new(self.max_attempts);
        let mut status: Option<String> = None;
        loop {
            let state = if pair.holds_one() {
                screen::RowState::Confirming
            } else {
                screen::RowState::Claiming
            };
            self.set(index, state);
            self.paint(index, Tone::Calm, status.clone(), false)?;

            let entry = match self.read(index, status.as_deref(), false)? {
                Typed::Submitted(entry) => entry,
                // There is nothing here to concede, so the lookup is not on
                // offer and a skip simply leaves the engram dormant.
                Typed::Lookup => continue,
                Typed::Skipped => {
                    self.set(index, screen::RowState::Skipped);
                    return Ok(false);
                }
                Typed::Aborted => {
                    self.set(index, screen::RowState::Aborted);
                    self.outturn.aborted = true;
                    return Ok(true);
                }
            };

            match pair.offer(entry.secret) {
                Pairing::Again => status = None,
                Pairing::Agreed(secret) => {
                    (self.attach)(index, &secret)?;
                    drop(secret);
                    self.outturn.attached.push(index);
                    self.set(index, screen::RowState::Attached);
                    return Ok(false);
                }
                Pairing::Differed => {
                    status = Some(DIFFERED.to_owned());
                    self.flash(index, status.clone(), false)?;
                }
                Pairing::OutOfRounds => {
                    self.set(index, screen::RowState::Differed);
                    self.flash(index, Some(DIFFERED.to_owned()), false)?;
                    return Ok(false);
                }
                Pairing::Empty => {
                    status = Some(EMPTY.to_owned());
                    self.flash(index, status.clone(), false)?;
                }
            }
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
        self.paint(index, Tone::Calm, status, lookup)?;
        Ok(if (self.verify)(&drilling.verifier, secret)? {
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

    /// A refusal is a flash and a counter, not a paragraph.
    fn flash(&mut self, index: usize, status: Option<String>, lookup: bool) -> Result<()> {
        let alarm = self.frame(index, Tone::Alarm, status.clone(), lookup);
        self.console.flash(&alarm)?;
        let calm = self.frame(index, Tone::Calm, status, lookup);
        self.console.paint(&calm)
    }

    fn read(&mut self, index: usize, status: Option<&str>, lookup: bool) -> Result<Typed> {
        let today = self.today;
        let rows = self.rows.clone();
        let style = self.console.style();
        let status = status.map(str::to_owned);
        let mut build = |refusal: crate::tui::Refusal| {
            let mut frame = screen::Frame::running(today, &rows, Some(index));
            frame.tone = refusal.tone();
            frame.status = refusal
                .status()
                .map(str::to_owned)
                .or_else(|| status.clone());
            frame.lookup = lookup;
            screen::card(&frame, style)
        };
        read_secret(self.console, lookup, &mut build)
    }
}

/// Where one drill has got to, beyond what is on the card.
#[derive(Clone, Copy, Debug)]
struct Prompt {
    ordinal: u8,
    aided: bool,
    missed: bool,
    exhausted: bool,
    /// Where the first sample put the engram. Later samples in the same drill
    /// carry it rather than deciding again.
    resolved: Option<Rung>,
}

impl Prompt {
    fn new(aided: bool, _rung: Rung) -> Self {
        Self {
            ordinal: 0,
            aided,
            missed: false,
            exhausted: false,
            resolved: None,
        }
    }

    /// What the line under the field says about how this drill is going.
    fn status(self, max_attempts: u8) -> Option<String> {
        (self.missed && !self.exhausted).then(|| {
            format!(
                "not it · try {} of {max_attempts}",
                self.ordinal.min(max_attempts)
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use jiff::civil::date;
    use relic_core::style::Style;

    use super::{Attaching, Capture, Drilling, Outturn, Task, Turn, screen};
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
    struct Fake {
        keys: Vec<(Key, u64)>,
        at: usize,
        elapsed: u64,
        flashes: usize,
        last: Vec<String>,
    }

    impl Fake {
        fn new(keys: Vec<(Key, u64)>) -> Self {
            Self {
                keys,
                at: 0,
                elapsed: 0,
                flashes: 0,
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

    fn attach_turn(name: &str) -> Turn {
        Turn {
            slug: name.parse().unwrap(),
            engram: EngramId::mint().unwrap(),
            task: Task::Attach(Box::new(Attaching {
                dossier: format!("{name}@1 · enrolled 2026-09-01 · rung 3"),
                replacing: false,
            })),
        }
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
        attached: Vec<String>,
        flashes: usize,
    }

    fn run(turns: &[Turn], keys: Vec<(Key, u64)>) -> Ran {
        let mut console = Fake::new(keys);
        let ladder = Ladder::default();
        let verify = |_: &Verifier, secret: &Secret| Ok(secret.expose() == RIGHT.as_bytes());
        let written = std::cell::RefCell::new(Vec::new());
        let attached = std::cell::RefCell::new(Vec::new());
        let outturn = {
            let mut record = |capture: &Capture| -> Result<()> {
                written.borrow_mut().push(*capture);
                Ok(())
            };
            let mut attach = |_index: usize, secret: &Secret| -> Result<()> {
                attached
                    .borrow_mut()
                    .push(String::from_utf8_lossy(secret.expose()).into_owned());
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
                    attach: &mut attach,
                },
            )
            .unwrap()
        };
        Ran {
            outturn,
            written: written.into_inner(),
            attached: attached.into_inner(),
            flashes: console.flashes,
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
    fn an_attachment_takes_the_secret_twice_and_writes_no_capture() {
        let turns = vec![attach_turn("a")];
        let mut keys = typing(RIGHT, 300);
        keys.extend(typing(RIGHT, 900));
        let ran = run(&turns, keys);

        assert!(
            ran.written.is_empty(),
            "nothing was judged, so nothing was measured"
        );
        assert_eq!(ran.attached, vec![RIGHT.to_owned()]);
        assert_eq!(ran.outturn.attached, vec![0]);
        assert_eq!(ran.outturn.landings(1), vec![Landing::Attached]);
        assert_eq!(ran.outturn.missed(), 0);
        assert!(matches!(
            ran.outturn.rows.first().map(|row| row.state),
            Some(screen::RowState::Attached)
        ));
    }

    #[test]
    fn an_attachment_whose_entries_never_agree_mints_nothing() {
        let turns = vec![attach_turn("a")];
        let mut keys = Vec::new();
        for _ in 0..3 {
            keys.extend(typing("one", 300));
            keys.extend(typing("two", 600));
        }
        let ran = run(&turns, keys);
        assert!(ran.attached.is_empty());
        assert!(ran.outturn.attached.is_empty());
        assert!(matches!(
            ran.outturn.rows.first().map(|row| row.state),
            Some(screen::RowState::Differed)
        ));
    }

    #[test]
    fn an_attachment_refuses_an_empty_entry_rather_than_conceding_to_it() {
        let turns = vec![attach_turn("a")];
        let mut keys = vec![(Key::Enter, 200)];
        keys.extend(typing(RIGHT, 400));
        keys.extend(typing(RIGHT, 800));
        let ran = run(&turns, keys);
        assert_eq!(
            ran.attached,
            vec![RIGHT.to_owned()],
            "the empty entry was refused and the pair started over"
        );
        assert!(ran.flashes > 0);
    }

    #[test]
    fn escape_leaves_an_attachment_dormant_with_nothing_written() {
        let turns = vec![attach_turn("a")];
        let ran = run(&turns, vec![(Key::Escape, 200)]);
        assert!(ran.attached.is_empty());
        assert!(ran.written.is_empty());
        assert!(matches!(
            ran.outturn.rows.first().map(|row| row.state),
            Some(screen::RowState::Skipped)
        ));
    }

    #[test]
    fn an_attachment_and_a_drill_sit_in_one_roster() {
        let turns = vec![attach_turn("a"), drill_turn("b", Occasion::Review, false)];
        let mut keys = typing(RIGHT, 300);
        keys.extend(typing(RIGHT, 900));
        keys.extend(typing(RIGHT, 1_500));
        let ran = run(&turns, keys);
        assert_eq!(ran.attached.len(), 1);
        assert_eq!(ran.written.len(), 1);
        assert_eq!(
            ran.outturn.landings(2),
            vec![Landing::Attached, Landing::Pass]
        );
    }

    #[test]
    fn a_refused_paste_is_counted_and_never_reaches_the_buffer() {
        let turns = vec![drill_turn("a", Occasion::Review, false)];
        let mut keys = vec![(Key::Paste, 200)];
        keys.extend(typing(RIGHT, 400));
        let ran = run(&turns, keys);
        let capture = ran.written.first().unwrap();
        assert_eq!(capture.paste_refused, 1);
        assert_eq!(capture.outcome, Outcome::Pass);
    }
}
