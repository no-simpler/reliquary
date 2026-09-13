//! The projection: what the record adds up to.
//!
//! The chains are the record; this is a reading of them, rebuilt on every
//! invocation. There is no second file holding schedule state, so state and
//! record have no way to disagree.
//!
//! **What replay reads and what it recomputes.** `rung_after` is read rather
//! than recomputed, so a later change to the ladder cannot rewrite the past.
//! Everything with a threshold in it — the streak above all — is computed in
//! `stats` from the merged corpus instead, which is both the right home for a
//! threshold and the only way to stay correct when two machines wrote without
//! having seen each other.
//!
//! **Every date here is monotone non-decreasing.** The merge orders by instant
//! while the schedule reads civil days, so a second machine in another zone can
//! hand replay a record whose day precedes its predecessor's. Assigning blindly
//! would walk an anchor backwards and make every interval after it nonsense.

pub mod chain;
pub mod drill;
pub mod record;

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;

use crate::ladder::{self, Ladder, Rung, Standing};
use crate::slug::Slug;
use record::{Captured, EngramId, Event, Outcome, Record};

/// What the record says about one enrolled secret.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Dossier {
    /// Which secret this is the record of.
    pub engram: EngramId,
    /// The day it was enrolled or rotated in.
    pub minted: Date,
    /// The day it stepped down, if it has. `None` means it is the current one.
    pub superseded: Option<Date>,
    /// Whether the rotation that minted it proved the outgoing secret first.
    /// `None` for the engram that opened the lineage.
    pub proved: Option<bool>,
    /// Where it sits on the ladder.
    pub rung: Rung,
    /// The day the schedule counts from: the last review, else the mint.
    pub anchor: Date,
    /// The last day the secret was in front of a person by any route — a
    /// capture, an enrolment, a rotation, an attachment.
    pub last_exposed: Date,
    /// When that was, to the second.
    pub last_exposed_at: Timestamp,
    /// The last day the schedule asked and got an answer.
    pub last_review: Option<Date>,
    /// The day it first stood alone: a first sample passed with the answer
    /// nowhere in front of the person. The honest start of the memory, and the
    /// only reading that says anything while the rung is still at the foot.
    pub first_unaided: Option<Date>,
    /// Whether the last aided sample was refused. Typing what the vault shows
    /// and being told wrong means the vault and the verifier have parted. A
    /// blank aided sample says nothing either way, and any pass clears it.
    pub aided_mismatch: bool,
}

impl Dossier {
    fn minted(engram: EngramId, day: Date, at: Timestamp, proved: Option<bool>) -> Self {
        Self {
            engram,
            minted: day,
            superseded: None,
            proved,
            rung: Rung::FIRST,
            anchor: day,
            last_exposed: day,
            last_exposed_at: at,
            last_review: None,
            first_unaided: None,
            aided_mismatch: false,
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

    /// Days since the secret was last in front of a person. The honest
    /// retention interval.
    pub fn effective_interval(&self, today: Date) -> u32 {
        ladder::days_between(self.last_exposed, today)
    }

    /// Days since the schedule's anchor.
    pub fn actual_interval(&self, today: Date) -> u32 {
        ladder::days_between(self.anchor, today)
    }

    /// Whether the secret has already been in front of a person today.
    pub fn seen_today(&self, today: Date) -> bool {
        self.last_exposed >= today
    }

    fn expose(&mut self, day: Date, at: Timestamp) {
        self.last_exposed = self.last_exposed.max(day);
        self.last_exposed_at = self.last_exposed_at.max(at);
    }
}

/// One rotation, read off a lineage.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cutover {
    /// The engram that stepped down.
    pub from: EngramId,
    /// The engram that took over.
    pub to: EngramId,
    /// When.
    pub day: Date,
    /// Whether the outgoing secret was proved first.
    pub proved: bool,
    /// How many days the outgoing engram held the lineage.
    pub held_days: u32,
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

    /// Every rotation this lineage has been through.
    pub fn cutovers(&self) -> Vec<Cutover> {
        let mut out = Vec::new();
        for pair in self.engrams.windows(2) {
            let (Some(from), Some(to)) = (pair.first(), pair.last()) else {
                continue;
            };
            out.push(Cutover {
                from: from.engram,
                to: to.engram,
                day: to.minted,
                proved: to.proved.unwrap_or(false),
                held_days: ladder::days_between(from.minted, to.minted),
            });
        }
        out
    }
}

/// Every lineage the record knows about.
#[derive(Clone, Debug, Default)]
pub struct Corpus {
    lineages: BTreeMap<Slug, Lineage>,
    by_engram: BTreeMap<EngramId, Slug>,
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
                lineage.retired = false;
                lineage
                    .engrams
                    .push(Dossier::minted(event.engram, record.day, record.at, None));
                self.by_engram.insert(event.engram, event.slug.clone());
            }
            Event::Rotate(event) => {
                let Some(lineage) = self.lineages.get_mut(&event.slug) else {
                    return;
                };
                if let Some(outgoing) = lineage.dossier_mut(&event.from) {
                    outgoing.superseded.get_or_insert(record.day);
                }
                if lineage.holds(&event.to) {
                    return;
                }
                lineage.retired = false;
                lineage.engrams.push(Dossier::minted(
                    event.to,
                    record.day,
                    record.at,
                    Some(event.proved),
                ));
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
            Event::Capture(event) => {
                if let Some(dossier) = self.dossier_mut(&event.engram) {
                    apply_capture(dossier, record, event, ladder);
                }
            }
        }
    }

    fn dossier_mut(&mut self, engram: &EngramId) -> Option<&mut Dossier> {
        let slug = self.by_engram.get(engram)?.clone();
        self.lineages.get_mut(&slug)?.dossier_mut(engram)
    }
}

/// What one sample does to an engram.
///
/// **The occasion decides whether the schedule was served; aided decides
/// whether the memory was measured.** Those are separate effects on separate
/// fields, which is the whole reason the two are separate fields on the wire.
fn apply_capture(dossier: &mut Dossier, record: &Record, event: &Captured, ladder: &Ladder) {
    if !event.outcome.exposed() {
        // Nothing was typed, so nothing was exposed: neither the schedule nor
        // the retention interval moves.
        return;
    }
    dossier.expose(record.day, record.at);

    if event.occasion.serves_the_schedule() {
        // Even aided. The schedule asked and got an answer, so asking again the
        // same day would be nagging; the rung stays where it was, so the next
        // ask lands at the same interval it would have.
        dossier.anchor = dossier.anchor.max(record.day);
        dossier.last_review = Some(match dossier.last_review {
            Some(previous) => previous.max(record.day),
            None => record.day,
        });
    }

    match (event.aided, event.outcome) {
        // The answer was in front of the person and the verifier refused it:
        // the one signal that the vault and the verifier have parted. A blank
        // aided sample offered nothing, so it says nothing.
        (true, Outcome::Fail) => dossier.aided_mismatch = true,
        (true | false, Outcome::Pass) => dossier.aided_mismatch = false,
        (false, Outcome::Fail)
        | (true | false, Outcome::Blank | Outcome::Skip | Outcome::Abort) => {}
    }

    if event.scores() {
        dossier.rung = ladder.recorded(event.rung_after);
    }

    if event.ordinal == 1
        && !event.aided
        && event.outcome == Outcome::Pass
        && dossier.first_unaided.is_none()
    {
        dossier.first_unaided = Some(record.day);
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::{Corpus, record::*};
    use crate::ladder::{Ladder, Occasion, Rung, Standing};
    use crate::machine::MachineId;

    fn at(day: Date) -> jiff::Timestamp {
        day.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp()
    }

    fn line(day: Date, event: Event) -> Record {
        Record {
            v: SCHEMA,
            at: at(day),
            day,
            machine: MachineId::of("test"),
            host: "Mac".to_owned(),
            seq: 0,
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

    fn captured(
        day: Date,
        name: &str,
        engram: EngramId,
        occasion: Occasion,
        aided: bool,
        outcome: Outcome,
        rung_after: u8,
    ) -> Record {
        line(
            day,
            Event::Capture(Captured {
                slug: name.parse().unwrap(),
                engram,
                sitting: SittingId::mint().unwrap(),
                ordinal: 1,
                occasion,
                aided,
                outcome,
                ttfk_ms: Some(900),
                total_ms: Some(3_000),
                corrections: 0,
                paste_refused: 0,
                scheduled_interval_days: 7,
                actual_interval_days: 7,
                effective_interval_days: 7,
                rung_before: 0,
                rung_after,
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
    }

    #[test]
    fn an_unaided_review_moves_both_the_rung_and_the_anchor() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 10), "a", engram, false),
            captured(
                date(2026, 9, 11),
                "a",
                engram,
                Occasion::Review,
                false,
                Outcome::Pass,
                1,
            ),
        ]);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.rung.get(), 1);
        assert_eq!(dossier.anchor, date(2026, 9, 11));
        assert_eq!(dossier.last_review, Some(date(2026, 9, 11)));
        assert_eq!(dossier.first_unaided, Some(date(2026, 9, 11)));
    }

    #[test]
    fn an_aided_review_moves_the_anchor_and_withholds_the_rung() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 10), "a", engram, false),
            captured(
                date(2026, 9, 11),
                "a",
                engram,
                Occasion::Review,
                true,
                Outcome::Pass,
                0,
            ),
        ]);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.rung, Rung::FIRST, "nothing was measured");
        assert_eq!(
            dossier.anchor,
            date(2026, 9, 11),
            "but the schedule was served, so it is not asked again today"
        );
        assert_eq!(dossier.first_unaided, None);
        assert_eq!(
            dossier.standing(date(2026, 9, 11), &Ladder::default()),
            Standing::Waiting {
                until: date(2026, 9, 12)
            }
        );
    }

    #[test]
    fn practice_moves_the_retention_interval_and_nothing_else() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 10), "a", engram, false),
            captured(
                date(2026, 9, 11),
                "a",
                engram,
                Occasion::Review,
                false,
                Outcome::Pass,
                1,
            ),
            captured(
                date(2026, 9, 12),
                "a",
                engram,
                Occasion::Practice,
                false,
                Outcome::Pass,
                1,
            ),
        ]);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.rung.get(), 1, "practice never measures");
        assert_eq!(
            dossier.anchor,
            date(2026, 9, 11),
            "and never serves the schedule"
        );
        assert_eq!(dossier.last_exposed, date(2026, 9, 12));
        assert_eq!(dossier.effective_interval(date(2026, 9, 13)), 1);
        assert_eq!(dossier.actual_interval(date(2026, 9, 13)), 2);
    }

    #[test]
    fn a_blank_lapses_the_ladder_like_any_other_failure() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", engram, false),
            captured(
                date(2026, 9, 8),
                "a",
                engram,
                Occasion::Review,
                false,
                Outcome::Blank,
                0,
            ),
        ]);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.rung, Rung::FIRST);
        assert_eq!(dossier.first_unaided, None);
    }

    #[test]
    fn a_skipped_prompt_exposes_nothing_and_so_moves_nothing() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 10), "a", engram, false),
            captured(
                date(2026, 9, 11),
                "a",
                engram,
                Occasion::Review,
                false,
                Outcome::Skip,
                0,
            ),
        ]);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.last_exposed, date(2026, 9, 10));
        assert_eq!(dossier.anchor, date(2026, 9, 10));
        assert_eq!(
            dossier.standing(date(2026, 9, 11), &Ladder::default()),
            Standing::Due,
            "a skipped drill is still due"
        );
    }

    #[test]
    fn a_rotation_supersedes_one_engram_and_starts_the_next_at_the_foot() {
        let first = EngramId::mint().unwrap();
        let second = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", first, false),
            captured(
                date(2026, 9, 8),
                "a",
                first,
                Occasion::Review,
                false,
                Outcome::Pass,
                1,
            ),
            line(
                date(2026, 9, 9),
                Event::Rotate(Rotated {
                    slug: slug("a"),
                    from: first,
                    to: second,
                    proved: true,
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
        assert_eq!(incoming.proved, Some(true));

        let cutovers = lineage.cutovers();
        assert_eq!(cutovers.len(), 1);
        assert_eq!(cutovers.first().map(|c| c.held_days), Some(8));
    }

    #[test]
    fn a_late_capture_against_a_superseded_engram_lands_on_its_own_record() {
        let first = EngramId::mint().unwrap();
        let second = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", first, false),
            line(
                date(2026, 9, 2),
                Event::Rotate(Rotated {
                    slug: slug("a"),
                    from: first,
                    to: second,
                    proved: true,
                }),
            ),
            captured(
                date(2026, 9, 3),
                "a",
                first,
                Occasion::Review,
                false,
                Outcome::Pass,
                1,
            ),
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
            captured(
                date(2026, 9, 2),
                "a",
                engram,
                Occasion::Review,
                false,
                Outcome::Pass,
                1,
            ),
        ];
        let before = replay(&history);
        let mut with = history;
        with.push(line(
            date(2026, 9, 20),
            Event::Attach(Attached {
                slug: slug("a"),
                engram,
            }),
        ));
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
    fn a_capture_on_an_engram_nothing_knows_is_ignored_rather_than_inventing_one() {
        let corpus = replay(&[captured(
            date(2026, 9, 1),
            "ghost",
            EngramId::mint().unwrap(),
            Occasion::Review,
            false,
            Outcome::Pass,
            1,
        )]);
        assert_eq!(corpus.lineages().count(), 0);
    }

    #[test]
    fn a_day_that_goes_backwards_under_merge_never_walks_a_date_back() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", engram, false),
            captured(
                date(2026, 9, 20),
                "a",
                engram,
                Occasion::Review,
                false,
                Outcome::Pass,
                1,
            ),
            // A second machine in another zone, sorting later by instant while
            // naming an earlier civil day.
            captured(
                date(2026, 9, 19),
                "a",
                engram,
                Occasion::Review,
                false,
                Outcome::Pass,
                2,
            ),
        ]);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.anchor, date(2026, 9, 20));
        assert_eq!(dossier.last_exposed, date(2026, 9, 20));
        assert_eq!(dossier.last_review, Some(date(2026, 9, 20)));
        assert_eq!(
            dossier.rung.get(),
            2,
            "the rung still follows the last record applied"
        );
    }

    #[test]
    fn a_rung_recorded_past_a_shortened_ladder_clamps_rather_than_addressing_nothing() {
        let engram = EngramId::mint().unwrap();
        let records = [
            enrolled(date(2026, 9, 1), "a", engram, false),
            captured(
                date(2026, 9, 8),
                "a",
                engram,
                Occasion::Review,
                false,
                Outcome::Pass,
                6,
            ),
        ];
        let short = Ladder::new(vec![1, 2]).unwrap();
        let corpus = Corpus::replay(records.iter(), &short);
        let dossier = corpus.lineage(&slug("a")).unwrap().current().unwrap();
        assert_eq!(dossier.rung, short.cap());
    }

    #[test]
    fn a_refused_aided_capture_stands_until_something_passes() {
        let engram = EngramId::mint().unwrap();
        let mut records = vec![
            enrolled(date(2026, 9, 1), "a", engram, false),
            captured(
                date(2026, 9, 2),
                "a",
                engram,
                Occasion::Review,
                true,
                Outcome::Fail,
                0,
            ),
        ];
        assert!(
            replay(&records)
                .lineage(&slug("a"))
                .unwrap()
                .current()
                .unwrap()
                .aided_mismatch
        );

        records.push(captured(
            date(2026, 9, 3),
            "a",
            engram,
            Occasion::Review,
            false,
            Outcome::Pass,
            1,
        ));
        assert!(
            !replay(&records)
                .lineage(&slug("a"))
                .unwrap()
                .current()
                .unwrap()
                .aided_mismatch
        );
    }

    #[test]
    fn a_blank_aided_capture_is_not_a_disagreement() {
        let engram = EngramId::mint().unwrap();
        let corpus = replay(&[
            enrolled(date(2026, 9, 1), "a", engram, false),
            captured(
                date(2026, 9, 2),
                "a",
                engram,
                Occasion::Review,
                true,
                Outcome::Blank,
                0,
            ),
        ]);
        assert!(
            !corpus
                .lineage(&slug("a"))
                .unwrap()
                .current()
                .unwrap()
                .aided_mismatch
        );
    }

    #[test]
    fn replaying_a_duplicated_chain_opens_no_second_engram() {
        let engram = EngramId::mint().unwrap();
        let one = enrolled(date(2026, 9, 1), "a", engram, false);
        let corpus = replay(&[one.clone(), one]);
        assert_eq!(corpus.lineage(&slug("a")).unwrap().engrams.len(), 1);
    }
}
