//! `decruft` — remove what a tool left behind and would rebuild without
//! noticing.

use anyhow::{Context, Result};
use camino::{Utf8Path, Utf8PathBuf};
use clap::Parser;

use decruft::{Doomed, Plan, cruft, ignored, walk};

/// The sweep.
#[derive(Debug, Parser)]
#[command(
    name = "decruft",
    about = "Remove inert OS metadata and interpreter caches",
    long_about = "Two lanes, because \"may this be deleted?\" has two different best \
answers.\n\nInside a git repository the repository answers: only ignored, untracked paths \
are candidates, so a per-repository unignore is respected and a tracked file is never a \
candidate at all. Outside one there is nobody to ask, so the answer is by name, and the set \
of names is small.\n\nEditor swap, backup and lock files are never removed. They are \
gitignored, which keeps them out of commits, but a live one is crash-recovery state. \
Dependency and build trees are left alone too — inert, but expensive to rebuild.\n\n\
Directories left empty by a removal are reported, never deleted.",
    version
)]
struct Cli {
    /// Say what would be removed, and remove nothing.
    #[arg(short = 'n', long)]
    dry_run: bool,

    /// Print only the summary.
    #[arg(short, long)]
    quiet: bool,

    /// Sweep under this directory instead of the home directory.
    #[arg(long, value_name = "DIR")]
    root: Option<Utf8PathBuf>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let home = match cli.root.clone() {
        Some(root) => root,
        None => relic_core::path::home().context("no home directory to sweep")?,
    };
    let root = home.clone();
    let data = data_dir(&home, cli.root.is_some());

    let plan = build(&root, &home, &data)?;
    report(&plan, &cli, &root);
    apply(&plan, cli.dry_run, cli.quiet)?;
    epilogue(&plan, &cli, &data);
    Ok(())
}

/// Where the depots live. No repository and no ignore file, so the name is the
/// only answer available there.
///
/// `--root` relocates it too: pointing the sweep at one tree and the data lane
/// at another would sweep something the caller did not name.
fn data_dir(home: &Utf8Path, relocated: bool) -> Utf8PathBuf {
    if relocated {
        return home.join(".local/share");
    }
    std::env::var_os("XDG_DATA_HOME")
        .and_then(|raw| relic_core::path::utf8(raw.into()).ok())
        .unwrap_or_else(|| home.join(".local/share"))
}

fn build(root: &Utf8Path, home: &Utf8Path, data: &Utf8Path) -> Result<Plan> {
    let mut plan = Plan::default();
    let repos = walk::repositories(root, home, data);
    plan.repositories = repos.len();

    let tool = ignored::tool()?;
    for repo in &repos {
        match ignored::paths(&tool, repo) {
            Ok(paths) => condemn(&mut plan, repo, &paths, home, data),
            Err(error) => plan.unanswered.push((repo.clone(), error.to_string())),
        }
    }

    // The data directory is swept by name, minus anything a repository already
    // answered for. It is not always under the root a run was pointed at.
    if data.is_dir() && data.starts_with(root) {
        for path in walk::cruft_by_name(data, home, data, &repos) {
            plan.doomed.push(Doomed {
                path,
                by_repository: false,
            });
        }
    }
    Ok(plan)
}

/// What a repository's answer condemns.
///
/// An entry is either cruft itself, or a directory git collapsed. Collapsed
/// means *everything under here is ignored*, which is exactly the condition
/// that makes the name a safe answer — so the by-name rule applies inside it.
/// Without this the cruft in an otherwise-ignored directory is invisible: git
/// reports the parent and stops, and the parent is not cruft.
fn condemn(
    plan: &mut Plan,
    repo: &Utf8Path,
    paths: &[Utf8PathBuf],
    home: &Utf8Path,
    data: &Utf8Path,
) {
    for relative in paths {
        let Some(name) = relative.file_name() else {
            continue;
        };
        let absolute = repo.join(relative);
        if cruft::is_cruft(name) {
            plan.doomed.push(Doomed {
                path: absolute,
                by_repository: true,
            });
            continue;
        }
        // Ignored, but expensive to rebuild, so not descended into — the same
        // reason the walker prunes it.
        if cruft::is_pruned(name) || !absolute.is_dir() {
            continue;
        }
        for path in walk::cruft_by_name(&absolute, home, data, &[]) {
            plan.doomed.push(Doomed {
                path,
                by_repository: true,
            });
        }
    }
}

fn report(plan: &Plan, cli: &Cli, root: &Utf8Path) {
    for (repo, why) in &plan.unanswered {
        eprintln!("decruft: {repo} went unswept — {why}");
    }
    if cli.quiet {
        return;
    }
    let verb = if cli.dry_run {
        "would remove"
    } else {
        "removed"
    };
    for item in plan.collapsed() {
        let lane = if item.by_repository { "" } else { " (by name)" };
        println!("  {verb} {}{lane}", shorten(&item.path, root));
    }
}

fn apply(plan: &Plan, dry_run: bool, _quiet: bool) -> Result<()> {
    if dry_run {
        return Ok(());
    }
    for item in plan.collapsed() {
        let path = &item.path;
        // Symlinks are unlinked, never followed: a link named like cruft is
        // one file, and whatever it points at is not this program's to judge.
        // Gone between the plan and the removal — its parent took it.
        let Ok(metadata) = fs_err::symlink_metadata(path.as_std_path()) else {
            continue;
        };
        let removed = if metadata.is_dir() {
            fs_err::remove_dir_all(path.as_std_path())
        } else {
            fs_err::remove_file(path.as_std_path())
        };
        if let Err(error) = removed {
            eprintln!("decruft: {path} survived — {error}");
        }
    }
    Ok(())
}

fn epilogue(plan: &Plan, cli: &Cli, data: &Utf8Path) {
    let count = plan.collapsed().len();
    let verb = if cli.dry_run {
        "would remove"
    } else {
        "removed"
    };
    if count == 0 {
        if !cli.quiet {
            println!(
                "decruft: nothing to remove across {} repositor{} + {data}",
                plan.repositories,
                plural(plan.repositories)
            );
        }
    } else {
        println!(
            "decruft: {verb} {count} item(s) across {} repositor{} + {data}",
            plan.repositories,
            plural(plan.repositories)
        );
    }

    if cli.dry_run {
        return;
    }
    let emptied = plan.emptied(&|path: &Utf8Path| {
        std::fs::read_dir(path.as_std_path()).is_ok_and(|mut entries| entries.next().is_some())
    });
    if !emptied.is_empty() {
        println!("decruft: {} director(ies) left empty:", emptied.len());
        for path in emptied {
            println!("  {path}");
        }
    }
}

const fn plural(n: usize) -> &'static str {
    if n == 1 { "y" } else { "ies" }
}

/// A path relative to the swept root, so a listing is readable rather than a
/// column of identical prefixes.
fn shorten(path: &Utf8Path, root: &Utf8Path) -> String {
    path.strip_prefix(root)
        .map_or_else(|_| path.to_string(), ToString::to_string)
}
