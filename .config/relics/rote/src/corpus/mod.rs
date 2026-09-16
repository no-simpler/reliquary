//! The projection: what the record adds up to.
//!
//! The chains are the record; this is a reading of them, rebuilt on every
//! invocation. There is no second file holding schedule state, so state and
//! record have no way to disagree.
//!
//! **Replay derives the rung, the anchor and the occasion; the record stores
//! none of them.** A drill on a day the engram is due is a review, and a
//! review passed climbs a rung; a fail on any day returns to the foot. So a
//! later change to the ladder re-derives the whole history, which is the
//! feature. Everything with a threshold in it — the streak above all — is
//! computed in `stats` over the derived drills, which is both the right home
//! for a threshold and the only way to stay correct when two machines wrote
//! without having seen each other.
//!
//! **Every date here is monotone non-decreasing.** The merge orders by instant
//! while the schedule reads civil days, so a second machine in another zone can
//! hand replay a record whose day precedes its predecessor's. Assigning it
//! unchecked would walk an anchor backwards and make every interval after it
//! nonsense.

pub mod chain;
pub mod drill;
pub mod record;

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;

use crate::ladder::{self, Ladder, Occasion, Rung, Standing};
use crate::slug::Slug;
use drill::Drill;
use record::{Drilled, EngramId, Event, Outcome, Record};

/// What the record says about one enrolled secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dossier {
    /// Which secret this is the record of.
    pub engram: EngramId,
    /// The day it was enrolled or rotated in.
    pub minted: Date,
    /// The day it stepped down, if it has. `None` means it is the current one.
    pub superseded: Option<Date>,
    /// Where it sits on the ladder.
    pub rung: Rung,
    /// The day the schedule counts from: the last review, else the mint.
    pub anchor: Date,
    /// The last day the secret was in front of a person by any route — a
    /// drill, an enrolment, a rotation, an attachment.
    pub last_exposed: Date,
    /// When that was, to the second.
    pub last_exposed_at: Timestamp,
    /// The last day the schedule moved: a review passed, or a fail on any day.
    pub last_review: Option<Date>,
}

impl Dossier {
    fn minted(engram: EngramId, day: Date, at: Timestamp) -> Self {
        Self {
            engram,
            minted: day,
            superseded: None,
            rung: Rung::FIRST,
            anchor: day,
            last_exposed: day,
            last_exposed_at: at,
            last_review: None,
        }
    }

    /// Whether this is the engram a drill would ask for.
    pub fn is_current(&self) -> bool {
        self.superseded.is_none()
    }

    /// Where it stands against its own schedule today.
    pub fn standing(&self, today: Date, ladder: &Ladder) -> Standing {
        ladder.standing(today, self.anchor, self.rung)
    }

    /// The day the schedule next asks for it.
    pub fn due(&self, ladder: &Ladder) -> Date {
        ladder.due_day(self.anchor, self.rung)
    }

    /// Whole days past the due day, zero while not yet due.
    pub fn days_overdue(&self, today: Date, ladder: &Ladder) -> u32 {
        ladder::days_between(self.due(ladder), today)
    }

    fn expose(&mut self, day: Date, at: Timestamp) {
        self.last_exposed = self.last_exposed.max(day);
        self.last_exposed_at = self.last_exposed_at.max(at);
    }
}

/// One slug's whole life: an ordered series of engrams.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lineage {
    /// The name, and the key.
    pub slug: Slug,
    /// Whether losing this one is unrecoverable rather than inconvenient. A
    /// property of the name's role, so it is inherited across rotations.
    pub critical: bool,
    /// Whether it has left the schedule.
    pub retired: bool,
    /// Every engram it has held, in mint order. The last is the current one.
    pub engrams: Vec<Dossier>,
}

impl Lineage {
    fn new(slug: Slug, critical: bool) -> Self {
        Self {
            slug,
            critical,
            retired: false,
            engrams: Vec::new(),
        }
    }

    /// The engram a drill would ask for.
    pub fn current(&self) -> Option<&Dossier> {
        self.engrams.last()
    }

    /// Its position within the lineage, counting from one. The `N` in `slug@N`.
    pub fn ordinal(&self, engram: &EngramId) -> Option<usize> {
        self.engrams
            .iter()
            .position(|d| d.engram == *engram)
            .map(|index| index.saturating_add(1))
    }

    /// How the current engram is spelled to a person.
    pub fn current_label(&self) -> String {
        self.label(self.engrams.len())
    }

    /// How the engram at a one-based position is spelled to a person.
    pub fn label(&self, ordinal: usize) -> String {
        format!("{}@{ordinal}", self.slug)
    }

    /// One engram's record, by identity.
    pub fn dossier(&self, engram: &EngramId) -> Option<&Dossier> {
        self.engrams.iter().find(|d| d.engram == *engram)
    }

    fn dossier_mut(&mut self, engram: &EngramId) -> Option<&mut Dossier> {
        self.engrams.iter_mut().find(|d| d.engram == *engram)
    }

    fn holds(&self, engram: &EngramId) -> bool {
        self.engrams.iter().any(|d| d.engram == *engram)
    }

    /// Make an engram the current one.
    ///
    /// Every engram still standing is superseded first, so a lineage holds at
    /// most one current engram by construction — whatever a merge of two
    /// chains hands replay, an enrolment over a live lineage or a rotation
    /// naming a predecessor this one never saw included.
    fn make_current(&mut self, engram: EngramId, day: Date, at: Timestamp) {
        for dossier in &mut self.engrams {
            dossier.superseded.get_or_insert(day);
        }
        self.retired = false;
        self.engrams.push(Dossier::minted(engram, day, at));
    }
}

/// Every lineage the record knows about.
#[derive(Clone, Debug, Default)]
pub struct Corpus {
    lineages: BTreeMap<Slug, Lineage>,
    by_engram: BTreeMap<EngramId, Slug>,
    drills: Vec<Drill>,
}

impl Corpus {
    /// Rebuild from records in merged order.
    pub fn replay<'a>(records: impl IntoIterator<Item = &'a Record>, ladder: &Ladder) -> Self {
        let mut corpus = Self::default();
        for record in records {
            corpus.apply(record, ladder);
        }
        corpus
    }

    /// One lineage, if the record knows it.
    pub fn lineage(&self, slug: &Slug) -> Option<&Lineage> {
        self.lineages.get(slug)
    }

    /// Every lineage, retired ones included, in name order.
    pub fn lineages(&self) -> impl Iterator<Item = &Lineage> {
        self.lineages.values()
    }

    /// The lineage holding an engram.
    pub fn owner(&self, engram: &EngramId) -> Option<&Lineage> {
        self.by_engram
            .get(engram)
            .and_then(|slug| self.lineage(slug))
    }

    /// Whether any lineage has ever held this engram.
    pub fn knows(&self, engram: &EngramId) -> bool {
        self.by_engram.contains_key(engram)
    }

    /// Every drill, in merged order, with what the schedule made of each.
    pub fn drills(&self) -> &[Drill] {
        &self.drills
    }

    /// How an engram is spelled to a person, wherever it sits.
    pub fn label(&self, engram: &EngramId) -> String {
        match self.owner(engram) {
            Some(lineage) => match lineage.ordinal(engram) {
                Some(ordinal) => lineage.label(ordinal),
                None => engram.to_string(),
            },
            None => engram.to_string(),
        }
    }

    /// The lineages still on the schedule, criticals first and then by name.
    ///
    /// The order is fixed rather than shuffled: a randomised order would trade
    /// away comparability between one day's timings and the next, which is the
    /// measurement the record exists for.
    pub fn active(&self) -> Vec<&Lineage> {
        let mut out: Vec<&Lineage> = self
            .lineages
            .values()
            .filter(|l| !l.retired && l.current().is_some())
            .collect();
        out.sort_by(|a, b| {
            b.critical
                .cmp(&a.critical)
                .then_with(|| a.slug.cmp(&b.slug))
        });
        out
    }

    fn apply(&mut self, record: &Record, ladder: &Ladder) {
        match &record.event {
            Event::Enroll(event) => {
                let lineage = self
                    .lineages
                    .entry(event.slug.clone())
                    .or_insert_with(|| Lineage::new(event.slug.clone(), event.critical));
                if lineage.holds(&event.engram) {
                    return;
                }
                lineage.critical = event.critical;
                lineage.make_current(event.engram, record.day, record.at);
                self.by_engram.insert(event.engram, event.slug.clone());
            }
            Event::Rotate(event) => {
                let Some(lineage) = self.lineages.get_mut(&event.slug) else {
                    return;
                };
                if lineage.holds(&event.to) {
                    return;
                }
                lineage.make_current(event.to, record.day, record.at);
                self.by_engram.insert(event.to, event.slug.clone());
            }
            Event::Attach(event) => {
                // A verifier was minted here. Nothing about the memory changed,
                // but the secret was typed, so the retention interval restarts.
                if let Some(dossier) = self.dossier_mut(&event.engram) {
                    dossier.expose(record.day, record.at);
                }
            }
            Event::Retire(event) => {
                if let Some(lineage) = self.lineages.get_mut(&event.slug) {
                    lineage.retired = true;
                }
            }
            Event::Drill(event) => {
                if let Some(dossier) = self.dossier_mut(&event.engram) {
                    let view = apply_drill(dossier, record, event, ladder);
                    self.drills.push(view);
                }
            }
        }
    }

    fn dossier_mut(&mut self, engram: &EngramId) -> Option<&mut Dossier> {
        let slug = self.by_engram.get(engram)?.clone();
        self.lineages.get_mut(&slug)?.dossier_mut(engram)
    }
}

/// What one drill does to an engram, and what it was.
///
/// The occasion is read off the standing **before** the dossier moves: a drill
/// on a day the engram is due is a review, whatever anyone declared. Then the
/// three-line reconciliation — a review passed climbs and anchors; a practice
/// passed moves nothing; a fail on any day returns to the foot and anchors.
fn apply_drill(dossier: &mut Dossier, record: &Record, event: &Drilled, ladder: &Ladder) -> Drill {
    let occasion = match ladder.standing(record.day, dossier.anchor, dossier.rung) {
        Standing::Due => Occasion::Review,
        Standing::Waiting { .. } => Occasion::Practice,
    };
    let view = Drill {
        engram: event.engram,
        day: record.day,
        at: record.at,
        occasion,
        scheduled_days: ladder.interval(dossier.rung),
        since_anchor: ladder::days_between(dossier.anchor, record.day),
        gap: ladder::days_between(dossier.last_exposed, record.day),
        outcome: event.outcome,
        ttfk_ms: event.ttfk_ms,
        follow_ups: event.follow_ups,
        recovered: event.recovered,
        aided: event.aided,
    };
    dossier.expose(record.day, record.at);
    let moved = match (occasion, event.outcome) {
        (Occasion::Review, Outcome::Pass) => Some(ladder.advanced(dossier.rung)),
        (Occasion::Practice, Outcome::Pass) => None,
        (Occasion::Review | Occasion::Practice, Outcome::Fail) => Some(Rung::FIRST),
    };
    if let Some(rung) = moved {
        dossier.rung = rung;
        dossier.anchor = dossier.anchor.max(record.day);
        dossier.last_review = Some(match dossier.last_review {
            Some(previous) => previous.max(record.day),
            None => record.day,
        });
    }
    view
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Corpus, record::*};
    use crate::ladder::{Ladder, Occasion, Rung, Standing};

    fn at(day: Date) -> jiff::Timestamp {
        day.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp()
    }

    fn line(day: Date, event: Event) -> Record {
        Record {
            v: SCHEMA,
            at: at(day),
            day,
            host: "Mac".to_owned(),
            prev: Digest::GENESIS,
            event,
        }
    }

    fn enrolled(day: Date, name: &str, engram: EngramId, critical: bool) -> Record {
        line(
            day,
            Event::Enroll(Enrolled {
                slug: name.parse().unwrap(),
                engram,
                critical,
            }),
        )
    }

    fn drilled(day: Date, engram: EngramId, outcome: Outcome) -> Record {
        line(
            day,
            Event::Drill(Drilled {
                engram,
                outcome,
                ttfk_ms: Some(900),
                follow_ups: 0,
                recovered: false,
                aided: false,
            }),
        )
    }

    fn replay(records: &[Record]) -> Corpus {
        Corpus::replay(records.iter(), &Ladder::default())
    }

    fn slug(name: &str) -> crate::slug::Slug {
        name.parse().unwrap()
    }

    #[test]
    fn enrolment_opens_a_lineage_at_the_foot_of_the_ladder() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[enrolled(date(2026, 9, 10), "escrow-p", engram, true)]);
        let lineage = corpus.lineage(&slug("escrow-p")).unwrap();
        let dossier = lineage.current().unwrap();
        assert!(lineage.critical);
        assert_eq!(dossier.rung, Rung::FIRST);
        assert_eq!(dossier.anchor, date(2026, 9, 10));
        assert_eq!(dossier.last_exposed, date(2026, 9, 10));
        assert_eq!(lineage.ordinal(&engram), Some(1));
        assert_eq!(corpus.label(&engram), "escrow-p@1");
        assert_eq!(
            dossier.standing(date(2026, 9, 11), &Ladder::default()),
            Standing::Due,
            "a fresh engram is due the day after enrolment"
        );
        assert!(corpus.drills().is_empty());
    }

    #[test]
    fn a_pass_while_due_advances_the_rung_and_anchors() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 10), "a", engram, false),
            drilled(date(2026, 9, 11), engram, Outcome::Pass),
        ]);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.rung.get(), 1);
        assert_eq!(dossier.anchor, date(2026, 9, 11));
        assert_eq!(dossier.last_review, Some(date(2026, 9, 11)));
        let drill = corpus.drills().first().unwrap();
        assert_eq!(drill.occasion, Occasion::Review);
        assert_eq!(drill.scheduled_days, 1);
        assert_eq!(drill.since_anchor, 1);
        assert_eq!(drill.gap, 1);
    }

    #[test]
    fn a_pass_while_not_due_moves_nothing_but_the_exposure() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 10), "a", engram, false),
            drilled(date(2026, 9, 11), engram, Outcome::Pass),
            drilled(date(2026, 9, 11), engram, Outcome::Pass),
        ]);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.rung.get(), 1, "the second was a practice");
        assert_eq!(dossier.anchor, date(2026, 9, 11));
        assert_eq!(dossier.last_exposed, date(2026, 9, 11));
        let second = corpus.drills().get(1).unwrap();
        assert_eq!(second.occasion, Occasion::Practice);
        assert_eq!(
            second.gap, 0,
            "a second drill the same day is zero days cold"
        );
        assert_eq!(second.since_anchor, 0);
    }

    #[test]
    fn a_fail_on_any_day_returns_to_the_foot_and_anchors() {
        let engram = EngramId::mint().unwrap();
        let mut records = vec![
            enrolled(date(2026, 9, 1), "a", engram, false),
            drilled(date(2026, 9, 2), engram, Outcome::Pass),
            drilled(date(2026, 9, 3), engram, Outcome::Pass),
            drilled(date(2026, 9, 5), engram, Outcome::Pass),
        ];
        let climbed = replay(&records);
        let dossier = climbed.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.rung.get(), 3);
        assert_eq!(
            dossier.standing(date(2026, 9, 6), &Ladder::default()),
            Standing::Waiting {
                until: date(2026, 9, 9)
            }
        );

        // A fail on a day nobody asked.
        records.push(drilled(date(2026, 9, 6), engram, Outcome::Fail));
        let fallen = replay(&records);
        let dossier = fallen.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.rung, Rung::FIRST);
        assert_eq!(dossier.anchor, date(2026, 9, 6));
        assert_eq!(dossier.last_review, Some(date(2026, 9, 6)));
        let last = fallen.drills().last().unwrap();
        assert_eq!(last.occasion, Occasion::Practice, "nobody asked");
        assert_eq!(last.outcome, Outcome::Fail);
    }

    #[test]
    fn the_occasion_is_read_off_the_standing_on_the_day() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", engram, false),
            drilled(date(2026, 9, 2), engram, Outcome::Pass),
            // Rung 1 asks for one day; three days later is still due.
            drilled(date(2026, 9, 5), engram, Outcome::Pass),
            // Rung 2 asks for two days; the next morning is not.
            drilled(date(2026, 9, 6), engram, Outcome::Pass),
        ]);
        let occasions: Vec<Occasion> = corpus.drills().iter().map(|d| d.occasion).collect();
        assert_eq!(
            occasions,
            vec![Occasion::Review, Occasion::Review, Occasion::Practice]
        );
        let late = corpus.drills().get(1).unwrap();
        assert_eq!(late.scheduled_days, 1);
        assert_eq!(late.since_anchor, 3);
    }

    #[test]
    fn a_shorter_ladder_re_derives_the_history() {
        let engram = EngramId::mint().unwrap();
        let records = [
            enrolled(date(2026, 9, 1), "a", engram, false),
            drilled(date(2026, 9, 2), engram, Outcome::Pass),
            drilled(date(2026, 9, 3), engram, Outcome::Pass),
            drilled(date(2026, 9, 5), engram, Outcome::Pass),
        ];
        let long = replay(&records);
        assert_eq!(
            long.lineage(&slug("a"))
                .unwrap()
                .current()
                .unwrap()
                .rung
                .get(),
            3
        );

        let short = Ladder::new(vec![1, 2]).unwrap();
        let corpus = Corpus::replay(records.iter(), &short);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(
            dossier.rung,
            short.cap(),
            "the same drills climb a shorter ladder"
        );
        let occasions: Vec<Occasion> = corpus.drills().iter().map(|d| d.occasion).collect();
        assert_eq!(
            occasions,
            vec![Occasion::Review, Occasion::Practice, Occasion::Review],
            "under a two-day cap the second came a day early"
        );
    }

    #[test]
    fn a_rotation_supersedes_one_engram_and_starts_the_next_at_the_foot() {
        let first = EngramId::mint().unwrap();
        let second = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", first, false),
            drilled(date(2026, 9, 8), first, Outcome::Pass),
            line(
                date(2026, 9, 9),
                Event::Rotate(Rotated {
                    slug: slug("a"),
                    to: second,
                }),
            ),
        ]);
        let lineage = corpus.lineage(&slug("a")).unwrap();
        assert_eq!(lineage.engrams.len(), 2);
        assert_eq!(lineage.current().map(|d| d.engram), Some(second));
        assert_eq!(lineage.ordinal(&second), Some(2));
        assert_eq!(corpus.label(&second), "a@2");

        let outgoing = lineage.dossier(&first).unwrap();
        assert_eq!(outgoing.superseded, Some(date(2026, 9, 9)));
        assert_eq!(
            outgoing.rung.get(),
            1,
            "the superseded record keeps what it measured"
        );

        let incoming = lineage.dossier(&second).unwrap();
        assert_eq!(incoming.rung, Rung::FIRST);
        assert_eq!(incoming.minted, date(2026, 9, 9));
    }

    #[test]
    fn a_late_drill_against_a_superseded_engram_lands_on_its_own_record() {
        let first = EngramId::mint().unwrap();
        let second = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", first, false),
            line(
                date(2026, 9, 2),
                Event::Rotate(Rotated {
                    slug: slug("a"),
                    to: second,
                }),
            ),
            drilled(date(2026, 9, 3), first, Outcome::Pass),
        ]);
        let lineage = corpus.lineage(&slug("a")).unwrap();
        assert_eq!(lineage.dossier(&first).unwrap().rung.get(), 1);
        assert_eq!(
            lineage.dossier(&second).unwrap().rung,
            Rung::FIRST,
            "the new secret is not credited with the old one's rehearsal"
        );
    }

    #[test]
    fn an_attachment_exposes_the_secret_and_changes_nothing_else() {
        let engram = EngramId::mint().unwrap();
        let history = vec![
            enrolled(date(2026, 9, 1), "a", engram, false),
            drilled(date(2026, 9, 2), engram, Outcome::Pass),
        ];
        let before = replay(&history);
        let mut with = history;
        with.push(line(date(2026, 9, 20), Event::Attach(Attached { engram })));
        let after = replay(&with);

        let one = before.lineage(&slug("a")).unwrap().current().unwrap();
        let two = after.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(one.rung, two.rung);
        assert_eq!(one.anchor, two.anchor);
        assert_eq!(one.last_review, two.last_review);
        assert_eq!(
            two.last_exposed,
            date(2026, 9, 20),
            "the secret was typed, so the retention interval restarts"
        );
    }

    #[test]
    fn a_retired_lineage_keeps_its_history_and_leaves_the_schedule() {
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", EngramId::mint().unwrap(), false),
            enrolled(date(2026, 9, 1), "b", EngramId::mint().unwrap(), false),
            line(date(2026, 9, 2), Event::Retire(Retired { slug: slug("a") })),
        ]);
        assert!(corpus.lineage(&slug("a")).unwrap().retired);
        assert_eq!(corpus.lineages().count(), 2);
        assert_eq!(corpus.active().len(), 1);
    }

    #[test]
    fn the_roster_order_is_fixed_with_criticals_first() {
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "zeta", EngramId::mint().unwrap(), false),
            enrolled(date(2026, 9, 1), "alpha", EngramId::mint().unwrap(), false),
            enrolled(
                date(2026, 9, 1),
                "escrow-p",
                EngramId::mint().unwrap(),
                true,
            ),
        ]);
        let order: Vec<&str> = corpus
            .active()
            .iter()
            .map(|lineage| lineage.slug.as_str())
            .collect();
        assert_eq!(order, vec!["escrow-p", "alpha", "zeta"]);
    }

    #[test]
    fn a_drill_on_an_engram_nothing_knows_is_ignored_rather_than_inventing_one() {
        let corpus = replay(&[drilled(
            date(2026, 9, 1),
            EngramId::mint().unwrap(),
            Outcome::Pass,
        )]);
        assert_eq!(corpus.lineages().count(), 0);
        assert!(corpus.drills().is_empty());
    }

    #[test]
    fn a_day_that_goes_backwards_under_merge_never_walks_a_date_back() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", engram, false),
            drilled(date(2026, 9, 20), engram, Outcome::Pass),
            // A second machine in another zone, sorting later by instant while
            // naming an earlier civil day.
            drilled(date(2026, 9, 19), engram, Outcome::Fail),
        ]);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.anchor, date(2026, 9, 20));
        assert_eq!(dossier.last_exposed, date(2026, 9, 20));
        assert_eq!(dossier.last_review, Some(date(2026, 9, 20)));
        assert_eq!(
            dossier.rung,
            Rung::FIRST,
            "the rung still follows the last record applied"
        );
        assert_eq!(
            corpus.drills().last().unwrap().gap,
            0,
            "a backwards day is a gap of zero, never a wrap"
        );
    }

    #[test]
    fn replaying_a_duplicated_chain_opens_no_second_engram() {
        let engram = EngramId::mint().unwrap();
        let one = enrolled(date(2026, 9, 1), "a", engram, false);
        let corpus = replay(&[one.clone(), one]);
        assert_eq!(corpus.lineage(&slug("a")).unwrap().engrams.len(), 1);
    }

    #[test]
    fn a_lineage_holds_one_current_engram_whatever_a_merge_hands_replay() {
        let first = EngramId::mint().unwrap();
        let second = EngramId::mint().unwrap();
        let third = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", first, false),
            // A second machine enrolling over a live lineage.
            enrolled(date(2026, 9, 5), "a", second, true),
            // And rotating over whatever it held.
            line(
                date(2026, 9, 9),
                Event::Rotate(Rotated {
                    slug: slug("a"),
                    to: third,
                }),
            ),
        ]);
        let lineage = corpus.lineage(&slug("a")).unwrap();
        assert_eq!(lineage.engrams.len(), 3);
        assert_eq!(lineage.engrams.iter().filter(|d| d.is_current()).count(), 1);
        assert_eq!(lineage.current().map(|d| d.engram), Some(third));
        assert_eq!(
            lineage.dossier(&first).unwrap().superseded,
            Some(date(2026, 9, 5))
        );
        assert_eq!(
            lineage.dossier(&second).unwrap().superseded,
            Some(date(2026, 9, 9))
        );
        assert!(
            lineage.critical,
            "the latest enrolment says what the name is"
        );
    }
}
