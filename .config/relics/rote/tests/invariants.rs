//! Properties the corpus holds whatever order it was written in.
//!
//! Per-machine chains make replay a merge of several ordered streams, which is
//! the shape property testing earns its keep on: the interesting failures are
//! interleavings nobody would think to write down.

mod support;

use proptest::prelude::*;
use rote::corpus::Corpus;
use rote::corpus::record::{Event, Record};
use rote::ladder::Ladder;
use support::{World, merged, shape, steps};

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
    /// merely mostly-total.
    #[test]
    fn the_merge_key_separates_every_record(script in steps()) {
        let records = World::build(&script);
        let mut keys: Vec<_> = records.iter().map(rote::corpus::record::Record::order).collect();
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

        let without: Vec<Record> = records
            .iter()
            .filter(|record| !matches!(record.event, Event::Attach(_)))
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
}
