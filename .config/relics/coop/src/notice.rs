//! What is outstanding, and the identity that lets it be recognised again.

use relic_core::finding::{Finding, Severity};

/// One thing wanting attention.
///
/// The payload is a [`Finding`] and nothing else, so a relic that already
/// answers `doctor --format json` is already a well-formed producer. `source` is
/// the declared id, which owns the display: a producer is free to namespace its
/// own stations, and the card is not the place to learn that vocabulary.
#[derive(Clone, Debug)]
pub struct Notice {
    /// The declaration this came from.
    pub source: String,
    /// What it found.
    pub finding: Finding,
}

impl Notice {
    /// The stable identity of this notice, independent of when it was seen.
    ///
    /// Content-addressed on purpose: a notice whose wording changes is a
    /// different notice, which is what makes a future dismissal expire the
    /// moment the underlying thing moves.
    pub fn identity(&self) -> String {
        let fix = self.finding.fix.as_ref().map_or("", |fix| fix.as_str());
        hex(fnv1a(&[
            self.source.as_str(),
            self.finding.severity.to_string().as_str(),
            self.finding.summary.as_str(),
            fix,
        ]))
    }

    /// Whether the fix hint says anything the source id does not.
    ///
    /// `up` fixed by running `up` needs no second column.
    pub fn distinct_fix(&self) -> Option<&str> {
        let fix = self.finding.fix.as_ref()?.as_str();
        (fix != self.source).then_some(fix)
    }
}

/// One value standing for the whole outstanding set.
///
/// The card is edge-triggered on this: equal digest, nothing to say. Order is
/// imposed before hashing, so two shells that read the same state agree
/// regardless of how the directory happened to enumerate.
pub fn digest(notices: &[Notice]) -> String {
    let mut ids: Vec<String> = notices.iter().map(Notice::identity).collect();
    ids.sort_unstable();
    let parts: Vec<&str> = ids.iter().map(String::as_str).collect();
    hex(fnv1a(&parts))
}

/// Notices worth drawing, in a stable order.
///
/// [`Severity::Note`] is dropped rather than filtered by a configurable floor.
/// relic-core defines a note as "read it, do not grade on it", which is the
/// opposite of a thing you can act on and be rid of — and an inbox that accepts
/// what cannot be dismissed fills up with furniture.
pub fn actionable(mut notices: Vec<Notice>) -> Vec<Notice> {
    notices.retain(|notice| notice.finding.severity != Severity::Note);
    notices.sort_by(|left, right| {
        right
            .finding
            .severity
            .cmp(&left.finding.severity)
            .then_with(|| left.source.cmp(&right.source))
            .then_with(|| {
                left.finding
                    .summary
                    .as_str()
                    .cmp(right.finding.summary.as_str())
            })
    });
    notices
}

/// FNV-1a, 64-bit, over the fields joined by a separator that cannot occur in
/// them.
///
/// Hand-rolled rather than `DefaultHasher`, whose output std does not promise to
/// keep stable across releases — and this value is written to disk, where a
/// silent change would re-announce every notice on the machine after a toolchain
/// bump.
fn fnv1a(parts: &[&str]) -> u64 {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0000_0100_0000_01b3;

    let mut hash = OFFSET;
    for (index, part) in parts.iter().enumerate() {
        if index > 0 {
            hash = (hash ^ 0x1f).wrapping_mul(PRIME);
        }
        for byte in part.as_bytes() {
            hash = (hash ^ u64::from(*byte)).wrapping_mul(PRIME);
        }
    }
    hash
}

fn hex(value: u64) -> String {
    format!("{value:016x}")
}

#[cfg(test)]
mod tests {
    use super::{Notice, actionable, digest};
    use relic_core::finding::{FixHint, Severity, StationId, Summary};

    fn notice(source: &str, severity: Severity, summary: &str, fix: Option<&str>) -> Notice {
        let station = StationId::from_static("test");
        let mut finding = station.finds(severity, Summary::lossy(summary));
        if let Some(fix) = fix {
            finding = finding.fixed_by(FixHint::lossy(fix));
        }
        Notice {
            source: source.to_owned(),
            finding,
        }
    }

    fn soft(source: &str, summary: &str) -> Notice {
        notice(source, Severity::Soft, summary, None)
    }

    #[test]
    fn the_digest_ignores_the_order_it_was_handed() {
        let one = soft("up", "a");
        let two = soft("rote", "b");
        assert_eq!(
            digest(&[one.clone(), two.clone()]),
            digest(&[two, one]),
            "two shells must agree without agreeing on directory order"
        );
    }

    #[test]
    fn the_digest_moves_when_the_wording_moves() {
        assert_ne!(digest(&[soft("up", "3d")]), digest(&[soft("up", "4d")]));
    }

    #[test]
    fn the_digest_moves_when_the_severity_moves() {
        assert_ne!(
            digest(&[soft("up", "a")]),
            digest(&[notice("up", Severity::Broken, "a", None)])
        );
    }

    #[test]
    fn an_empty_coop_has_a_digest_of_its_own() {
        assert_ne!(digest(&[]), digest(&[soft("up", "a")]));
    }

    #[test]
    fn the_separator_keeps_adjacent_fields_from_running_together() {
        assert_ne!(
            digest(&[soft("ab", "c")]),
            digest(&[soft("a", "bc")]),
            "concatenation without a separator would collide these"
        );
    }

    #[test]
    fn a_note_is_never_drawn() {
        let kept = actionable(vec![
            notice("x", Severity::Note, "informational", None),
            soft("up", "act on me"),
        ]);
        assert_eq!(kept.len(), 1);
        assert_eq!(kept.first().unwrap().source, "up");
    }

    #[test]
    fn the_worse_severity_is_drawn_first() {
        let kept = actionable(vec![
            soft("aaa", "soft"),
            notice("zzz", Severity::Broken, "broken", None),
        ]);
        assert_eq!(kept.first().unwrap().source, "zzz");
    }

    #[test]
    fn a_fix_that_only_repeats_the_source_is_not_a_second_column() {
        assert_eq!(
            notice("up", Severity::Soft, "s", Some("up")).distinct_fix(),
            None
        );
        assert_eq!(
            notice("rote", Severity::Soft, "s", Some("rote drill")).distinct_fix(),
            Some("rote drill")
        );
    }
}
