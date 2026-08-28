//! The station roster, and choosing from it.

use anyhow::{Result, bail};

use relic_core::finding::StationId;

use crate::station::Station;

/// Every station, in the order a run reports them.
///
/// Order is by subject, not by speed. A run does not halt at the first failure —
/// `assay` is a standing audit rather than a commit gate, and an audit that stops
/// at the first problem hides how many there are — so there is nothing for
/// fastest-first ordering to buy.
pub fn roster() -> Vec<Box<dyn Station>> {
    vec![
        Box::new(crate::stations::bedrock::Bedrock::default()),
        Box::new(crate::stations::brew_health::BrewHealth::default()),
        Box::new(crate::stations::md_blocks::MdBlocks::default()),
        Box::new(crate::stations::shell_parity::ShellParity::default()),
        Box::new(crate::stations::shell_lint::ShellLint::default()),
        Box::new(crate::stations::shell_startup::ShellStartup::default()),
        Box::new(crate::stations::relic_cache::RelicCache::default()),
        Box::new(crate::stations::yadm_coverage::YadmCoverage::default()),
        Box::new(crate::stations::registry::RegistryAdapter::default()),
    ]
}

/// The stations named, or all of them.
///
/// An unknown name is refused rather than ignored: a typo that silently runs
/// nothing reports a clean machine, which is the failure mode this whole crate
/// exists to close.
///
/// # Errors
///
/// When a name is not a station id, or names no station in the roster.
pub fn select(all: Vec<Box<dyn Station>>, names: &[String]) -> Result<Vec<Box<dyn Station>>> {
    if names.is_empty() {
        return Ok(all);
    }

    let mut wanted = Vec::with_capacity(names.len());
    for name in names {
        let id: StationId = name
            .parse()
            .map_err(|error| anyhow::anyhow!("{name:?} is not a station name: {error}"))?;
        if !all.iter().any(|station| station.id() == &id) {
            let known = all
                .iter()
                .map(|station| station.id().as_str())
                .collect::<Vec<_>>()
                .join(", ");
            if known.is_empty() {
                bail!("no station is called {id}; there are no stations");
            }
            bail!("no station is called {id}; there is: {known}");
        }
        if !wanted.contains(&id) {
            wanted.push(id);
        }
    }

    Ok(all
        .into_iter()
        .filter(|station| wanted.contains(station.id()))
        .collect())
}

#[cfg(test)]
mod tests {
    use anyhow::Result;
    use relic_core::finding::Outcome;

    use super::*;
    use crate::station::Context;

    struct Fixture(StationId);

    fn fixture(id: &str) -> Box<dyn Station> {
        Box::new(Fixture(id.parse().expect("a valid id")))
    }

    impl Station for Fixture {
        fn id(&self) -> &StationId {
            &self.0
        }
        fn title(&self) -> &'static str {
            "a fixture"
        }
        fn check(&self, _cx: &Context) -> Result<Outcome> {
            Ok(Outcome::Ran(Vec::new()))
        }
    }

    fn three() -> Vec<Box<dyn Station>> {
        vec![fixture("one"), fixture("two"), fixture("three")]
    }

    #[test]
    fn every_published_station_id_is_a_station_id() {
        for station in roster() {
            let spelled = station.id().as_str();
            assert_eq!(
                spelled.parse::<StationId>().as_ref(),
                Ok(station.id()),
                "{spelled} is not well formed"
            );
            assert!(!station.title().is_empty(), "{spelled} has no title");
        }
    }

    #[test]
    fn no_two_stations_share_a_name() {
        let mut seen: Vec<StationId> = Vec::new();
        for station in roster() {
            assert!(
                !seen.contains(station.id()),
                "{} is listed twice",
                station.id()
            );
            seen.push(station.id().clone());
        }
    }

    #[test]
    fn naming_nothing_selects_everything() {
        assert_eq!(select(three(), &[]).expect("selected").len(), 3);
    }

    #[test]
    fn selection_keeps_roster_order_not_argument_order() {
        let chosen = select(three(), &["three".to_owned(), "one".to_owned()]).expect("selected");
        let ids: Vec<_> = chosen.iter().map(|s| s.id().to_string()).collect();
        assert_eq!(ids, vec!["one", "three"]);
    }

    #[test]
    fn a_name_repeated_runs_once() {
        let chosen = select(three(), &["two".to_owned(), "two".to_owned()]).expect("selected");
        assert_eq!(chosen.len(), 1);
    }

    #[test]
    fn an_unknown_name_is_refused_rather_than_silently_running_nothing() {
        let Err(error) = select(three(), &["tow".to_owned()]) else {
            panic!("an unknown name must be refused");
        };
        let text = error.to_string();
        assert!(text.contains("tow"), "{text}");
        assert!(text.contains("one, two, three"), "{text}");
    }

    #[test]
    fn a_name_that_is_not_even_an_id_says_so() {
        let Err(error) = select(three(), &["Two!".to_owned()]) else {
            panic!("a malformed name must be refused");
        };
        assert!(error.to_string().contains("not a station name"));
    }
}
