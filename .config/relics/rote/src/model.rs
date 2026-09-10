//! The projection: what the log adds up to.
//!
//! The log is the record; this is a reading of it, rebuilt on every invocation.
//! There is no second file holding schedule state, so state and log have no way
//! to disagree.
//!
//! Replay **reads** `step_after` rather than recomputing it. The step a record
//! carries is what the tool decided at the time, which makes history immune to a
//! later change in the ladder; `doctor` recomputes and reports a divergence
//! instead of silently rewriting the past.

use std::collections::BTreeMap;

use jiff::Timestamp;
use jiff::civil::Date;

use crate::ladder::{self, Class, Step};
use crate::log::{Attempted, Event, Line, Outcome, Record};
use crate::slug::Slug;

/// What the log says about one slug.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SlugState {
    /// The slug.
    pub slug: Slug,
    /// Which verifier generation is current.
    pub version: u32,
    /// Whether losing this one is unrecoverable rather than inconvenient.
    pub critical: bool,
    /// Whether it has left the schedule.
    pub retired: bool,
    /// Where it sits on the ladder.
    pub step: Step,
    /// The day the schedule counts from: the last review or probe, else
    /// enrollment.
    pub anchor: Date,
    /// The day it was enrolled, or last re-enrolled.
    pub added: Date,
    /// The last scheduled review.
    pub last_review: Option<Date>,
    /// The last entry of any kind that put the secret in front of a person.
    pub last_attempt: Option<Date>,
    /// When that was, to the second.
    pub last_attempt_at: Option<Timestamp>,
    /// A stretch horizon, while one is set.
    pub hold_until: Option<Date>,
    /// Consecutive first-attempt cold passes whose effective interval reached
    /// the cap. A pass below the cap leaves it alone — it is not evidence in
    /// either direction — and a failure clears it.
    pub cap_passes: u32,
    /// The day the slug first stood alone: a first-attempt pass with the answer
    /// nowhere in front of the person. The honest start of the memory, and the
    /// only reading that says anything while the step is still at the foot.
    pub first_unaided: Option<Date>,
    /// Whether the last aided entry was refused. Typing what the vault shows and
    /// being told wrong means the verifier and the item have diverged. Cleared
    /// by the next pass of any kind.
    pub aided_mismatch: bool,
}

impl SlugState {
    /// Where the slug stands against its own schedule today.
    pub fn standing(&self, today: Date) -> ladder::Standing {
        ladder::standing(today, self.anchor, self.step, self.hold_until)
    }

    /// The cutover gate for this slug.
    pub fn gate(&self) -> ladder::Gate {
        ladder::gate(self.step, self.cap_passes)
    }

    /// Days since the last entry of any kind.
    pub fn effective_interval(&self, today: Date) -> Option<u32> {
        self.last_attempt
            .map(|last| ladder::days_between(last, today))
    }

    /// Whether the secret has already been in front of a person today, whether
    /// through an entry or through enrollment.
    pub fn seen_today(&self, today: Date) -> bool {
        self.last_attempt == Some(today) || self.added == today
    }

    /// Whether the slug has been practiced since its last scheduled review.
    ///
    /// A warm slug cannot produce a cold entry at the cap interval, which is
    /// what the cutover gate is waiting for.
    pub fn warm(&self) -> bool {
        match (self.last_attempt, self.last_review) {
            (Some(attempt), Some(review)) => attempt > review,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }

    /// Days since the last scheduled review.
    pub fn actual_interval(&self, today: Date) -> Option<u32> {
        self.last_review
            .map(|last| ladder::days_between(last, today))
    }
}

/// Every slug the log knows about.
#[derive(Clone, Debug, Default)]
pub struct State {
    slugs: BTreeMap<Slug, SlugState>,
}

impl State {
    /// Rebuild from the log.
    pub fn replay<'a>(lines: impl IntoIterator<Item = &'a Line>) -> Self {
        let mut state = Self::default();
        for record in lines.into_iter().filter_map(Line::record) {
            state.apply(record);
        }
        state
    }

    /// One slug, if the log knows it.
    pub fn get(&self, slug: &Slug) -> Option<&SlugState> {
        self.slugs.get(slug)
    }

    /// Every slug, retired ones included, in name order.
    pub fn all(&self) -> impl Iterator<Item = &SlugState> {
        self.slugs.values()
    }

    /// The slugs still on the schedule, criticals first and then by name.
    ///
    /// The order is fixed rather than shuffled: a randomised order would trade
    /// away comparability between one day's timings and the next, which is the
    /// measurement the log exists for.
    pub fn scheduled(&self) -> Vec<&SlugState> {
        let mut out: Vec<&SlugState> = self.slugs.values().filter(|s| !s.retired).collect();
        out.sort_by(|a, b| {
            b.critical
                .cmp(&a.critical)
                .then_with(|| a.slug.cmp(&b.slug))
        });
        out
    }

    fn apply(&mut self, record: &Record) {
        match &record.event {
            Event::Add(event) => {
                self.slugs.insert(
                    event.slug.clone(),
                    SlugState {
                        slug: event.slug.clone(),
                        version: event.version,
                        critical: event.critical,
                        retired: false,
                        step: Step::FIRST,
                        anchor: record.day,
                        added: record.day,
                        last_review: None,
                        last_attempt: None,
                        last_attempt_at: None,
                        hold_until: None,
                        cap_passes: 0,
                        first_unaided: None,
                        aided_mismatch: false,
                    },
                );
            }
            Event::Rekey(event) => {
                if let Some(slug) = self.slugs.get_mut(&event.slug) {
                    // A new secret starts the ladder over. Otherwise the tool
                    // would keep crediting a retired one's rehearsal to it.
                    slug.version = event.version;
                    slug.step = Step::FIRST;
                    slug.anchor = record.day;
                    slug.added = record.day;
                    slug.last_review = None;
                    slug.last_attempt = None;
                    slug.last_attempt_at = None;
                    slug.hold_until = None;
                    slug.cap_passes = 0;
                    slug.first_unaided = None;
                    slug.aided_mismatch = false;
                    slug.retired = false;
                }
            }
            Event::Retire(event) => {
                if let Some(slug) = self.slugs.get_mut(&event.slug) {
                    slug.retired = true;
                }
            }
            Event::Probe(event) => {
                if let Some(slug) = self.slugs.get_mut(&event.slug) {
                    slug.hold_until = event.until;
                }
            }
            Event::Attempt(event) => {
                if let Some(slug) = self.slugs.get_mut(&event.slug) {
                    apply_attempt(slug, record, event);
                }
            }
        }
    }
}

/// What one entry does to a slug.
///
/// Split out of [`State::apply`] because it is the only arm with a shape of its
/// own: four classes and five outcomes, each answering a different question.
fn apply_attempt(slug: &mut SlugState, record: &Record, event: &Attempted) {
    match event.outcome {
        // Nothing was typed, so nothing was exposed: neither the
        // schedule nor the retention interval moves.
        Outcome::Skip | Outcome::Abort => return,
        Outcome::Pass | Outcome::Fail | Outcome::Blank => {}
    }
    slug.last_attempt = Some(record.day);
    slug.last_attempt_at = Some(record.at);
    match event.class {
        Class::Review => {
            slug.last_review = Some(record.day);
            slug.anchor = record.day;
        }
        Class::Probe => {
            // The horizon is spent, and the schedule counts from the
            // cold entry that ended it.
            slug.anchor = record.day;
            slug.hold_until = None;
        }
        Class::Practice | Class::Aided => {}
    }
    match (event.class, event.outcome) {
        (Class::Aided, Outcome::Pass) => slug.aided_mismatch = false,
        (Class::Aided, _) => slug.aided_mismatch = true,
        (_, Outcome::Pass) => slug.aided_mismatch = false,
        _ => {}
    }
    if event.class.scores() {
        slug.step = Step::from_recorded(event.step_after);
    }
    if event.attempt == 1 {
        if event.class.unaided() && event.outcome == Outcome::Pass && slug.first_unaided.is_none() {
            slug.first_unaided = Some(record.day);
        }
        match event.class {
            Class::Review | Class::Probe => match event.outcome {
                Outcome::Pass => {
                    let reached = event
                        .effective_interval_days
                        .is_some_and(|days| days >= ladder::cap_days());
                    if reached {
                        slug.cap_passes = slug.cap_passes.saturating_add(1);
                    }
                }
                Outcome::Fail | Outcome::Blank => slug.cap_passes = 0,
                Outcome::Skip | Outcome::Abort => {}
            },
            Class::Practice | Class::Aided => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use jiff::civil::{Date, date};

    use super::State;
    use crate::ladder::{self, Class, Gate, Standing, Step};
    use crate::log::{
        Added, Attempted, Digest, Event, Line, Outcome, Probed, Record, Rekeyed, Retired, SCHEMA,
        SessionId,
    };
    use crate::slug::Slug;

    fn slug(name: &str) -> Slug {
        name.parse().unwrap()
    }

    fn line(day: Date, event: Event) -> Line {
        Line::Parsed(Box::new(Record {
            v: SCHEMA,
            at: day.to_zoned(jiff::tz::TimeZone::UTC).unwrap().timestamp(),
            day,
            host: "Mac".to_owned(),
            prev: Digest::GENESIS,
            event,
        }))
    }

    fn added(day: Date, name: &str, critical: bool) -> Line {
        line(
            day,
            Event::Add(Added {
                slug: slug(name),
                version: 1,
                critical,
            }),
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "a fixture that names every field of the record it builds"
    )]
    fn attempt(
        day: Date,
        name: &str,
        class: Class,
        outcome: Outcome,
        attempt: u8,
        effective: Option<u32>,
        step_before: u8,
        step_after: u8,
    ) -> Line {
        line(
            day,
            Event::Attempt(Attempted {
                slug: slug(name),
                session: SessionId::from_bytes([0; 8]),
                version: 1,
                class,
                attempt,
                outcome,
                ttfk_ms: Some(900),
                total_ms: Some(3_000),
                corrections: 0,
                paste_refused: 0,
                scheduled_interval_days: 7,
                actual_interval_days: effective,
                effective_interval_days: effective,
                stretch: false,
                step_before,
                step_after,
            }),
        )
    }

    #[test]
    fn an_aided_entry_moves_nothing_but_the_retention_interval() {
        let state = State::replay(&[
            added(date(2026, 9, 1), "escrow-p", true),
            attempt(
                date(2026, 9, 8),
                "escrow-p",
                Class::Review,
                Outcome::Pass,
                1,
                Some(7),
                4,
                4,
            ),
            attempt(
                date(2026, 9, 9),
                "escrow-p",
                Class::Aided,
                Outcome::Pass,
                2,
                Some(1),
                4,
                4,
            ),
        ]);
        let slug = state.get(&slug("escrow-p")).unwrap();
        assert_eq!(slug.step, Step::cap());
        assert_eq!(
            slug.cap_passes, 1,
            "the aided entry neither adds evidence nor retracts it"
        );
        assert_eq!(
            slug.last_review,
            Some(date(2026, 9, 8)),
            "an aided entry is not a review"
        );
        assert_eq!(
            slug.last_attempt,
            Some(date(2026, 9, 9)),
            "but the answer was in front of a person, so the next entry is one-day evidence"
        );
    }

    #[test]
    fn standing_alone_is_the_first_cold_first_attempt_pass() {
        let state = State::replay(&[
            added(date(2026, 9, 1), "escrow-p", true),
            attempt(
                date(2026, 9, 2),
                "escrow-p",
                Class::Aided,
                Outcome::Pass,
                1,
                Some(1),
                0,
                0,
            ),
            attempt(
                date(2026, 9, 3),
                "escrow-p",
                Class::Review,
                Outcome::Pass,
                1,
                Some(1),
                0,
                1,
            ),
            attempt(
                date(2026, 9, 4),
                "escrow-p",
                Class::Review,
                Outcome::Pass,
                1,
                Some(1),
                1,
                2,
            ),
        ]);
        let slug = state.get(&slug("escrow-p")).unwrap();
        assert_eq!(slug.first_unaided, Some(date(2026, 9, 3)));
    }

    #[test]
    fn a_refused_aided_entry_stands_until_something_passes() {
        let mut lines = vec![
            added(date(2026, 9, 1), "escrow-p", true),
            attempt(
                date(2026, 9, 2),
                "escrow-p",
                Class::Aided,
                Outcome::Fail,
                2,
                Some(1),
                0,
                0,
            ),
        ];
        let state = State::replay(&lines);
        assert!(state.get(&slug("escrow-p")).unwrap().aided_mismatch);

        lines.push(attempt(
            date(2026, 9, 3),
            "escrow-p",
            Class::Review,
            Outcome::Pass,
            1,
            Some(1),
            0,
            1,
        ));
        let state = State::replay(&lines);
        assert!(!state.get(&slug("escrow-p")).unwrap().aided_mismatch);
    }

    #[test]
    fn a_blank_lapses_the_ladder_like_any_other_failure() {
        let state = State::replay(&[
            added(date(2026, 9, 1), "escrow-p", true),
            attempt(
                date(2026, 9, 8),
                "escrow-p",
                Class::Review,
                Outcome::Blank,
                1,
                Some(7),
                4,
                0,
            ),
        ]);
        let slug = state.get(&slug("escrow-p")).unwrap();
        assert_eq!(slug.step, Step::FIRST);
        assert_eq!(slug.cap_passes, 0);
        assert_eq!(slug.first_unaided, None);
    }

    #[test]
    fn enrolment_puts_a_slug_at_the_foot_of_the_ladder() {
        let state = State::replay(&[added(date(2026, 9, 10), "escrow-p", true)]);
        let slug = state.get(&slug("escrow-p")).unwrap();
        assert_eq!(slug.step, Step::FIRST);
        assert_eq!(slug.anchor, date(2026, 9, 10));
        assert!(slug.critical);
        assert_eq!(slug.last_attempt, None);
        assert_eq!(
            slug.standing(date(2026, 9, 11)),
            Standing::Due,
            "a fresh slug is due the day after enrollment"
        );
    }

    #[test]
    fn a_review_moves_the_step_and_the_anchor() {
        let state = State::replay(&[
            added(date(2026, 9, 10), "a", false),
            attempt(
                date(2026, 9, 11),
                "a",
                Class::Review,
                Outcome::Pass,
                1,
                Some(1),
                0,
                1,
            ),
        ]);
        let slug = state.get(&slug("a")).unwrap();
        assert_eq!(slug.step, Step::from_recorded(1));
        assert_eq!(slug.anchor, date(2026, 9, 11));
        assert_eq!(slug.last_review, Some(date(2026, 9, 11)));
    }

    #[test]
    fn practice_moves_the_retention_interval_and_nothing_else() {
        let state = State::replay(&[
            added(date(2026, 9, 10), "a", false),
            attempt(
                date(2026, 9, 11),
                "a",
                Class::Review,
                Outcome::Pass,
                1,
                Some(1),
                0,
                1,
            ),
            attempt(
                date(2026, 9, 12),
                "a",
                Class::Practice,
                Outcome::Pass,
                1,
                Some(1),
                1,
                1,
            ),
        ]);
        let slug = state.get(&slug("a")).unwrap();
        assert_eq!(slug.step, Step::from_recorded(1), "practice never scores");
        assert_eq!(slug.anchor, date(2026, 9, 11), "and never moves the anchor");
        assert_eq!(slug.last_attempt, Some(date(2026, 9, 12)));
        assert_eq!(slug.effective_interval(date(2026, 9, 13)), Some(1));
        assert_eq!(slug.actual_interval(date(2026, 9, 13)), Some(2));
    }

    #[test]
    fn a_slug_practised_since_its_last_review_reads_as_warm() {
        let cold = State::replay(&[
            added(date(2026, 9, 10), "a", false),
            attempt(
                date(2026, 9, 11),
                "a",
                Class::Review,
                Outcome::Pass,
                1,
                Some(1),
                0,
                1,
            ),
        ]);
        assert!(!cold.get(&slug("a")).unwrap().warm());

        let warm = State::replay(&[
            added(date(2026, 9, 10), "a", false),
            attempt(
                date(2026, 9, 11),
                "a",
                Class::Review,
                Outcome::Pass,
                1,
                Some(1),
                0,
                1,
            ),
            attempt(
                date(2026, 9, 12),
                "a",
                Class::Practice,
                Outcome::Pass,
                1,
                Some(1),
                1,
                1,
            ),
        ]);
        assert!(warm.get(&slug("a")).unwrap().warm());
    }

    #[test]
    fn a_skipped_prompt_exposes_nothing_and_so_moves_nothing() {
        let state = State::replay(&[
            added(date(2026, 9, 10), "a", false),
            attempt(
                date(2026, 9, 11),
                "a",
                Class::Review,
                Outcome::Skip,
                1,
                None,
                0,
                0,
            ),
        ]);
        let slug = state.get(&slug("a")).unwrap();
        assert_eq!(slug.last_attempt, None);
        assert_eq!(slug.anchor, date(2026, 9, 10));
    }

    #[test]
    fn the_gate_needs_cold_passes_at_the_cap_and_practice_cannot_buy_them() {
        let mut lines = vec![added(date(2026, 9, 1), "a", false)];
        let mut day = date(2026, 9, 8);
        for _ in 0..ladder::GATE_PASSES {
            lines.push(attempt(
                day,
                "a",
                Class::Review,
                Outcome::Pass,
                1,
                Some(7),
                4,
                4,
            ));
            day = day.checked_add(jiff::Span::new().days(7)).unwrap();
        }
        let state = State::replay(&lines);
        assert_eq!(
            state.get(&slug("a")).unwrap().gate(),
            Gate::Ready {
                passes: ladder::GATE_PASSES
            }
        );

        let mut warm = lines.clone();
        warm.push(attempt(
            day,
            "a",
            Class::Review,
            Outcome::Pass,
            1,
            Some(1),
            4,
            4,
        ));
        assert_eq!(
            State::replay(&warm).get(&slug("a")).unwrap().cap_passes,
            ladder::GATE_PASSES,
            "a pass at a one-day interval is not evidence in either direction"
        );
    }

    #[test]
    fn a_lapse_clears_the_run_of_cold_passes() {
        let state = State::replay(&[
            added(date(2026, 9, 1), "a", false),
            attempt(
                date(2026, 9, 8),
                "a",
                Class::Review,
                Outcome::Pass,
                1,
                Some(7),
                4,
                4,
            ),
            attempt(
                date(2026, 9, 15),
                "a",
                Class::Review,
                Outcome::Fail,
                1,
                Some(7),
                4,
                0,
            ),
        ]);
        let slug = state.get(&slug("a")).unwrap();
        assert_eq!(slug.cap_passes, 0);
        assert_eq!(slug.step, Step::FIRST);
        assert_eq!(slug.gate(), Gate::Below);
    }

    #[test]
    fn only_the_first_try_counts_toward_the_gate() {
        let state = State::replay(&[
            added(date(2026, 9, 1), "a", false),
            attempt(
                date(2026, 9, 8),
                "a",
                Class::Review,
                Outcome::Fail,
                1,
                Some(7),
                4,
                0,
            ),
            attempt(
                date(2026, 9, 8),
                "a",
                Class::Review,
                Outcome::Pass,
                2,
                Some(0),
                4,
                0,
            ),
        ]);
        assert_eq!(state.get(&slug("a")).unwrap().cap_passes, 0);
    }

    #[test]
    fn a_horizon_is_spent_by_the_probe_that_ends_it() {
        let state = State::replay(&[
            added(date(2026, 9, 1), "a", false),
            line(
                date(2026, 9, 2),
                Event::Probe(Probed {
                    slug: slug("a"),
                    until: Some(date(2026, 10, 17)),
                }),
            ),
            attempt(
                date(2026, 10, 17),
                "a",
                Class::Probe,
                Outcome::Pass,
                1,
                Some(45),
                4,
                4,
            ),
        ]);
        let slug = state.get(&slug("a")).unwrap();
        assert_eq!(slug.hold_until, None);
        assert_eq!(slug.anchor, date(2026, 10, 17));
        assert_eq!(slug.cap_passes, 1, "a long cold pass is gate evidence");
    }

    #[test]
    fn a_rotation_starts_the_ladder_over() {
        let state = State::replay(&[
            added(date(2026, 9, 1), "a", false),
            attempt(
                date(2026, 9, 8),
                "a",
                Class::Review,
                Outcome::Pass,
                1,
                Some(7),
                4,
                4,
            ),
            line(
                date(2026, 9, 9),
                Event::Rekey(Rekeyed {
                    slug: slug("a"),
                    version: 2,
                    proved: true,
                }),
            ),
        ]);
        let slug = state.get(&slug("a")).unwrap();
        assert_eq!(slug.version, 2);
        assert_eq!(slug.step, Step::FIRST);
        assert_eq!(slug.cap_passes, 0);
        assert_eq!(slug.last_attempt, None);
    }

    #[test]
    fn a_retired_slug_keeps_its_history_and_leaves_the_schedule() {
        let state = State::replay(&[
            added(date(2026, 9, 1), "a", false),
            added(date(2026, 9, 1), "b", false),
            line(date(2026, 9, 2), Event::Retire(Retired { slug: slug("a") })),
        ]);
        assert!(state.get(&slug("a")).unwrap().retired);
        assert_eq!(state.all().count(), 2);
        assert_eq!(state.scheduled().len(), 1);
    }

    #[test]
    fn the_session_order_is_fixed_with_criticals_first() {
        let state = State::replay(&[
            added(date(2026, 9, 1), "zeta", false),
            added(date(2026, 9, 1), "alpha", false),
            added(date(2026, 9, 1), "escrow-p", true),
        ]);
        let order: Vec<&str> = state.scheduled().iter().map(|s| s.slug.as_str()).collect();
        assert_eq!(order, vec!["escrow-p", "alpha", "zeta"]);
    }

    #[test]
    fn an_attempt_on_an_unknown_slug_is_ignored_rather_than_inventing_one() {
        let state = State::replay(&[attempt(
            date(2026, 9, 1),
            "ghost",
            Class::Review,
            Outcome::Pass,
            1,
            Some(1),
            0,
            1,
        )]);
        assert_eq!(state.all().count(), 0);
    }
}
