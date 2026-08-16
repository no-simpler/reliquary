pub const TOPICS: &[(&str, &str)] = &[
    ("metadata", METADATA),
    ("retention", RETENTION),
    ("dedup", DEDUP),
];

pub const METADATA: &str = "\
METADATA

Strict, limited, normalized on entry.
Invalid metadata shown by midden doctor; needs repair.

  id            four characters, unique across the corpus and its archive
  kind          gap, conflict, stale, hunt, rebuff, friction or rework
  title         one line, at most 72 characters
  detail        optional, one line at most 200 characters
  target        optional, at most 120 characters: where the fix would land
  status        open, actioned or dismissed
  occurrences   how many sessions have filed this cause
  project       absolute path this was filed against
  cwd           where the filing session was working
  branch        optional, when the working directory was on one
  session       optional, the Claude Code session id
  created       RFC 3339, UTC
  updated       RFC 3339, UTC
  seen          the last ten sighting timestamps
  fingerprint   eight characters; the dedup key

The body is the evidence, at most 1200 bytes. It is preserved byte for byte
across every metadata rewrite.";

pub const DEDUP: &str = "\
DEDUP

fingerprint = kind + normalized target + normalized claim.

Normalization folds case, punctuation, filler words and a trailing separator,
so two sessions phrasing one cause differently land on one note. Project,
session and branch are deliberately excluded: one ambiguous directive met from
three repositories is one finding.

Filing a fingerprint that already exists bumps occurrences, appends to seen,
and refreshes updated. An actioned note reopens; a dismissed one stays
dismissed. Archived notes never match, so a retired cause that returns gets a
fresh note with a fresh date.

Changing kind, target or title with midden set recomputes the fingerprint.";

pub const RETENTION: &str = "\
RETENTION

Applied by midden gc, which runs from up. The archive is terminal and never
swept.

  dismissed, quiet 30 days     dropped
  actioned, quiet 90 days      dropped
  open, seen once, quiet 180 days   archived

Quiet is measured from updated, so a recurrence resets the clock.

Past 200 open notes midden doctor fails. The bound is reported, never
enforced — refusing a note would lose the observation. The answer is to drain
the corpus, not to raise the number.";

pub fn topic(name: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, body)| *body)
}

pub fn topic_names() -> String {
    TOPICS
        .iter()
        .map(|(key, _)| *key)
        .collect::<Vec<_>>()
        .join(", ")
}
