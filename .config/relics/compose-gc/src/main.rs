//! `compose-gc` — reclaim what a removed worktree left in the daemon.

use std::collections::BTreeSet;
use std::io::{Write, stdout};
use std::process::ExitCode;

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::{Parser, Subcommand};
use relic_core::git::{self, Git};

use compose_gc::compose;
use compose_gc::docker::{Absent, Docker, Inventory};
use compose_gc::layout::{Anchor, Liveness};
use compose_gc::plan::{self, Reason};
use compose_gc::project::ProjectName;
use compose_gc::render::{self, Refusal, Style};

/// Nothing to do, or done.
const CLEAN: u8 = 0;
/// Something would not go.
const FAILED: u8 = 1;
/// The caller asked for something that is not a thing.
const MISUSE: u8 = 2;

#[derive(Debug, Parser)]
#[command(
    name = "compose-gc",
    about = "Reclaim Docker Compose state left behind by dead worktrees",
    long_about = "Scoped to the current repository: only projects whose compose file lived \
under <repo>/.claude/worktrees/ or <root>/<repo>-* are ever considered, and a volume-only \
leftover must additionally match the main stack's volume shape — which is what holds a \
sibling repository's cache and an IDE's helper volumes out of range.\n\nLiveness is checked \
twice, because either alone is wrong: the worktree directory still being there, and git still \
registering it.",
    version
)]
struct Cli {
    /// Say what would go, and remove nothing.
    #[arg(short = 'n', long, global = true)]
    dry_run: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Tear one worktree's stack down, every profile, with its volumes.
    Down {
        /// The worktree whose stack goes.
        path: Utf8PathBuf,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(&cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("compose-gc: {error:#}");
            ExitCode::from(FAILED)
        }
    }
}

fn run(cli: &Cli) -> Result<u8> {
    let style = Style {
        color: anstream::AutoStream::choice(&stdout()) != anstream::ColorChoice::Never,
        dry_run: cli.dry_run,
    };
    let mut out = stdout().lock();

    let docker = match Docker::connect() {
        Ok(docker) => docker,
        // Neither is a failure: a machine with no daemon holds no Compose
        // state, so there is nothing to reclaim and nothing went wrong.
        Err(Absent::NotInstalled) => {
            render::note(&mut out, "docker not installed — nothing to do", style)?;
            return Ok(CLEAN);
        }
        Err(Absent::Unreachable) => {
            render::note(&mut out, "docker daemon unreachable — nothing to do", style)?;
            return Ok(CLEAN);
        }
    };

    match &cli.command {
        Some(Command::Down { path }) => teardown(&mut out, &docker, path, style),
        None => sweep(&mut out, &docker, style),
    }
}

// ---------------------------------------------------------------- reconcile

fn sweep(out: &mut impl Write, docker: &Docker, style: Style) -> Result<u8> {
    let cwd = relic_core::path::cwd().context("no working directory")?;
    let Some(git) = git::detect() else {
        render::note(
            out,
            "git not installed — no repository to be scoped to",
            style,
        )?;
        return Ok(CLEAN);
    };
    let Some(repo) = git.main_worktree(&cwd) else {
        render::note(out, "not inside a git repository — nothing to do", style)?;
        return Ok(CLEAN);
    };
    let Some(anchor) = Anchor::at(&repo) else {
        render::note(
            out,
            "no main worktree for this repository — nothing to do",
            style,
        )?;
        return Ok(CLEAN);
    };

    let inventory = docker
        .inventory()
        .context("reading the daemon's Compose state")?;
    let main = main_project(&anchor, &inventory, docker);
    let mut liveness = liveness(git, &anchor, &inventory, main.as_ref());
    let mut plan = plan::build(&anchor, &inventory, &liveness, main.as_ref());

    // A live worktree whose compose file sets `name:` answers to a project the
    // directory does not spell, and a stranded volume set is matched on name
    // alone — so that worktree's data would be this sweep's to destroy. Asking
    // Compose closes it, and is paid for only when something is at stake.
    if plan.reapings.iter().any(|r| r.reason == Reason::Stranded) {
        let declared: BTreeSet<ProjectName> = liveness
            .dirs
            .iter()
            .filter_map(|dir| docker.compose_project_name(dir))
            .collect();
        if !declared.is_subset(&liveness.projects) {
            liveness.projects.extend(declared);
            plan = plan::build(&anchor, &inventory, &liveness, main.as_ref());
        }
    }

    for note in &plan.notes {
        render::note(out, note, style)?;
    }
    let mut failed = false;
    for reaping in &plan.reapings {
        let refusals = if style.dry_run {
            Vec::new()
        } else {
            reclaim(docker, reaping)
        };
        failed |= !refusals.is_empty();
        render::reaping(out, reaping, &refusals, style)?;
    }
    render::summary(out, &plan, &anchor.name, style)?;
    Ok(if failed { FAILED } else { CLEAN })
}

/// Remove one project, in the only order that can work.
///
/// Containers first — **all** of them — because a volume or network is refused
/// while any container still holds it. The retired script reclaimed after each
/// container in turn, so every stack with more than one service reported
/// failures for work its own next pass then completed.
fn reclaim(docker: &Docker, reaping: &compose_gc::Reaping) -> Vec<Refusal> {
    let mut refusals = Vec::new();
    let mut held = false;
    for id in &reaping.containers {
        if let Err(error) = docker.remove_container(id) {
            refusals.push(refusal("container", id, &error));
            held = true;
        }
    }
    // A volume still held by a container that would not go is not news, and a
    // second diagnosis of the first failure is how a report gets skimmed.
    if held {
        return refusals;
    }
    for name in &reaping.volumes {
        if let Err(error) = docker.remove_volume(name) {
            refusals.push(refusal("volume", name, &error));
        }
    }
    for id in &reaping.networks {
        if let Err(error) = docker.remove_network(id) {
            refusals.push(refusal("network", id, &error));
        }
    }
    refusals
}

fn refusal(kind: &str, id: &str, error: &relic_core::tool::Error) -> Refusal {
    Refusal {
        what: format!("{kind} {id}"),
        why: error.to_string(),
    }
}

/// The live stack's project name.
///
/// Its own containers know it; failing that Compose is asked, because `name:`
/// and `COMPOSE_PROJECT_NAME` both outrank the directory; failing that, the
/// name Compose would derive from the directory.
fn main_project(anchor: &Anchor, inventory: &Inventory, docker: &Docker) -> Option<ProjectName> {
    let observed = inventory
        .containers
        .iter()
        .filter(|container| {
            container.config_dir.as_deref().is_some_and(|dir| {
                dir.starts_with(&anchor.repo) && anchor.worktree_of(dir).is_none()
            })
        })
        .map(|container| container.project.clone())
        .next();
    if observed.is_some() {
        return observed;
    }
    if compose::designates_a_stack(&anchor.repo, designated())
        && let Some(name) = docker.compose_project_name(&anchor.repo)
    {
        return Some(name);
    }
    ProjectName::derived(&anchor.repo)
}

fn designated() -> bool {
    std::env::var_os(compose::COMPOSE_FILE).is_some_and(|value| !value.is_empty())
}

/// Every worktree of this repository that still exists or is still registered,
/// and the projects they account for.
fn liveness(
    git: Git,
    anchor: &Anchor,
    inventory: &Inventory,
    main: Option<&ProjectName>,
) -> Liveness {
    let mut dirs: BTreeSet<Utf8PathBuf> = BTreeSet::new();

    for dir in [anchor.nest(), anchor.root.clone()] {
        let Ok(entries) = std::fs::read_dir(dir.as_std_path()) else {
            continue;
        };
        for entry in entries.flatten() {
            let Ok(path) = relic_core::path::utf8(entry.path()) else {
                continue;
            };
            if path.is_dir() && anchor.worktree_of(&path).as_ref() == Some(&path) {
                dirs.insert(path);
            }
        }
    }
    for path in registered(git, anchor) {
        if let Some(root) = anchor.worktree_of(&path) {
            dirs.insert(root);
        }
    }

    let mut projects: BTreeSet<ProjectName> = dirs
        .iter()
        .filter_map(|d| ProjectName::derived(d))
        .collect();
    projects.extend(main.cloned());
    // A live worktree's own containers name it, whatever the directory is
    // called — the same reading the main stack gets.
    for container in &inventory.containers {
        let live = container
            .config_dir
            .as_deref()
            .and_then(|dir| anchor.worktree_of(dir))
            .is_some_and(|root| dirs.contains(&root));
        if live {
            projects.insert(container.project.clone());
        }
    }
    Liveness { dirs, projects }
}

fn registered(git: Git, anchor: &Anchor) -> Vec<Utf8PathBuf> {
    let mut command = git.at(&anchor.repo);
    command.args(["worktree", "list", "--porcelain"]);
    let Ok(output) = git.capture(&mut command) else {
        return Vec::new();
    };
    output
        .stdout
        .lines()
        .filter_map(|line| line.strip_prefix("worktree "))
        .map(Utf8PathBuf::from)
        .collect()
}

// ----------------------------------------------------------------- teardown

/// Bring one worktree's stack down, and prove it went.
///
/// The retired script read Docker's English to tell an absent compose file from
/// a resource that outlived the teardown. Both are answered here without
/// reading a message meant for a person: absence is a filesystem question, and
/// survival is asked of the daemon afterwards.
fn teardown(out: &mut impl Write, docker: &Docker, path: &Utf8Path, style: Style) -> Result<u8> {
    if !path.as_std_path().is_dir() {
        // The caller names a worktree it has not removed yet. A path that is
        // not there is a mistake, and reporting "nothing to tear down" would
        // let a typo pass for a clean teardown.
        eprintln!("compose-gc: no such directory: {path}");
        return Ok(MISUSE);
    }
    let designated = compose::designates_a_stack(path, designated());
    let before = projects_at(docker, path)?;

    if style.dry_run {
        if designated || !before.is_empty() {
            render::note(
                out,
                &format!("would tear down the compose project at {path}"),
                style,
            )?;
        } else {
            render::note(
                out,
                &format!("no compose project at {path} — nothing to tear down"),
                style,
            )?;
        }
        return Ok(CLEAN);
    }

    let result = docker
        .compose_down(path)
        .context("tearing the stack down")?;
    if !result.ok && !designated && before.is_empty() {
        render::note(
            out,
            &format!("no compose project at {path} — nothing to tear down"),
            style,
        )?;
        return Ok(CLEAN);
    }

    // The postcondition, asked of the daemon rather than inferred from what
    // Compose printed: whatever it says, what matters is whether anything of
    // the project is still there.
    let after = docker
        .inventory()
        .context("re-reading the daemon's Compose state")?;
    let survivors: Vec<String> = before
        .iter()
        .flat_map(|project| {
            let mut names: Vec<String> = after
                .containers_of(project)
                .into_iter()
                .map(|id| format!("container {id}"))
                .collect();
            names.extend(
                after
                    .volumes_of(project)
                    .into_iter()
                    .map(|v| format!("volume {v}")),
            );
            names.extend(
                after
                    .networks_of(project)
                    .into_iter()
                    .map(|n| format!("network {n}")),
            );
            names
        })
        .collect();

    if !survivors.is_empty() {
        for survivor in &survivors {
            render::note(out, &format!("{survivor} outlived the teardown"), style)?;
        }
        eprintln!(
            "compose-gc: teardown of {path} left {} resource(s) — the reconcile sweep still has work",
            survivors.len()
        );
        return Ok(FAILED);
    }
    if !result.ok {
        eprintln!(
            "compose-gc: teardown of {path} failed: {}",
            result.stderr.trim()
        );
        return Ok(FAILED);
    }
    render::note(
        out,
        &format!("tore down the compose project at {path}"),
        style,
    )?;
    Ok(CLEAN)
}

/// The projects whose containers were raised from somewhere under `path`.
///
/// Read before the teardown, because afterwards there is nothing left to ask.
fn projects_at(docker: &Docker, path: &Utf8Path) -> Result<BTreeSet<ProjectName>> {
    let inventory = docker
        .inventory()
        .context("reading the daemon's Compose state")?;
    Ok(inventory
        .containers
        .iter()
        .filter(|container| {
            container
                .config_dir
                .as_deref()
                .is_some_and(|dir| dir.starts_with(path))
        })
        .map(|container| container.project.clone())
        .collect())
}
