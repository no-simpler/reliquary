//! What to reap, decided in full before anything is removed.
//!
//! Pure over one snapshot of the daemon and one reading of the filesystem, so
//! `--dry-run` and a real run are the same computation rather than two that
//! agree — and so every rule below is a unit test rather than a Docker
//! fixture.

use std::collections::{BTreeMap, BTreeSet};

use camino::Utf8PathBuf;

use crate::docker::Inventory;
use crate::layout::{Anchor, Liveness};
use crate::project::ProjectName;

/// Why a project is a leftover.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Reason {
    /// Its containers were raised from a worktree that is gone.
    Abandoned,
    /// It has no containers left, and its volumes have the main stack's shape.
    Stranded,
}

/// One project, and everything of it that goes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reaping {
    /// The project.
    pub project: ProjectName,
    /// Why it is a leftover.
    pub reason: Reason,
    /// The worktree it was raised from, when that is known.
    pub worktree: Option<Utf8PathBuf>,
    /// Its containers, removed first.
    pub containers: Vec<String>,
    /// Its volumes, removed once nothing holds them.
    pub volumes: Vec<String>,
    /// Its networks, removed last.
    pub networks: Vec<String>,
}

/// The whole sweep.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    /// One entry per project, never one per container.
    pub reapings: Vec<Reaping>,
    /// What was seen and deliberately not acted on.
    pub notes: Vec<String>,
}

/// How a project's containers place it.
///
/// Only projects with **at least one** container raised from a worktree of this
/// repository are placed at all. Everything else on the daemon is somebody
/// else's stack, and saying so about each of them would bury the sweep.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Standing {
    /// Every container came from a worktree of this repository that is gone.
    Abandoned,
    /// At least one container is accounted for by something that still exists.
    Live,
    /// Partly this repository's, and partly something this cannot place.
    Divided,
}

/// Decide the sweep.
///
/// `main` is the live stack's project name, and `Plan` never touches it: the
/// repository's own stack is state, not residue.
#[must_use]
pub fn build(
    anchor: &Anchor,
    inventory: &Inventory,
    liveness: &Liveness,
    main: Option<&ProjectName>,
) -> Plan {
    let mut plan = Plan::default();
    // Every project the daemon holds a container for, placed or not: the
    // stranded sweep is for volume sets with nothing left running, and a
    // container anywhere is enough to disqualify one.
    let with_containers: BTreeSet<ProjectName> = inventory
        .containers
        .iter()
        .map(|container| container.project.clone())
        .collect();

    for (project, standing, worktree) in group(anchor, inventory, liveness) {
        if Some(&project) == main || liveness.accounts_for(&project) {
            continue;
        }
        match standing {
            Standing::Abandoned => plan.reapings.push(Reaping {
                containers: inventory.containers_of(&project),
                volumes: inventory.volumes_of(&project),
                networks: inventory.networks_of(&project),
                project,
                reason: Reason::Abandoned,
                worktree,
            }),
            Standing::Live => {}
            // Reported rather than swept: a project this repository half
            // recognizes is the one case where guessing costs somebody data.
            Standing::Divided => plan.notes.push(format!(
                "{project} was raised from more than one place — left alone"
            )),
        }
    }

    let Some(main) = main else {
        plan.notes
            .push("no main project to match a volume shape against".to_owned());
        plan.reapings.sort_by(|a, b| a.project.cmp(&b.project));
        return plan;
    };
    let shape = signature(inventory, main);
    if shape.is_empty() {
        plan.notes
            .push("main stack never raised — no volume shape to match against".to_owned());
        plan.reapings.sort_by(|a, b| a.project.cmp(&b.project));
        return plan;
    }

    // Volumes carry no config-file label, so a volume-only leftover cannot be
    // traced to a directory. The provenance test is its shape: the same set of
    // declared volume names as the main stack, which is what holds a sibling
    // repository's cache and an IDE's helper volumes out of range.
    for project in projects_with_volumes(inventory) {
        if &project == main
            || with_containers.contains(&project)
            || liveness.accounts_for(&project)
            || signature(inventory, &project) != shape
        {
            continue;
        }
        plan.reapings.push(Reaping {
            containers: Vec::new(),
            volumes: inventory.volumes_of(&project),
            networks: inventory.networks_of(&project),
            project,
            reason: Reason::Stranded,
            worktree: None,
        });
    }

    plan.reapings.sort_by(|a, b| a.project.cmp(&b.project));
    plan
}

/// A project's declared volume names — its resource names with its own prefix
/// removed, which is what makes two projects raised from one file comparable.
fn signature(inventory: &Inventory, project: &ProjectName) -> BTreeSet<String> {
    inventory
        .volumes_of(project)
        .iter()
        .map(|volume| project.strip_prefix(volume).to_owned())
        .collect()
}

fn projects_with_volumes(inventory: &Inventory) -> BTreeSet<ProjectName> {
    inventory
        .volumes
        .iter()
        .map(|volume| volume.project.clone())
        .collect()
}

/// Each project this repository has a claim on, how its containers place it,
/// and the worktree they came from.
fn group(
    anchor: &Anchor,
    inventory: &Inventory,
    liveness: &Liveness,
) -> Vec<(ProjectName, Standing, Option<Utf8PathBuf>)> {
    let mut claims: BTreeMap<ProjectName, Claim> = BTreeMap::new();
    for container in &inventory.containers {
        let worktree = container
            .config_dir
            .as_deref()
            .and_then(|dir| anchor.worktree_of(dir));
        let claim = claims.entry(container.project.clone()).or_default();
        match worktree {
            None => claim.unplaceable = true,
            Some(dir) if liveness.spares(&dir) => {
                claim.live = true;
                claim.worktree.get_or_insert(dir);
            }
            Some(dir) => {
                claim.abandoned = true;
                claim.worktree.get_or_insert(dir);
            }
        }
    }
    claims
        .into_iter()
        .filter_map(|(project, claim)| Some((project, claim.standing()?, claim.worktree)))
        .collect()
}

/// What one project's containers add up to.
#[derive(Default)]
struct Claim {
    /// A container from a worktree of this repository that is still there.
    live: bool,
    /// A container from a worktree of this repository that is gone.
    abandoned: bool,
    /// A container from anywhere else, including one with no compose file
    /// recorded at all.
    unplaceable: bool,
    /// The first worktree of this repository it was seen in.
    worktree: Option<Utf8PathBuf>,
}

impl Claim {
    /// `None` when this repository has no claim on the project at all.
    fn standing(&self) -> Option<Standing> {
        if !self.live && !self.abandoned {
            return None;
        }
        if self.live {
            return Some(Standing::Live);
        }
        if self.unplaceable {
            return Some(Standing::Divided);
        }
        Some(Standing::Abandoned)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::docker::{Container, Resource};
    use camino::Utf8Path;

    fn anchor() -> Anchor {
        Anchor::at(Utf8Path::new("/dev/gmrepo")).unwrap()
    }

    fn project(name: &str) -> ProjectName {
        ProjectName::observed(name).unwrap()
    }

    fn container(id: &str, name: &str, dir: Option<&str>) -> Container {
        Container {
            id: id.to_owned(),
            project: project(name),
            config_dir: dir.map(Utf8PathBuf::from),
        }
    }

    fn resource(id: &str, name: &str) -> Resource {
        Resource {
            id: id.to_owned(),
            project: project(name),
        }
    }

    /// The main stack raised, plus two volumes per project so a shape exists.
    fn inventory() -> Inventory {
        Inventory {
            containers: vec![container("main1", "gmrepo", Some("/dev/gmrepo"))],
            volumes: vec![
                resource("gmrepo_data", "gmrepo"),
                resource("gmrepo_cache", "gmrepo"),
            ],
            networks: vec![resource("net-main", "gmrepo")],
        }
    }

    fn live(dirs: &[&str]) -> Liveness {
        Liveness {
            dirs: dirs.iter().map(Utf8PathBuf::from).collect(),
            projects: BTreeSet::new(),
        }
    }

    #[test]
    fn a_project_is_one_reaping_however_many_containers_it_has() {
        // The retired script called its reclaim step once per container, so a
        // two-service stack was counted twice — and its second pass tried to
        // remove volumes the first container still held.
        let mut inv = inventory();
        inv.containers.push(container(
            "a1",
            "wt-a",
            Some("/dev/gmrepo/.claude/worktrees/wt-a"),
        ));
        inv.containers.push(container(
            "a2",
            "wt-a",
            Some("/dev/gmrepo/.claude/worktrees/wt-a"),
        ));
        inv.volumes.push(resource("wt-a_data", "wt-a"));
        inv.volumes.push(resource("wt-a_cache", "wt-a"));
        inv.networks.push(resource("net-a", "wt-a"));

        let plan = build(&anchor(), &inv, &live(&[]), Some(&project("gmrepo")));
        assert_eq!(plan.reapings.len(), 1);
        let reaping = &plan.reapings[0];
        assert_eq!(reaping.reason, Reason::Abandoned);
        assert_eq!(reaping.containers, vec!["a1".to_owned(), "a2".to_owned()]);
        assert_eq!(
            reaping.volumes,
            vec!["wt-a_cache".to_owned(), "wt-a_data".to_owned()]
        );
        assert_eq!(reaping.networks, vec!["net-a".to_owned()]);
    }

    #[test]
    fn a_worktree_that_still_exists_is_spared() {
        let mut inv = inventory();
        inv.containers.push(container(
            "a1",
            "wt-a",
            Some("/dev/gmrepo/.claude/worktrees/wt-a"),
        ));
        inv.volumes.push(resource("wt-a_data", "wt-a"));
        let plan = build(
            &anchor(),
            &inv,
            &live(&["/dev/gmrepo/.claude/worktrees/wt-a"]),
            Some(&project("gmrepo")),
        );
        assert!(plan.reapings.is_empty());
    }

    #[test]
    fn a_project_named_by_a_live_worktree_is_spared() {
        // The name is the only handle a stranded volume set offers, so a live
        // worktree that would normalize to it takes it out of range.
        let mut inv = inventory();
        inv.volumes.push(resource("wt-a_data", "wt-a"));
        inv.volumes.push(resource("wt-a_cache", "wt-a"));
        let mut liveness = live(&[]);
        liveness.projects.insert(project("wt-a"));
        let plan = build(&anchor(), &inv, &liveness, Some(&project("gmrepo")));
        assert!(plan.reapings.is_empty());
    }

    #[test]
    fn a_foreign_repositorys_stack_is_neither_reaped_nor_reported() {
        // Every other repository on the machine is in the same inventory. A
        // note about each of them would bury the sweep in things it will never
        // touch, which is how a report stops being read.
        let mut inv = inventory();
        inv.containers
            .push(container("x1", "other", Some("/dev/other")));
        inv.volumes.push(resource("other_data", "other"));
        inv.volumes.push(resource("other_cache", "other"));
        let plan = build(&anchor(), &inv, &live(&[]), Some(&project("gmrepo")));
        assert!(plan.reapings.is_empty(), "{plan:?}");
        assert!(plan.notes.is_empty(), "{plan:?}");
    }

    #[test]
    fn an_abandoned_project_whose_name_a_live_worktree_claims_is_spared() {
        // Two worktrees, one gone and one there, and the compose file names
        // the project — so both stacks answer to it and the containers of the
        // removed one are the live one's.
        let mut inv = inventory();
        inv.containers.push(container(
            "a1",
            "declared",
            Some("/dev/gmrepo/.claude/worktrees/wt-a"),
        ));
        inv.volumes.push(resource("declared_data", "declared"));
        let mut liveness = live(&["/dev/gmrepo/.claude/worktrees/wt-live"]);
        liveness.projects.insert(project("declared"));
        let plan = build(&anchor(), &inv, &liveness, Some(&project("gmrepo")));
        assert!(plan.reapings.is_empty(), "{plan:?}");
    }

    #[test]
    fn a_stranded_volume_set_needs_the_main_stacks_shape() {
        let mut inv = inventory();
        // Same shape: reaped.
        inv.volumes.push(resource("wt-b_data", "wt-b"));
        inv.volumes.push(resource("wt-b_cache", "wt-b"));
        // A sibling repository's cache: one volume, a different shape.
        inv.volumes.push(resource("sibling_target", "sibling"));
        let plan = build(&anchor(), &inv, &live(&[]), Some(&project("gmrepo")));
        assert_eq!(plan.reapings.len(), 1);
        assert_eq!(plan.reapings[0].project, project("wt-b"));
        assert_eq!(plan.reapings[0].reason, Reason::Stranded);
    }

    #[test]
    fn a_project_with_containers_is_never_reached_by_the_stranded_sweep() {
        let mut inv = inventory();
        inv.containers.push(container(
            "a1",
            "wt-a",
            Some("/dev/gmrepo/.claude/worktrees/wt-a"),
        ));
        inv.volumes.push(resource("wt-a_data", "wt-a"));
        inv.volumes.push(resource("wt-a_cache", "wt-a"));
        let plan = build(
            &anchor(),
            &inv,
            &live(&["/dev/gmrepo/.claude/worktrees/wt-a"]),
            Some(&project("gmrepo")),
        );
        assert!(plan.reapings.is_empty());
    }

    #[test]
    fn without_a_main_shape_the_stranded_sweep_says_so_and_does_nothing() {
        let inv = Inventory {
            containers: Vec::new(),
            volumes: vec![resource("wt-b_data", "wt-b")],
            networks: Vec::new(),
        };
        let plan = build(&anchor(), &inv, &live(&[]), Some(&project("gmrepo")));
        assert!(plan.reapings.is_empty());
        assert_eq!(plan.notes.len(), 1);
        assert!(plan.notes[0].contains("never raised"));
    }

    #[test]
    fn the_main_project_is_never_swept() {
        let inv = inventory();
        let plan = build(&anchor(), &inv, &live(&[]), Some(&project("gmrepo")));
        assert!(plan.reapings.is_empty(), "{plan:?}");
    }

    #[test]
    fn a_project_this_repository_only_half_owns_is_reported_not_reaped() {
        let mut inv = inventory();
        inv.containers.push(container(
            "a1",
            "wt-a",
            Some("/dev/gmrepo/.claude/worktrees/wt-a"),
        ));
        inv.containers.push(container("a2", "wt-a", None));
        inv.volumes.push(resource("wt-a_data", "wt-a"));
        let plan = build(&anchor(), &inv, &live(&[]), Some(&project("gmrepo")));
        assert!(plan.reapings.is_empty());
        assert_eq!(plan.notes.len(), 1);
        assert!(plan.notes[0].contains("wt-a"));
    }

    #[test]
    fn the_order_is_the_same_whatever_order_docker_answered_in() {
        let mut inv = inventory();
        for name in ["wt-c", "wt-a", "wt-b"] {
            inv.volumes.push(resource(&format!("{name}_data"), name));
            inv.volumes.push(resource(&format!("{name}_cache"), name));
        }
        let plan = build(&anchor(), &inv, &live(&[]), Some(&project("gmrepo")));
        let names: Vec<String> = plan
            .reapings
            .iter()
            .map(|r| r.project.to_string())
            .collect();
        assert_eq!(
            names,
            vec!["wt-a".to_owned(), "wt-b".to_owned(), "wt-c".to_owned()]
        );
    }
}
