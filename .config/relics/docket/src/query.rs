//! What a listing asks the depot for: which items answer the flags, in which
//! order, and — across projects — which project comes first.
//!
//! The scan is linear and nothing is indexed. A depot holds a handful of items
//! per project, and an index would be a second copy of the truth that every
//! write has to keep honest.

use camino::Utf8Path;

use anyhow::{Result, bail};

use crate::cli::ListArgs;
use crate::field;
use crate::item::Kind;
use crate::store::{self, Depot, Record};

/// One item that answered, with what a listing needs in order to show it.
pub struct Hit {
    pub record: Record,
    /// Its place on its own project's docket, counting from one. Taken from the
    /// unfiltered listing, because a position is the handle
    /// docket reorder --position takes: a narrowed listing renumbered from one
    /// would print handles that address different items.
    pub position: usize,
    /// The body line a search matched on, when it matched there rather than in
    /// the name or the tagline.
    pub excerpt: Option<String>,
}

/// What one record answered with.
struct Answer {
    excerpt: Option<String>,
}

/// What a listing narrows by, built once so every project is narrowed by the
/// same thing.
pub struct Filter {
    kind: Option<Kind>,
    tags: Vec<String>,
    blocked: bool,
    invalid: bool,
    /// Lowered here, because matching ignores case and the text does not change
    /// from one item to the next.
    needle: Option<String>,
}

impl Filter {
    /// Values are checked here rather than at the first item they meet: a tag
    /// that could never be written is a tag that will never match, and saying
    /// so beats an empty listing.
    pub fn new(args: &ListArgs) -> Result<Filter> {
        let mut tags = Vec::new();
        for raw in &args.tag {
            match field::tag(raw)? {
                Some(tag) => tags.push(tag),
                None => bail!("--tag is empty; name the tag to look for"),
            }
        }
        let needle = match &args.search {
            Some(raw) if raw.trim().is_empty() => {
                bail!("--search is empty; give the text to look for")
            }
            Some(raw) => Some(raw.trim().to_lowercase()),
            None => None,
        };
        Ok(Filter {
            kind: args.kind,
            tags,
            blocked: args.blocked,
            invalid: args.invalid,
            needle,
        })
    }

    /// Whether anything was asked for beyond the listing itself, so an empty
    /// result can say which of the two it is.
    pub fn is_narrowing(&self) -> bool {
        self.kind.is_some()
            || !self.tags.is_empty()
            || self.blocked
            || self.invalid
            || self.needle.is_some()
    }

    /// Whether a record answers every filter, and what to quote when a search
    /// found its match in the body.
    ///
    /// Ordered by what each test costs: the shelf and the metadata are already
    /// in hand, and the body is opened last, so a search never reads a file
    /// another filter had already dropped.
    fn admits(&self, record: &Record) -> Option<Answer> {
        if self.invalid && record.item.is_ok() {
            return None;
        }
        // The shelf, not the metadata, so this answers for an item that will
        // not parse. A valid item always agrees with the shelf it sits on.
        if self.kind.is_some_and(|kind| kind != record.kind) {
            return None;
        }
        if self.blocked || !self.tags.is_empty() {
            // Neither can be answered by an item that will not parse.
            let Ok(item) = &record.item else { return None };
            if self.blocked && !item.is_blocked() {
                return None;
            }
            if !self.tags.iter().all(|tag| item.tags.contains(tag)) {
                return None;
            }
        }
        match &self.needle {
            Some(needle) => self.find(needle, record),
            None => Some(Answer { excerpt: None }),
        }
    }

    /// The item's own text: its name, its tagline, and its body. Metadata is
    /// never searched for an item that parses — every one states its kind and
    /// its keys there, so a search for handoff would answer with every handoff
    /// on the machine.
    fn find(&self, needle: &str, record: &Record) -> Option<Answer> {
        let named = match &record.item {
            Ok(item) => holds(&item.name, needle) || holds(&item.tagline, needle),
            // Nothing parsed, so the name is the one in the filename.
            Err(_) => holds(&store::existing_slug(&record.path, record.id), needle),
        };
        if named {
            return Some(Answer { excerpt: None });
        }

        // A file that cannot be read is a miss. A listing that failed on one
        // damaged item would hide every sound one beside it.
        let text = fs_err::read_to_string(&record.path).ok()?;
        let body = match store::split(&text) {
            Ok((_, body)) => body,
            // Unsplittable, so there is no metadata to hold apart from a body.
            Err(_) => text.as_str(),
        };
        let line = body.lines().find(|line| holds(line, needle))?;
        Some(Answer {
            excerpt: Some(field::excerpt(line, needle, field::EXCERPT_MAX)),
        })
    }
}

/// One project's docket, narrowed.
pub fn project(depot: &Depot, project: &Utf8Path, filter: &Filter) -> Vec<Hit> {
    narrow(depot.list(project), filter)
}

/// Every project on the machine as one listing. A project's own order is
/// deliberate, so it wins inside that project; projects are ranked against each
/// other by the item at the head of each.
pub fn roster(depot: &Depot, filter: &Filter) -> Vec<Hit> {
    let mut groups: Vec<(i64, Vec<Hit>)> = depot
        .projects()
        .into_iter()
        .map(|project| narrow(depot.list(&project), filter))
        .filter(|hits| !hits.is_empty())
        .map(|hits| (rank(&hits), hits))
        .collect();
    groups.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1[0].record.project.cmp(&b.1[0].record.project))
    });
    groups.into_iter().flat_map(|(_, hits)| hits).collect()
}

/// The pure core: records in, hits out, positions from the order they arrived
/// in.
fn narrow(records: Vec<Record>, filter: &Filter) -> Vec<Hit> {
    records
        .into_iter()
        .enumerate()
        .filter_map(|(index, record)| {
            let answer = filter.admits(&record)?;
            Some(Hit {
                record,
                position: index + 1,
                excerpt: answer.excerpt,
            })
        })
        .collect()
}

/// Where a project sits in a listing across the machine: when the item at the
/// head of what it shows was opened. A head whose metadata will not parse
/// cannot answer, and ranks last — the convention Record::order already takes.
///
/// The head of what is *shown*, not of the whole docket, so the order a listing
/// is in is explicable from the listing itself.
fn rank(hits: &[Hit]) -> i64 {
    hits.first()
        .and_then(|hit| hit.record.item.as_ref().ok())
        .map_or(i64::MAX, |item| item.created.as_second())
}

/// Plain text rather than a pattern, and case is ignored. `needle` is already
/// lowered.
fn holds(haystack: &str, needle: &str) -> bool {
    haystack.to_lowercase().contains(needle)
}

#[cfg(test)]
mod tests {
    use camino::Utf8PathBuf;

    use jiff::Timestamp;

    use super::*;
    use crate::id::Id;
    use crate::item::{Item, Rung};

    fn filter(args: ListArgs) -> Filter {
        Filter::new(&args).expect("the flags are well formed")
    }

    fn item(name: &str, tags: &[&str], blocked: Option<&str>, created: i64) -> Item {
        Item {
            id: Id::mint(),
            name: name.to_owned(),
            tagline: format!("The tagline of {name}."),
            project: Utf8PathBuf::from("/tmp/project"),
            created: Timestamp::from_second(created).expect("a stamp in range"),
            updated: Timestamp::from_second(created).expect("a stamp in range"),
            order: 10,
            rung: Rung::Handoff,
            blocked: blocked.map(str::to_owned),
            origin: None,
            tags: tags.iter().map(|t| (*t).to_owned()).collect(),
        }
    }

    fn record(kind: Kind, item: Result<Item, String>) -> Record {
        let id = item.as_ref().map(|i| i.id).unwrap_or_else(|_| Id::mint());
        Record {
            id,
            kind,
            path: Utf8PathBuf::from("/tmp/project").join(format!("{id}-NAME.md")),
            project: Utf8PathBuf::from("/tmp/project"),
            item,
        }
    }

    fn sound(name: &str) -> Record {
        record(Kind::Handoff, Ok(item(name, &[], None, 0)))
    }

    fn damaged(kind: Kind) -> Record {
        record(kind, Err("kind is spec, so stage is required".to_owned()))
    }

    #[test]
    fn a_filter_with_no_flags_admits_everything() {
        let filter = filter(ListArgs::default());
        assert!(!filter.is_narrowing());
        let hits = narrow(vec![sound("ALPHA"), damaged(Kind::Spec)], &filter);
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn invalid_selects_only_what_will_not_parse() {
        let filter = filter(ListArgs {
            invalid: true,
            ..ListArgs::default()
        });
        let hits = narrow(vec![sound("ALPHA"), damaged(Kind::Spec)], &filter);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].record.item.is_err());
    }

    #[test]
    fn kind_answers_from_the_shelf_even_when_metadata_will_not_parse() {
        let records = || vec![sound("ALPHA"), damaged(Kind::Spec)];
        let specs = narrow(
            records(),
            &filter(ListArgs {
                kind: Some(Kind::Spec),
                ..ListArgs::default()
            }),
        );
        assert_eq!(specs.len(), 1);
        assert!(specs[0].record.item.is_err());

        let handoffs = narrow(
            records(),
            &filter(ListArgs {
                kind: Some(Kind::Handoff),
                ..ListArgs::default()
            }),
        );
        assert_eq!(handoffs.len(), 1);
        assert!(handoffs[0].record.item.is_ok());
    }

    #[test]
    fn tags_are_demanded_all_at_once() {
        let records = || {
            vec![
                record(Kind::Handoff, Ok(item("BOTH", &["ci", "release"], None, 0))),
                record(Kind::Handoff, Ok(item("ONE", &["ci"], None, 0))),
                damaged(Kind::Handoff),
            ]
        };
        let by = |tags: &[&str]| {
            narrow(
                records(),
                &filter(ListArgs {
                    tag: tags.iter().map(|t| (*t).to_owned()).collect(),
                    ..ListArgs::default()
                }),
            )
            .len()
        };
        assert_eq!(by(&["ci"]), 2);
        assert_eq!(by(&["ci", "release"]), 1);
        assert_eq!(by(&["ci", "nightly"]), 0);
        assert_eq!(by(&["absent"]), 0);
    }

    #[test]
    fn blocked_drops_a_record_that_cannot_answer_it() {
        let hits = narrow(
            vec![
                record(Kind::Handoff, Ok(item("HELD", &[], Some("the review"), 0))),
                sound("FREE"),
                damaged(Kind::Handoff),
            ],
            &filter(ListArgs {
                blocked: true,
                ..ListArgs::default()
            }),
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].record.item.as_ref().unwrap().name, "HELD");
    }

    #[test]
    fn positions_are_the_place_on_the_unfiltered_docket() {
        let records = vec![
            sound("FIRST"),
            damaged(Kind::Spec),
            sound("THIRD"),
            damaged(Kind::Spec),
            sound("FIFTH"),
        ];
        let hits = narrow(
            records,
            &filter(ListArgs {
                kind: Some(Kind::Spec),
                ..ListArgs::default()
            }),
        );
        assert_eq!(
            hits.iter().map(|hit| hit.position).collect::<Vec<_>>(),
            vec![2, 4]
        );
    }

    #[test]
    fn a_project_ranks_by_the_head_it_shows() {
        let older = narrow(
            vec![record(Kind::Handoff, Ok(item("OLD", &[], None, 1_000)))],
            &filter(ListArgs::default()),
        );
        let newer = narrow(
            vec![record(Kind::Handoff, Ok(item("NEW", &[], None, 9_000)))],
            &filter(ListArgs::default()),
        );
        let unknown = narrow(vec![damaged(Kind::Handoff)], &filter(ListArgs::default()));

        assert_eq!(rank(&older), 1_000);
        assert_eq!(rank(&newer), 9_000);
        assert_eq!(rank(&unknown), i64::MAX);
        assert!(rank(&older) < rank(&newer) && rank(&newer) < rank(&unknown));
    }

    #[test]
    fn matching_ignores_case_and_reads_plain_text() {
        assert!(holds("The Rosetta table", "rosetta"));
        assert!(holds("ROSETTA_MESSENGER", "rosetta"));
        assert!(!holds("the roster", "rosetta"));
        // A pattern is text, not a pattern.
        assert!(!holds("the rosetta table", "ros.tta"));
    }

    #[test]
    fn a_search_finds_the_name_and_the_tagline_without_reading_a_body() {
        let filter = filter(ListArgs {
            search: Some("ALPHA".to_owned()),
            ..ListArgs::default()
        });
        // The path does not exist, so a hit here can only have come from
        // metadata already in hand.
        let hits = narrow(vec![sound("ALPHA"), sound("BETA")], &filter);
        assert_eq!(hits.len(), 1);
        assert!(hits[0].excerpt.is_none());
    }

    #[test]
    fn an_unreadable_file_is_a_miss_rather_than_a_failure() {
        let filter = filter(ListArgs {
            search: Some("absent".to_owned()),
            ..ListArgs::default()
        });
        assert!(narrow(vec![sound("ALPHA"), damaged(Kind::Spec)], &filter).is_empty());
    }

    #[test]
    fn an_empty_needle_or_a_malformed_tag_is_refused() {
        assert!(
            Filter::new(&ListArgs {
                search: Some("   ".to_owned()),
                ..ListArgs::default()
            })
            .is_err()
        );
        assert!(
            Filter::new(&ListArgs {
                tag: vec!["two words".to_owned()],
                ..ListArgs::default()
            })
            .is_err()
        );
        assert!(
            Filter::new(&ListArgs {
                tag: vec!["  ".to_owned()],
                ..ListArgs::default()
            })
            .is_err()
        );
    }

    #[test]
    fn metadata_is_not_searched() {
        let item = item("ALPHA", &[], None, 0);
        let rendered = store::render(&item, "The body says nothing of the sort.\n").unwrap();
        let (_, body) = store::split(&rendered).unwrap();
        assert!(rendered.contains("kind: handoff"));
        assert!(!holds(body, "handoff"));
    }
}
