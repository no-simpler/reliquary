pub const TOPICS: &[(&str, &str)] = &[("ladder", LADDER), ("metadata", METADATA)];

pub const LADDER: &str = "\
LADDER

handoff -> relay -> spec

Promotion forward only; metadata strictly additive.
Handoff can promote directly to spec.";

pub const METADATA: &str = "\
METADATA

Strict, limited, for housekeeping and human review.
Normalized on entry.
Invalid metadata shown by docket doctor; needs repair.

every item:

  id            four characters, unique across every project on this machine
  kind          handoff, relay or spec
  name          up to three words of A-Z, 0-9 and underscore, at most 20
                characters; resolves an item wherever an id does
  tagline       one line, at most 80 characters
  project       absolute path this item belongs to
  created       RFC 3339, UTC
  updated       RFC 3339, UTC
  order         sparse; docket reorder owns it
  blocked       optional, one line at most 80: present means blocked, and why
  origin        optional: the project this was written from, when they differ
  tags          optional, single tokens of at most 32 characters

relay adds:

  chain         stable across every hop of one chain
  hop           1 for the first
  supersedes    the item this one was minted from, absent on the first hop

spec adds:

  stage         design or implementation";

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
