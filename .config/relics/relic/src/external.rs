//! Stage-3 relics: independent repositories that publish through the same
//! helper and are managed in their own trees.
//!
//! The list is parsed out of `GRADUATION.md`'s *Known external relics* section,
//! and is **a best-effort convenience, not authoritative**. The dependency runs
//! one way — a relic depends on Reliquary, never the reverse — so nothing here
//! reads an external repository, and a parse miss yields no rows rather than an
//! error.

use camino::{Utf8Path, Utf8PathBuf};

/// The heading the list sits under.
const HEADING: &str = "### Known external relics";

/// One Stage-3 relic.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    /// Its name.
    pub name: String,
    /// Where its repository is on this machine, if it is.
    pub path: Utf8PathBuf,
}

/// Read the list, or nothing.
#[must_use]
pub fn all(graduation: &Utf8Path, home: &Utf8Path) -> Vec<External> {
    fs_err::read_to_string(graduation.as_std_path())
        .map(|body| parse(&body, home))
        .unwrap_or_default()
}

/// Parse the section out of the document.
#[must_use]
pub fn parse(body: &str, home: &Utf8Path) -> Vec<External> {
    let mut out = Vec::new();
    let mut inside = false;
    for line in body.lines() {
        if line.starts_with(HEADING) {
            inside = true;
            continue;
        }
        if !inside {
            continue;
        }
        if line.starts_with('#') {
            break;
        }
        let Some(item) = line.strip_prefix("- ") else {
            continue;
        };
        let mut spans = item.split('`').skip(1).step_by(2);
        let (Some(name), Some(path)) = (spans.next(), spans.next()) else {
            continue;
        };
        out.push(External {
            name: name.to_owned(),
            path: expand(path, home),
        });
    }
    out
}

/// Resolve a leading `~` against `home`, and drop a trailing slash.
fn expand(path: &str, home: &Utf8Path) -> Utf8PathBuf {
    let rest = path.strip_suffix('/').unwrap_or(path);
    match rest.strip_prefix("~/") {
        Some(tail) => home.join(tail),
        None => Utf8PathBuf::from(rest),
    }
}

#[cfg(test)]
mod tests {
    use super::{expand, parse};
    use camino::Utf8Path;

    const HOME: &str = "/home/x";

    #[test]
    fn the_section_is_bounded_by_the_next_heading() {
        let body = "\
# Graduation
- `before` — `~/nope`

### Known external relics

- `bb` — `~/Developer/bb`
- `halo` — `~/Developer/halo/`

## Something else
- `after` — `~/nope`
";
        let found = parse(body, Utf8Path::new(HOME));
        let names: Vec<&str> = found.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["bb", "halo"]);
        assert_eq!(
            found.get(1).map(|e| e.path.as_str()),
            Some("/home/x/Developer/halo")
        );
    }

    #[test]
    fn a_line_that_is_not_a_pair_of_spans_is_not_a_relic() {
        let body = "### Known external relics\n\n- `only-one-span`\n- prose\n";
        assert!(parse(body, Utf8Path::new(HOME)).is_empty());
    }

    #[test]
    fn a_document_with_no_section_yields_nothing() {
        assert!(parse("# Graduation\n\nprose\n", Utf8Path::new(HOME)).is_empty());
    }

    #[test]
    fn a_tilde_is_the_home_it_was_given_and_nothing_else_is_touched() {
        assert_eq!(expand("~/a/b", Utf8Path::new(HOME)), "/home/x/a/b");
        assert_eq!(expand("/abs/path", Utf8Path::new(HOME)), "/abs/path");
        assert_eq!(expand("~/a/", Utf8Path::new(HOME)), "/home/x/a");
    }
}
