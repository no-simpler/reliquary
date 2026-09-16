//! Properties the corpus holds whatever order it was written in.
//!
//! Per-machine chains make replay a merge of several ordered streams, which is
//! the shape property testing earns its keep on: the interesting failures are
//! interleavings nobody would think to write down.

mod support;

use proptest::prelude::*;
use rote::corpus::Corpus;
use rote::corpus::record::{Event, Outcome};
use rote::ladder::{Ladder, Rung, Standing};
use support::{Seeded, World, merged, shape, steps};

proptest! {
    /// Whatever order the records arrive in, the corpus is the same one.
    #[test]
    fn the_merge_is_order_insensitive(script in steps(), rotations in 0usize..5) {
        let records = World::build(&script);
        let ladder = Ladder::default();
        let first = Corpus::replay(merged(&records), &ladder);

        let mut shuffled = records.clone();
        let count = shuffled.len();
        if count > 0 {
            shuffled.rotate_left(rotations % count);
        }
        shuffled.reverse();
        let second = Corpus::replay(merged(&shuffled), &ladder);

        prop_assert_eq!(shape(&first), shape(&second));
    }

    /// No two records share a merge key, so the order is total rather than
    /// merely mostly-total. The key is nothing a record carries: the file names
    /// the machine and the line index is the position.
    #[test]
    fn the_merge_key_separates_every_record(script in steps()) {
        let records = World::build(&script);
        let mut keys: Vec<_> = records.iter().map(Seeded::key).collect();
        let before = keys.len();
        keys.sort();
        keys.dedup();
        prop_assert_eq!(keys.len(), before);
    }

    /// An attachment says a machine has a verifier. It says nothing about a
    /// memory, and the corpus must read it that way.
    #[test]
    fn attachments_move_no_rung_and_no_anchor(script in steps()) {
        let records = World::build(&script);
        let ladder = Ladder::default();
        let with = Corpus::replay(merged(&records), &ladder);

        let without: Vec<Seeded> = records
            .iter()
            .filter(|seeded| !matches!(seeded.record.event, Event::Attach(_)))
            .cloned()
            .collect();
        let without = Corpus::replay(merged(&without), &ladder);

        let rungs = |corpus: &Corpus| -> Vec<(String, u8, String)> {
            corpus
                .lineages()
                .flat_map(|lineage| {
                    lineage.engrams.iter().map(|dossier| {
                        (
                            lineage.slug.to_string(),
                            dossier.rung.get(),
                            dossier.anchor.to_string(),
                        )
                    })
                })
                .collect()
        };
        prop_assert_eq!(rungs(&with), rungs(&without));
    }

    /// Ordinals run from one with no gaps, whatever the interleaving, because
    /// they are a position in a list rather than a number anybody allocates.
    #[test]
    fn ordinals_run_from_one_without_gaps(script in steps()) {
        let records = World::build(&script);
        let corpus = Corpus::replay(merged(&records), &Ladder::default());
        for lineage in corpus.lineages() {
            for (index, dossier) in lineage.engrams.iter().enumerate() {
                prop_assert_eq!(
                    lineage.ordinal(&dossier.engram),
                    Some(index.saturating_add(1))
                );
            }
            prop_assert!(lineage.engrams.iter().filter(|d| d.is_current()).count() <= 1);
        }
    }

    /// Every date in a dossier only ever moves forward, so a second machine in
    /// another timezone cannot walk an anchor backwards.
    #[test]
    fn no_date_ever_walks_backwards(script in steps()) {
        let records = World::build(&script);
        let corpus = Corpus::replay(merged(&records), &Ladder::default());
        for lineage in corpus.lineages() {
            for dossier in &lineage.engrams {
                prop_assert!(dossier.anchor >= dossier.minted);
                prop_assert!(dossier.last_exposed >= dossier.minted);
                if let Some(review) = dossier.last_review {
                    prop_assert!(dossier.anchor >= review);
                }
            }
        }
    }

    /// A fail returns an engram to the foot on any day, asked for or not, and
    /// anchors the schedule there.
    #[test]
    fn a_fail_on_any_day_returns_to_the_foot(script in steps()) {
        let records = World::build(&script);
        let ladder = Ladder::default();
        let ordered = merged(&records);
        for (index, record) in ordered.iter().enumerate() {
            let Event::Drill(drilled) = &record.event else {
                continue;
            };
            if drilled.outcome != Outcome::Fail {
                continue;
            }
            let after = Corpus::replay(ordered.iter().take(index + 1).copied(), &ladder);
            let Some(dossier) = after
                .owner(&drilled.engram)
                .and_then(|lineage| lineage.dossier(&drilled.engram))
            else {
                continue;
            };
            prop_assert_eq!(dossier.rung, Rung::FIRST);
            prop_assert!(dossier.anchor >= record.day);
            prop_assert!(dossier.last_review.is_some_and(|day| day >= record.day));
        }
    }

    /// A pass on a day the schedule did not ask moves neither the rung nor
    /// the anchor; only the exposure moves.
    #[test]
    fn a_pass_while_not_due_moves_nothing(script in steps()) {
        let records = World::build(&script);
        let ladder = Ladder::default();
        let ordered = merged(&records);
        for (index, record) in ordered.iter().enumerate() {
            let Event::Drill(drilled) = &record.event else {
                continue;
            };
            if drilled.outcome != Outcome::Pass {
                continue;
            }
            let before = Corpus::replay(ordered.iter().take(index).copied(), &ladder);
            let Some(was) = before
                .owner(&drilled.engram)
                .and_then(|lineage| lineage.dossier(&drilled.engram))
                .cloned()
            else {
                continue;
            };
            if was.standing(record.day, &ladder) == Standing::Due {
                continue;
            }
            let after = Corpus::replay(ordered.iter().take(index + 1).copied(), &ladder);
            let now = after
                .owner(&drilled.engram)
                .and_then(|lineage| lineage.dossier(&drilled.engram))
                .cloned()
                .unwrap();
            prop_assert_eq!(now.rung, was.rung);
            prop_assert_eq!(now.anchor, was.anchor);
            prop_assert_eq!(now.last_review, was.last_review);
            prop_assert!(now.last_exposed >= was.last_exposed);
        }
    }
}
