//! The sitting: what to ask for, and the loop that asks.
//!
//! A sitting works through its **turns**. One turn is one engram, and it is
//! either a **drill** — the secret is asked for and the verifier judges it — or
//! **dormant**, where this machine has no verifier and there is nothing to ask.
//! A dormant turn is a row that is skipped over and named on the way out; the
//! sitting does not attach, because an attachment is a claim and a drill is a
//! measurement, and one prompt should not be both.
//!
//! A drill opens with a **cold capture**, which is the one prompt in the tool
//! that measures a memory. A cold pass ends the turn. A cold fail opens the
//! tail: **follow-ups**, unbounded, masked, with the lookup on offer, until one
//! passes or the person leaves. One record per drill, written when it ends.
//!
//! The loop is generic over where keys come from, where the card is painted
//! and where each drill is written, so every path through it is driven by a
//! scripted list of keys in the tests. Only raw mode, the alternate screen and
//! the real `event::read()` sit outside that, in `tui`.

pub mod screen;

use anyhow::Result;
use jiff::civil::Date;
use relic_core::style::Style;

use crate::corpus::record::{Drilled, EngramId, Outcome};
use crate::ladder::{Ladder, Occasion, Standing};
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
    /// Whether the schedule asked, read off the engram's standing today.
    pub occasion: Occasion,
    /// What the ladder asked for.
    pub scheduled_interval_days: u32,
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

/// Which lineages a sitting asks about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selection<'a> {
    /// What the schedule is asking for today, and nothing else.
    Due,
    /// The named lineages, due or not. Empty means every active lineage.
    Named(&'a [Slug]),
}

/// Decide what to ask for.
///
/// The occasion is never declared: it is read off each engram's standing today,
/// in either selection, so a drill on a due engram is a review whoever asked
/// for it. **A dormant turn appears wherever its lineage appears**: an engram
/// with no verifier here cannot be drilled, and a row that says so is how the
/// sitting names the verb that puts one back.
pub fn roster(
    corpus: &crate::corpus::Corpus,
    verifiers: &crate::verifier::file::Verifiers,
    today: Date,
    ladder: &Ladder,
    selection: Selection<'_>,
) -> Vec<Turn> {
    let mut turns = Vec::new();
    for lineage in corpus.active() {
        if let Selection::Named(only) = selection
            && !only.is_empty()
            && !only.contains(&lineage.slug)
        {
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
        let occasion = match dossier.standing(today, ladder) {
            Standing::Due => Occasion::Review,
            Standing::Waiting { .. } if selection == Selection::Due => continue,
            Standing::Waiting { .. } => Occasion::Practice,
        };
        turns.push(Turn {
            slug: lineage.slug.clone(),
            engram: dossier.engram,
            task: Task::Drill(Box::new(Drilling {
                occasion,
                scheduled_interval_days: ladder.interval(dossier.rung),
                at_cap: ladder.at_cap(dossier.rung),
                verifier,
            })),
        });
    }
    turns
}

/// How a whole sitting ended.
#[derive(Clone, Debug, Default)]
pub struct Outturn {
    /// Every drill written, with the turn it was, in the order they ended.
    pub drills: Vec<(usize, Drilled)>,
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
    /// what was actually in front of a person. A turn passed over at its cold
    /// capture landed nowhere.
    pub fn landings(&self, turns: usize) -> Vec<Landing> {
        (0..turns)
            .filter_map(|index| {
                if self.dormant.contains(&index) {
                    return Some(Landing::Dormant);
                }
                let (_, drill) = self.drills.iter().find(|(turn, _)| *turn == index)?;
                Some(match drill.outcome {
                    Outcome::Pass => Landing::Pass,
                    Outcome::Fail => Landing::Fail,
                })
            })
            .collect()
    }

    /// Drills whose cold capture failed.
    pub fn failed(&self) -> usize {
        self.drills
            .iter()
            .filter(|(_, drill)| drill.outcome == Outcome::Fail)
            .count()
    }

    /// Fails a follow-up got past.
    pub fn recovered(&self) -> usize {
        self.drills
            .iter()
            .filter(|(_, drill)| drill.recovered)
            .count()
    }

    /// Drills in which the answer was looked up.
    pub fn aided(&self) -> usize {
        self.drills.iter().filter(|(_, drill)| drill.aided).count()
    }
}

/// Where one engram ended up in one sitting.
///
/// A separate vocabulary from [`Occasion`] on purpose: the occasion says why the
/// drill happened, the landing says how it went. Neither is spelled twice.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Landing {
    /// The cold capture passed.
    Pass,
    /// The cold capture failed, whatever followed.
    Fail,
    /// No verifier here, so nothing was asked. The row was skipped over.
    Dormant,
}

impl Landing {
    /// The one spelling of this landing. Every render site reads it here.
    pub fn word(self) -> &'static str {
        match self {
            Self::Pass => "pass",
            Self::Fail => "fail",
            Self::Dormant => "dormant",
        }
    }

    /// How it reads when it stands alone rather than in a tally.
    pub fn alone(self) -> &'static str {
        match self {
            Self::Pass => "passed",
            Self::Fail => "failed",
            Self::Dormant => "dormant",
        }
    }
}

/// What the line under the field says when a capture was wrong.
pub const NOT_IT: &str = "not it";

/// The standing status through the tail: the follow-up about to be typed.
fn follow_up_status(count: u16) -> String {
    format!("{NOT_IT} · follow-up {count}")
}

/// Where each drill goes the moment it ends.
///
/// Called before the next prompt is drawn, so a sitting cut short by a closed
/// window or a signal keeps every drill that had already ended.
pub type Record<'a> = dyn FnMut(usize, &Drilled) -> Result<()> + 'a;

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
    /// Where each drill goes.
    pub record: &'a mut Record<'a>,
}

/// Run the sitting.
///
/// # Errors
///
/// When the terminal cannot be read or written, a drill cannot be recorded, or
/// the verifier fails outright — which is different from refusing a secret,
/// and is not recorded as a fail.
pub fn run(turns: &[Turn], today: Date, wiring: Wiring<'_>) -> Result<Outturn> {
    let Wiring {
        console,
        verify,
        record,
    } = wiring;
    Session {
        turns,
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
    console: &'a mut dyn Console,
    verify: &'a Verify<'a>,
    record: &'a mut Record<'a>,
    today: Date,
    rows: Vec<screen::Row>,
    outturn: Outturn,
}

/// Where the tail of one drill has got to, beyond what is on the card.
#[derive(Clone, Copy, Debug, Default)]
struct Tail {
    /// Captures taken after the cold one.
    follow_ups: u16,
    /// Whether the lookup was taken. Latched: once the answer has been seen
    /// there is nothing left in this drill to protect.
    aided: bool,
}

impl Tail {
    fn drilled(self, engram: EngramId, ttfk_ms: Option<u64>, recovered: bool) -> Drilled {
        Drilled {
            engram,
            outcome: Outcome::Fail,
            ttfk_ms,
            follow_ups: self.follow_ups,
            recovered,
            aided: self.aided,
        }
    }
}

impl Session<'_> {
    fn run(mut self) -> Result<Outturn> {
        // The rectangle is the same for every state of the sitting, so the
        // opening card's height is the sitting's height. Read off a rendered
        // card rather than kept as a second constant that could drift from it.
        let opening = screen::card(
            &screen::Frame::running(self.today, &self.rows, Some(0)),
            &Field::hidden(),
            Style::PLAIN,
        );
        self.console.anchor(opening.height());

        for index in 0..self.turns.len() {
            let Some(turn) = self.turns.get(index) else {
                break;
            };
            let stopped = match &turn.task {
                Task::Drill(drilling) => self.drill(index, turn.engram, drilling)?,
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

    /// One drill: the cold capture, and the tail if it failed. `true` means the
    /// sitting was abandoned.
    fn drill(&mut self, index: usize, engram: EngramId, drilling: &Drilling) -> Result<bool> {
        // A cold capture is the one prompt in the tool that measures a memory,
        // and everything follows from that: it refuses a paste, draws nothing,
        // offers no reveal and moves no caret.
        self.set(index, screen::RowState::Cold);
        self.paint(index, Tone::Calm, None, false, &Field::resting(true))?;
        let entry = match self.read(index, None, false, true)? {
            Typed::Submitted(entry) => entry,
            // Not on offer at a cold capture, so never returned here.
            Typed::Lookup | Typed::Skipped => {
                self.set(index, screen::RowState::Skipped);
                return Ok(false);
            }
            Typed::Aborted => {
                self.set(index, screen::RowState::Aborted);
                self.outturn.aborted = true;
                return Ok(true);
            }
        };
        let outcome = self.judge(index, drilling, &entry.secret, None, false)?;
        let ttfk_ms = entry.forget().ttfk_ms;
        if outcome == Outcome::Pass {
            self.set(
                index,
                screen::RowState::Passed {
                    ttfk_ms,
                    recovered: false,
                },
            );
            self.keep(
                index,
                Drilled {
                    engram,
                    outcome,
                    ttfk_ms,
                    follow_ups: 0,
                    recovered: false,
                    aided: false,
                },
            )?;
            return Ok(false);
        }
        self.tail(index, engram, drilling, ttfk_ms)
    }

    /// Follow-ups after a cold fail, until one passes or the person leaves.
    fn tail(
        &mut self,
        index: usize,
        engram: EngramId,
        drilling: &Drilling,
        ttfk_ms: Option<u64>,
    ) -> Result<bool> {
        let mut tail = Tail::default();
        // The cold fail is the first refusal; a later one is a follow-up's.
        let mut refused = true;
        loop {
            let count = tail.follow_ups.saturating_add(1);
            self.set(index, screen::RowState::FollowUp { count });
            if let Some(row) = self.rows.get_mut(index) {
                row.set_aided(tail.aided);
            }
            let status = Some(follow_up_status(count));
            let lookup = !tail.aided;
            let field = Field::resting(false);
            if refused {
                self.flash(index, status.clone(), lookup, &field)?;
            } else {
                self.paint(index, Tone::Calm, status.clone(), lookup, &field)?;
            }
            refused = false;

            match self.read(index, status.as_deref(), lookup, false)? {
                Typed::Lookup => {
                    tail.aided = true;
                }
                Typed::Submitted(entry) => {
                    tail.follow_ups = tail.follow_ups.saturating_add(1);
                    let outcome = self.judge(index, drilling, &entry.secret, status, lookup)?;
                    let _ = entry.forget();
                    if outcome == Outcome::Pass {
                        self.set(
                            index,
                            screen::RowState::Passed {
                                ttfk_ms,
                                recovered: true,
                            },
                        );
                        self.keep(index, tail.drilled(engram, ttfk_ms, true))?;
                        return Ok(false);
                    }
                    self.set(
                        index,
                        screen::RowState::Failed {
                            follow_ups: tail.follow_ups,
                        },
                    );
                    refused = true;
                }
                Typed::Skipped => {
                    self.set(
                        index,
                        screen::RowState::Failed {
                            follow_ups: tail.follow_ups,
                        },
                    );
                    self.keep(index, tail.drilled(engram, ttfk_ms, false))?;
                    return Ok(false);
                }
                Typed::Aborted => {
                    self.set(
                        index,
                        screen::RowState::Failed {
                            follow_ups: tail.follow_ups,
                        },
                    );
                    self.keep(index, tail.drilled(engram, ttfk_ms, false))?;
                    self.outturn.aborted = true;
                    return Ok(true);
                }
            }
        }
    }

    /// What the verifier makes of one capture.
    ///
    /// An empty entry is a fail the verifier is never asked about: nothing was
    /// offered, and submitting nothing is what the card asks for in place of
    /// typing something to get past the prompt.
    fn judge(
        &mut self,
        index: usize,
        drilling: &Drilling,
        secret: &Secret,
        status: Option<String>,
        lookup: bool,
    ) -> Result<Outcome> {
        if secret.is_empty() {
            return Ok(Outcome::Fail);
        }
        self.set(index, screen::RowState::Checking);
        // Checking: the box and its caret are taken down and no key does
        // anything.
        let card = self.frame(index, Tone::Calm, status, lookup, &Field::hidden());
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

    /// Keep one drill, and write it out before anything else happens.
    fn keep(&mut self, index: usize, drilled: Drilled) -> Result<()> {
        (self.record)(index, &drilled)?;
        self.outturn.drills.push((index, drilled));
        Ok(())
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

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use jiff::civil::date;
    use relic_core::style::Style;

    use super::{Drilling, Landing, Outturn, Task, Turn, screen};
    use crate::corpus::record::{Drilled, EngramId, Outcome};
    use crate::ladder::Occasion;
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

    fn drill_turn(name: &str, occasion: Occasion) -> Turn {
        Turn {
            slug: name.parse().unwrap(),
            engram: EngramId::mint().unwrap(),
            task: Task::Drill(Box::new(Drilling {
                occasion,
                scheduled_interval_days: 1,
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
        written: Vec<(usize, Drilled)>,
        flashes: usize,
        drains: usize,
        /// The card as it last stood, joined into one string.
        last: String,
    }

    fn run(turns: &[Turn], keys: Vec<(Key, u64)>) -> Ran {
        let mut console = Fake::new(keys);
        let verify =
            |_: usize, _: &Verifier, secret: &Secret| Ok(secret.expose() == RIGHT.as_bytes());
        let written = std::cell::RefCell::new(Vec::new());
        let outturn = {
            let mut record = |index: usize, drilled: &Drilled| -> Result<()> {
                written.borrow_mut().push((index, drilled.clone()));
                Ok(())
            };
            super::run(
                turns,
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

    fn only(ran: &Ran) -> &Drilled {
        assert_eq!(ran.written.len(), 1, "one record per drill");
        &ran.written.first().unwrap().1
    }

    #[test]
    fn a_cold_pass_writes_one_pass_with_no_follow_ups() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let ran = run(&turns, typing(RIGHT, 400));
        let drill = only(&ran);
        assert_eq!(drill.engram, turns.first().unwrap().engram);
        assert_eq!(drill.outcome, Outcome::Pass);
        assert_eq!(drill.follow_ups, 0);
        assert!(!drill.recovered);
        assert!(!drill.aided);
        assert_eq!(drill.ttfk_ms, Some(400));
        assert_eq!(ran.outturn.landings(1), vec![Landing::Pass]);
        assert_eq!(ran.outturn.failed(), 0);
        assert_eq!(
            ran.outturn.rows.first().map(|row| row.state),
            Some(screen::RowState::Passed {
                ttfk_ms: Some(400),
                recovered: false,
            })
        );
    }

    #[test]
    fn a_cold_fail_and_a_follow_up_pass_write_one_fail_recovered() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let mut keys = typing("wrong", 300);
        keys.extend(typing(RIGHT, 900));
        let ran = run(&turns, keys);

        let drill = only(&ran);
        assert_eq!(
            drill.outcome,
            Outcome::Fail,
            "the cold capture is what was measured"
        );
        assert_eq!(drill.follow_ups, 1);
        assert!(drill.recovered);
        assert!(!drill.aided);
        assert_eq!(
            drill.ttfk_ms,
            Some(300),
            "the latency is the cold capture's"
        );
        assert_eq!(ran.outturn.landings(1), vec![Landing::Fail]);
        assert_eq!(ran.outturn.failed(), 1);
        assert_eq!(ran.outturn.recovered(), 1);
        assert!(ran.flashes > 0, "a refusal is flashed");
        assert_eq!(
            ran.outturn.rows.first().map(|row| row.state),
            Some(screen::RowState::Passed {
                ttfk_ms: Some(300),
                recovered: true,
            })
        );
    }

    #[test]
    fn a_follow_up_that_fails_asks_again_and_counts() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let mut keys = typing("wrong", 300);
        keys.extend(typing("still wrong", 900));
        keys.extend(typing("no", 1_500));
        keys.extend(typing(RIGHT, 2_000));
        let ran = run(&turns, keys);
        let drill = only(&ran);
        assert_eq!(drill.follow_ups, 3);
        assert!(drill.recovered);
    }

    #[test]
    fn the_lookup_latches_aided_onto_the_record() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let mut keys = typing("wrong", 300);
        keys.push((Key::Lookup, 900));
        keys.extend(typing(RIGHT, 1_000));
        let ran = run(&turns, keys);
        let drill = only(&ran);
        assert_eq!(drill.outcome, Outcome::Fail);
        assert!(drill.aided);
        assert!(drill.recovered);
        assert_eq!(drill.follow_ups, 1, "a lookup is not a capture");
        assert_eq!(ran.outturn.aided(), 1);
    }

    #[test]
    fn the_lookup_is_not_on_offer_at_a_cold_capture() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let mut keys = vec![(Key::Lookup, 200)];
        keys.extend(typing(RIGHT, 400));
        let ran = run(&turns, keys);
        let drill = only(&ran);
        assert!(!drill.aided, "the key did nothing");
        assert_eq!(drill.outcome, Outcome::Pass);
        assert!(!ran.last.contains("^L"));
    }

    #[test]
    fn escape_at_a_cold_capture_writes_nothing() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let ran = run(&turns, vec![(Key::Escape, 200)]);
        assert!(ran.written.is_empty());
        assert_eq!(ran.outturn.landings(1), Vec::new());
        assert!(!ran.outturn.aborted);
        assert_eq!(
            ran.outturn.rows.first().map(|row| row.state),
            Some(screen::RowState::Skipped)
        );
    }

    #[test]
    fn escape_in_a_follow_up_writes_one_fail_not_recovered() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let mut keys = typing("wrong", 300);
        keys.push((Key::Escape, 900));
        let ran = run(&turns, keys);
        let drill = only(&ran);
        assert_eq!(drill.outcome, Outcome::Fail);
        assert_eq!(drill.follow_ups, 0);
        assert!(!drill.recovered);
        assert!(!ran.outturn.aborted);
    }

    #[test]
    fn an_interrupt_in_a_follow_up_writes_the_drill_and_abandons_the_sitting() {
        let turns = vec![
            drill_turn("a", Occasion::Review),
            drill_turn("b", Occasion::Review),
        ];
        let mut keys = typing("wrong", 300);
        keys.push((Key::Lookup, 800));
        keys.push((Key::Interrupt, 900));
        let ran = run(&turns, keys);
        assert!(ran.outturn.aborted);
        let drill = only(&ran);
        assert_eq!(drill.outcome, Outcome::Fail);
        assert!(drill.aided, "what was latched is on the record");
        assert!(!drill.recovered);
        assert_eq!(
            ran.outturn.landings(2),
            vec![Landing::Fail],
            "the second turn was never reached"
        );
    }

    #[test]
    fn an_interrupt_at_a_cold_capture_writes_nothing_and_asks_nobody_else() {
        let turns = vec![
            drill_turn("a", Occasion::Review),
            drill_turn("b", Occasion::Review),
        ];
        let ran = run(&turns, vec![(Key::Interrupt, 200)]);
        assert!(ran.outturn.aborted);
        assert!(ran.written.is_empty());
        assert_eq!(ran.outturn.landings(2), Vec::new());
    }

    #[test]
    fn submitting_nothing_cold_is_a_fail_with_no_latency() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let ran = run(&turns, vec![(Key::Enter, 800), (Key::Escape, 1_200)]);
        let drill = only(&ran);
        assert_eq!(drill.outcome, Outcome::Fail);
        assert_eq!(drill.ttfk_ms, None, "nothing was typed");
        assert_eq!(ran.outturn.landings(1), vec![Landing::Fail]);
    }

    #[test]
    fn the_tail_says_which_follow_up_is_next_and_offers_the_lookup() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let mut keys = typing("wrong", 300);
        keys.extend(typing("wrong", 900));
        keys.push((Key::Escape, 2_000));
        let ran = run(&turns, keys);
        assert!(ran.last.contains("not it · follow-up 2"), "{}", ran.last);
        assert!(ran.last.contains("^L"), "{}", ran.last);
        assert!(ran.last.contains("a follow-up"), "{}", ran.last);
    }

    #[test]
    fn once_looked_up_the_lookup_is_withdrawn_and_the_card_says_so() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let mut keys = typing("wrong", 300);
        keys.push((Key::Lookup, 900));
        keys.push((Key::Escape, 1_000));
        let ran = run(&turns, keys);
        assert!(!ran.last.contains("^L"), "{}", ran.last);
        assert!(ran.last.contains("looked up"), "{}", ran.last);
        assert!(ran.last.contains("aided"), "{}", ran.last);
    }

    #[test]
    fn the_verifier_is_told_which_turn_it_is_judging() {
        let turns = vec![
            drill_turn("a", Occasion::Review),
            drill_turn("b", Occasion::Review),
        ];
        let mut keys = typing(RIGHT, 300);
        keys.extend(typing(RIGHT, 900));
        let mut console = Fake::new(keys);
        let seen = std::cell::RefCell::new(Vec::new());
        let verify = |index: usize, _: &Verifier, _: &Secret| {
            seen.borrow_mut().push(index);
            Ok(true)
        };
        let mut record = |_: usize, _: &Drilled| -> Result<()> { Ok(()) };
        super::run(
            &turns,
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
        let turns = vec![dormant_turn("a"), drill_turn("b", Occasion::Review)];
        let ran = run(&turns, typing(RIGHT, 300));
        assert_eq!(
            ran.written.len(),
            1,
            "nothing is written for a dormant turn"
        );
        assert_eq!(ran.written.first().map(|(turn, _)| *turn), Some(1));
        assert_eq!(ran.outturn.dormant, vec![0]);
        assert_eq!(
            ran.outturn.landings(2),
            vec![Landing::Dormant, Landing::Pass]
        );
        assert_eq!(ran.outturn.failed(), 0);
        assert!(!ran.outturn.aborted);
        assert!(matches!(
            ran.outturn.rows.first().map(|row| row.state),
            Some(screen::RowState::Dormant)
        ));
        assert!(ran.last.contains("dormant"), "{}", ran.last);
    }

    #[test]
    fn a_cold_capture_refuses_a_paste() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let mut keys = vec![pasting(RIGHT, 200)];
        keys.push((Key::Enter, 300));
        let ran = run(&turns, keys);
        let drill = only(&ran);
        assert_eq!(
            drill.outcome,
            Outcome::Fail,
            "nothing of the paste reached the field, so nothing was offered"
        );
        assert!(ran.flashes > 0, "and the refusal was said");
    }

    #[test]
    fn a_follow_up_takes_a_paste() {
        let turns = vec![drill_turn("a", Occasion::Review)];
        let mut keys = typing("wrong", 300);
        keys.push(pasting(RIGHT, 1_000));
        keys.push((Key::Enter, 1_100));
        let ran = run(&turns, keys);
        let drill = only(&ran);
        assert_eq!(drill.outcome, Outcome::Fail);
        assert!(
            drill.recovered,
            "what was pasted reached the field and the verifier saw it"
        );
        assert!(!drill.aided, "a paste is not a lookup");
    }

    #[test]
    fn the_field_goes_before_the_verifier_runs_and_the_keyboard_is_drained_after() {
        let turns = vec![drill_turn("escrow-p", Occasion::Review)];
        let ran = run(&turns, typing(RIGHT, 400));
        assert!(
            ran.drains > 0,
            "a key struck at the verifier must not reach the card after it"
        );
    }
}
