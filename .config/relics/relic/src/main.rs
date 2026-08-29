//! `relic` — manage Reliquary relics.
//!
//! The first Stage-2 relic, and the one that publishes every other. That is why
//! it was the last thing this repository's Rustification rewrote: nothing on the
//! path from a bare machine to the first binary may presuppose a binary.

use std::io::Write;
use std::process::ExitCode;

use anyhow::{Result, bail};
use camino::Utf8PathBuf;
use clap::{Parser, Subcommand};
use relic_core::ui::{ColorChoice, Format};

use relic::doctor;
use relic::external::{self, External};
use relic::gate;
use relic::lane::{self, Lane, Relic};
use relic::manifest::Runtime;
use relic::paths::Paths;
use relic::publish;
use relic::ratchet;
use relic::registry::{Registry, State};
use relic::render::{Style, heading, pad};
use relic::scaffold::{self, Name};

/// Nothing wrong, or done.
const CLEAN: u8 = 0;
/// Something refused, or drifted.
const REFUSED: u8 = 1;

#[derive(Debug, Parser)]
#[command(
    name = "relic",
    about = "Manage Reliquary relics",
    long_about = "A relic is a personal tool the author keeps, at one of three stages: a \
one-shot script in ~/.config/bin, an in-house directory with a manifest, or an independent \
repository.\n\nThis is the surface over stages 1 and 2 — what exists, what is published, what \
has drifted, and the gates each one passes before it lands on PATH.\n\nRelics are Rust by \
default; any other runtime records why in its manifest, and `doctor` lists the ones that have \
not. <NAME> may be omitted for status, publish, test, update and mutants when run from inside a \
relic's directory.",
    version,
    infer_subcommands = true,
    disable_help_subcommand = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// All relics, with stage, runtime and published state.
    List,
    /// One relic's detail: deps, PATH wiring, published names.
    Status {
        /// Which one. Defaults to the relic the cwd is inside.
        name: Option<String>,
    },
    /// Publish a relic's entrypoints onto PATH.
    Publish {
        /// Which one. Defaults to the relic the cwd is inside.
        name: Option<String>,
        /// Every in-house relic in both lanes. What bootstrap hands off to.
        #[arg(long, conflicts_with = "name")]
        all: bool,
    },
    /// Run the gates: format, lint, suite.
    Test {
        /// Which one. Defaults to the relic the cwd is inside.
        name: Option<String>,
        /// Add coverage, and check it against the committed baseline.
        #[arg(long)]
        cover: bool,
    },
    /// Mutate the code and check that the tests fail.
    Mutants {
        /// Which one. Defaults to the relic the cwd is inside.
        name: Option<String>,
        /// Passed through to `cargo mutants`.
        #[arg(trailing_var_arg = true)]
        extra: Vec<String>,
    },
    /// Rebuild and republish, or run the relic's own periodic job.
    Update {
        /// Which one. Defaults to the relic the cwd is inside.
        name: Option<String>,
        /// Every in-house relic in both lanes. What `up` runs.
        #[arg(long, conflicts_with = "name")]
        all: bool,
    },
    /// Promote a Stage-1 util, or lay down a fresh relic.
    Scaffold {
        /// The relic's name, which is also the binary's.
        name: Name,
        /// What it is written in. Inferred from a promoted script's shebang,
        /// else rust.
        #[arg(short = 'r', long)]
        runtime: Option<Runtime>,
        /// Why it is not Rust. Required for any runtime but rust.
        #[arg(short = 'e', long, value_name = "WHY")]
        exempt: Option<String>,
    },
    /// Show, fold or prune the shared PATH registry.
    Registry {
        /// Fold legacy per-meta registries into the shared one.
        #[arg(long, conflicts_with = "prune")]
        migrate: bool,
        /// Drop entries with no backing file.
        #[arg(long)]
        prune: bool,
    },
    /// Fold legacy per-meta registries into the shared one.
    Migrate,
    /// Cross-check registry ↔ ~/.local/bin ↔ entrypoints.
    Doctor,
    /// Print this message.
    Help,
}

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            let _ = error.print();
            return ExitCode::from(u8::try_from(error.exit_code()).unwrap_or(REFUSED));
        }
    };
    match run(cli) {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            let _ = writeln!(anstream::stderr(), "error: {error:#}");
            ExitCode::from(REFUSED)
        }
    }
}

/// Everything a command needs, resolved once.
struct Cx {
    paths: Paths,
    style: Style,
}

impl Cx {
    /// The registry, read fresh: a publish in the same run changes it.
    fn registry(&self) -> Registry {
        Registry::load(&self.paths.registry())
    }

    /// The relic a command was aimed at, or the one the cwd is inside.
    ///
    /// `op` names the caller, because an external relic is refused by name and
    /// the useful half of that refusal is *which* flow to go and run.
    fn aim(&self, op: &str, name: Option<&str>) -> Result<Relic> {
        if let Some(name) = name {
            if let Some(relic) = lane::find(&self.paths, name) {
                return Ok(relic);
            }
            if let Some(found) = self.external(name) {
                bail!(
                    "{name} is an external (Stage 3) relic — run its own {op} flow in {}",
                    found.path
                );
            }
            bail!("unknown relic: {name}");
        }
        let cwd = relic_core::path::cwd()?;
        lane::containing(&self.paths, &cwd)
            .ok_or_else(|| anyhow::anyhow!("no relic given and none detected from cwd"))
    }

    /// A named external relic, when GRADUATION.md lists one.
    fn external(&self, name: &str) -> Option<External> {
        external::all(&self.paths.graduation, &self.paths.home)
            .into_iter()
            .find(|e| e.name == name)
    }
}

fn run(cli: Cli) -> Result<u8> {
    let format = Format::from_process(None, "RELIC_UI");
    let cx = Cx {
        paths: Paths::from_env()?,
        // The whole colour ladder in one place: `Auto` honours `NO_COLOR`,
        // `CLICOLOR_FORCE`, `TERM=dumb` and tty-ness through `anstream`, which
        // is the same one clap itself walks.
        style: Style {
            colour: ColorChoice::Auto.use_color(format),
        },
    };
    let mut out = anstream::stdout();
    match cli.command {
        Command::Help => {
            let mut command = <Cli as clap::CommandFactory>::command();
            command.print_long_help()?;
            Ok(CLEAN)
        }
        Command::List => list(&mut out, &cx),
        Command::Status { name } => status(&mut out, &cx, name.as_deref()),
        Command::Publish { name, all } => {
            if all {
                return each(&mut out, &cx, "publish", publish::publish);
            }
            publish::publish(&cx.paths, &cx.aim("publish", name.as_deref())?)?;
            Ok(CLEAN)
        }
        Command::Test { name, cover } => {
            let relic = cx.aim("test", name.as_deref())?;
            if cover {
                gate::cover(&relic)?;
            } else {
                gate::test(&cx.paths, &relic)?;
            }
            Ok(CLEAN)
        }
        Command::Mutants { name, extra } => {
            gate::mutants(&cx.aim("mutants", name.as_deref())?, &extra)?;
            Ok(CLEAN)
        }
        Command::Update { name, all } => {
            if all {
                return each(&mut out, &cx, "update", gate::update);
            }
            gate::update(&cx.paths, &cx.aim("update", name.as_deref())?)?;
            Ok(CLEAN)
        }
        Command::Scaffold {
            name,
            runtime,
            exempt,
        } => scaffold_relic(&mut out, &cx, &name, runtime, exempt.as_deref()),
        Command::Registry { migrate, prune } => {
            if migrate {
                return fold(&mut out, &cx);
            }
            if prune {
                publish::registry_helper(&cx.paths, "install_on_path_prune_registry")?;
                return Ok(CLEAN);
            }
            show_registry(&mut out, &cx)
        }
        Command::Migrate => fold(&mut out, &cx),
        Command::Doctor => examine(&mut out, &cx),
    }
}

/// Report anything discovery could not read, once, before the tables.
fn warn_unreadable(relics: &[Relic]) {
    let mut stderr = anstream::stderr();
    for relic in relics {
        if let Err(problem) = &relic.manifest {
            let _ = writeln!(stderr, "warn: {problem}");
        }
    }
}

fn list(out: &mut impl Write, cx: &Cx) -> Result<u8> {
    let relics = lane::all(&cx.paths);
    warn_unreadable(&relics);
    let registry = cx.registry();

    for lane in Lane::both() {
        let members: Vec<&Relic> = relics.iter().filter(|r| r.lane == lane).collect();
        if lane == Lane::Private && members.is_empty() {
            continue;
        }
        if lane == Lane::Private {
            writeln!(out)?;
        }
        heading(
            out,
            cx.style,
            lane.heading(),
            &format!("({})", short(&cx.paths, &lane.root(&cx.paths))),
        )?;
        if members.is_empty() {
            writeln!(out, "  {}", cx.style.dim("(none)"))?;
        }
        for relic in members {
            let runtime = relic.manifest.as_ref().map_or("?", |m| m.runtime.as_str());
            let state = if relic.manifest.is_err() {
                State::Broken
            } else {
                State::of(&registry, &relic.published_names())
            };
            writeln!(
                out,
                "  {} {} {}",
                pad(relic.slug(), 18),
                pad(runtime, 7),
                paint(cx.style, state)
            )?;
        }
    }

    let externals = external::all(&cx.paths.graduation, &cx.paths.home);
    if !externals.is_empty() {
        writeln!(out)?;
        heading(
            out,
            cx.style,
            "External relics",
            "(Stage 3; per GRADUATION.md)",
        )?;
        for found in &externals {
            let state = if found.path.is_dir() {
                if registry.knows_owner(&found.name) {
                    State::Published
                } else {
                    State::Unknown
                }
            } else {
                State::Absent
            };
            writeln!(
                out,
                "  {} {} {}",
                pad(&found.name, 18),
                pad("ext", 7),
                paint(cx.style, state)
            )?;
        }
    }
    Ok(CLEAN)
}

/// A published state, in its colour.
fn paint(style: Style, state: State) -> String {
    let label = state.label();
    match state {
        State::Published => style.green(label),
        State::Partial => style.yellow(label),
        State::Broken => style.red(label),
        State::Unpublished | State::NoEntrypoints | State::Absent | State::Unknown => {
            style.dim(label)
        }
    }
}

/// A path with the home directory folded back to `~`.
fn short(paths: &Paths, path: &camino::Utf8Path) -> String {
    path.strip_prefix(&paths.home)
        .map_or_else(|_| path.to_string(), |rest| format!("~/{rest}"))
}

fn status(out: &mut impl Write, cx: &Cx, name: Option<&str>) -> Result<u8> {
    // An external relic is a legitimate answer to `status`, and only to
    // `status`: everything else here would have to reach into its repository.
    if let Some(name) = name
        && lane::find(&cx.paths, name).is_none()
        && let Some(found) = cx.external(name)
    {
        return status_external(out, cx, &found);
    }
    let relic = cx.aim("status", name)?;
    let manifest = match &relic.manifest {
        Ok(manifest) => manifest,
        Err(problem) => bail!("{problem}"),
    };
    let registry = cx.registry();
    let names = manifest.published_names(&relic.dir);

    writeln!(
        out,
        "{} {}",
        cx.style.bold(relic.slug()),
        cx.style.dim(relic.dir.as_str())
    )?;
    writeln!(out, "  stage:     2 (in-house)")?;
    writeln!(out, "  runtime:   {}", manifest.runtime)?;
    writeln!(out, "  published: {}", State::of(&registry, &names).plain())?;
    for name in &names {
        if registry.has(name) {
            let owner = registry
                .owner(name)
                .map(|o| format!(" {}", cx.style.dim(&format!("(owner: {o})"))))
                .unwrap_or_default();
            writeln!(out, "    - {name} → ~/.local/bin/{name}{owner}")?;
        } else {
            writeln!(out, "    - {name} → {}", cx.style.dim("not on PATH"))?;
        }
    }
    match publish::check_deps(manifest) {
        Ok(()) => writeln!(out, "  deps:      {}", cx.style.green("ok"))?,
        Err(report) => {
            writeln!(out, "  deps:      {}", cx.style.red("missing"))?;
            for line in report.lines() {
                writeln!(out, "    {line}")?;
            }
        }
    }
    Ok(CLEAN)
}

fn status_external(out: &mut impl Write, cx: &Cx, found: &External) -> Result<u8> {
    writeln!(
        out,
        "{} {}",
        cx.style.bold(&found.name),
        cx.style.dim(found.path.as_str())
    )?;
    writeln!(out, "  stage:     3 (external)")?;
    if !found.path.is_dir() {
        writeln!(
            out,
            "  present:   {}",
            cx.style
                .dim("no (listed in GRADUATION.md, not on this machine)")
        )?;
        writeln!(out, "  manage:    in its own repo — {}", found.path)?;
        return Ok(CLEAN);
    }
    writeln!(out, "  present:   yes")?;
    if let Some(git) = relic_core::git::detect() {
        let dirty = git
            .capture(git.at(&found.path).args(["status", "--porcelain"]))
            .map(|out| !out.stdout.trim().is_empty());
        if let Ok(dirty) = dirty {
            let word = if dirty {
                cx.style.yellow("dirty")
            } else {
                cx.style.green("clean")
            };
            writeln!(out, "  git:       {word}")?;
        }
    }
    let registry = cx.registry();
    let state = if registry.knows_owner(&found.name) {
        State::Published
    } else {
        State::Unknown
    };
    writeln!(
        out,
        "  published: {} {}",
        state.plain(),
        cx.style.dim("(best-effort, by owner column)")
    )?;
    for name in registry.owned_by(&found.name) {
        writeln!(out, "    - {name} → ~/.local/bin/{name}")?;
    }
    writeln!(out, "  manage:    in its own repo — {}", found.path)?;
    Ok(CLEAN)
}

/// Run one operation over every in-house relic in both lanes.
///
/// **What bootstrap and `up` reach for**, which is why one relic's failure does
/// not stop the rest: a machine with nine relics published and one broken is a
/// machine you can work on, and a periodic update that abandoned the remaining
/// relics on the first failure would leave the machine worse for having run.
fn each(
    out: &mut impl Write,
    cx: &Cx,
    op: &str,
    run: fn(&Paths, &Relic) -> Result<(), publish::Error>,
) -> Result<u8> {
    let relics = lane::all(&cx.paths);
    warn_unreadable(&relics);
    let mut failed = 0;
    for relic in &relics {
        if let Err(error) = run(&cx.paths, relic) {
            let _ = writeln!(anstream::stderr(), "  {op} failed: {} — {error}", relic.dir);
            failed += 1;
        }
    }
    if failed > 0 {
        writeln!(out, "{failed} relic(s) failed to {op}")?;
        return Ok(REFUSED);
    }
    Ok(CLEAN)
}

fn scaffold_relic(
    out: &mut impl Write,
    cx: &Cx,
    name: &Name,
    runtime: Option<Runtime>,
    exempt: Option<&str>,
) -> Result<u8> {
    if lane::find(&cx.paths, name.as_str()).is_some()
        || cx.paths.public.join(name.as_str()).exists()
    {
        bail!("a relic named '{name}' already exists");
    }
    let dir = cx.paths.public.join(name.as_str());

    // Promotion source: an existing Stage-1 one-shot.
    let source = cx.paths.bin.join(name.as_str());
    let source = source.is_file().then_some(source);

    // Explicit flag → a promoted script's shebang → the default. Inference
    // reads what the script *is*, so it is not overridden by the stance: a
    // rewrite is a deliberate `-r rust`, not a silent one that leaves the old
    // script unrunnable.
    let runtime = match (runtime, source.as_ref()) {
        (Some(chosen), _) => chosen,
        (None, Some(script)) => match scaffold::infer_runtime(script) {
            Some(inferred) => {
                writeln!(
                    out,
                    "{}",
                    cx.style
                        .dim(&format!("inferred runtime '{inferred}' from {script}"))
                )?;
                inferred
            }
            None => default_runtime(out, cx)?,
        },
        (None, None) => default_runtime(out, cx)?,
    };

    // The stance, enforced at the one moment it is cheap to follow.
    if !runtime.is_compiled() && exempt.is_none_or(str::is_empty) {
        bail!(
            "runtime '{runtime}' needs a reason: pass --exempt \"<why this one is not Rust>\" \
             (see GRADUATION.md)"
        );
    }

    lay_down(cx, name, &dir, runtime, source.as_deref(), exempt)?;
    writeln!(
        out,
        "{} {dir} {}",
        cx.style.green("scaffolded"),
        cx.style.dim(&format!("(runtime: {runtime})"))
    )?;

    if runtime.is_compiled() {
        writeln!(
            out,
            "{}",
            cx.style.dim(&format!(
                "added to the workspace members in {}/Cargo.toml",
                cx.paths.public
            ))
        )?;
        writeln!(out)?;
        writeln!(out, "next steps:")?;
        writeln!(out, "  1. write {dir}/src/main.rs")?;
        if let Some(script) = &source {
            writeln!(
                out,
                "     {}",
                cx.style.yellow(&format!(
                    "{script} moved to src/{name}.port-me — port it, then delete it"
                ))
            )?;
        }
        writeln!(out, "  2. relic test {name}")?;
        writeln!(out, "  3. relic publish {name}")?;
        writeln!(
            out,
            "{}",
            cx.style
                .dim("staging is left until there's something publishable.")
        )?;
        return Ok(CLEAN);
    }

    if let Some(script) = &source {
        writeln!(
            out,
            "promoted {} from {script} → src/{name}",
            cx.style.bold(name.as_str())
        )?;
        let relic = lane::find(&cx.paths, name.as_str())
            .ok_or_else(|| anyhow::anyhow!("scaffolded {dir} is not readable"))?;
        publish::publish(&cx.paths, &relic)?;
        stage_in_yadm(out, cx, script, &dir);
        writeln!(out)?;
        return status(out, cx, Some(name.as_str()));
    }

    writeln!(out)?;
    writeln!(
        out,
        "next steps {}:",
        cx.style.dim("(fresh relic — no Stage-1 source found)")
    )?;
    writeln!(out, "  1. add your executable under {dir}/src/")?;
    writeln!(out, "  2. ln -s ../src/<file> {dir}/entrypoints/{name}")?;
    writeln!(out, "  3. relic publish {name}")?;
    writeln!(
        out,
        "{}",
        cx.style
            .dim("staging is left until there's something publishable.")
    )?;
    Ok(CLEAN)
}

/// Say which runtime the stance gives, and give it.
fn default_runtime(out: &mut impl Write, cx: &Cx) -> Result<Runtime> {
    writeln!(
        out,
        "{}",
        cx.style
            .dim("runtime 'rust' (the default — relics are Rust unless exempted)")
    )?;
    Ok(Runtime::Rust)
}

/// Build the tree from the template.
fn lay_down(
    cx: &Cx,
    name: &Name,
    dir: &Utf8PathBuf,
    runtime: Runtime,
    source: Option<&camino::Utf8Path>,
    exempt: Option<&str>,
) -> Result<()> {
    scaffold::copy_tree(&cx.paths.template, dir)?;
    let manifest = dir.join("relic.toml");
    let mut body = fs_err::read_to_string(manifest.as_std_path())?;
    body = scaffold::set_field(&body, "name", name.as_str());
    body = scaffold::set_field(&body, "runtime", runtime.as_str());
    if let Some(why) = exempt.filter(|w| !w.is_empty()) {
        body = scaffold::set_field(&body, "runtime-exemption", why);
    }
    fs_err::write(manifest.as_std_path(), body)?;
    fs_err::write(
        dir.join("CLAUDE.md").as_std_path(),
        scaffold::claude_md(name.as_str()),
    )?;

    if runtime.is_compiled() {
        // A Stage-1 script cannot be promoted into a compiled relic as-is; it
        // is kept beside the skeleton as the thing to port, and the caller
        // says so.
        if let Some(script) = source {
            fs_err::rename(
                script.as_std_path(),
                dir.join(format!("src/{name}.port-me")).as_std_path(),
            )?;
        }
        fs_err::write(
            dir.join("Cargo.toml").as_std_path(),
            scaffold::cargo_manifest(name.as_str()),
        )?;
        fs_err::write(
            dir.join("src/main.rs").as_std_path(),
            scaffold::cargo_main(name.as_str()),
        )?;
        let _ = fs_err::remove_file(dir.join("src/.gitkeep").as_std_path());
        let _ = fs_err::remove_dir_all(dir.join("entrypoints").as_std_path());

        let workspace = cx.paths.public.join("Cargo.toml");
        if workspace.is_file() {
            let body = fs_err::read_to_string(workspace.as_std_path())?;
            fs_err::write(
                workspace.as_std_path(),
                scaffold::add_member(&body, name.as_str()),
            )?;
        }
        return Ok(());
    }

    if let Some(script) = source {
        let landed = dir.join(format!("src/{name}"));
        fs_err::rename(script.as_std_path(), landed.as_std_path())?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mut mode = fs_err::metadata(landed.as_std_path())?.permissions();
            mode.set_mode(mode.mode() | 0o755);
            std::fs::set_permissions(landed.as_std_path(), mode)?;
        }
        let _ = fs_err::remove_file(dir.join("src/.gitkeep").as_std_path());
        let _ = fs_err::remove_file(dir.join("entrypoints/.gitkeep").as_std_path());
        scaffold::symlink(
            &format!("../src/{name}"),
            &dir.join("entrypoints").join(name.as_str()),
        )?;
    }
    Ok(())
}

/// Stage the result in yadm: the new tree, plus the moved Stage-1 path's
/// deletion when that path was tracked.
///
/// Best-effort and independent per path — a missing yadm, an untracked source,
/// or a non-yadm HOME must not fail a scaffold that already stands on disk.
/// **Stages only**; the commit is deliberate.
fn stage_in_yadm(out: &mut impl Write, cx: &Cx, old: &camino::Utf8Path, dir: &Utf8PathBuf) {
    let Some(yadm) = relic_core::tool::Tool::find("yadm") else {
        return;
    };
    let budget = std::time::Duration::from_secs(120);
    let mut staged = false;
    let mut add = yadm.command();
    add.arg("add").arg(dir.as_std_path());
    if yadm.run_within(&mut add, budget).is_ok_and(|e| e.ok()) {
        staged = true;
    }
    let mut tracked = yadm.command();
    tracked
        .args(["ls-files", "--error-unmatch"])
        .arg(old.as_std_path());
    if yadm.run_within(&mut tracked, budget).is_ok_and(|e| e.ok()) {
        let mut remove = yadm.command();
        // `-A` records the deletion.
        remove.args(["add", "-A"]).arg(old.as_std_path());
        if yadm.run_within(&mut remove, budget).is_ok_and(|e| e.ok()) {
            staged = true;
        }
    }
    if staged {
        let _ = writeln!(
            out,
            "{}",
            cx.style
                .dim(&format!("staged in yadm: {}", short(&cx.paths, dir)))
        );
    } else {
        let _ = writeln!(
            anstream::stderr(),
            "warn: could not stage in yadm; stage manually if this HOME is yadm-tracked"
        );
    }
}

fn show_registry(out: &mut impl Write, cx: &Cx) -> Result<u8> {
    let path = cx.paths.registry();
    let registry = Registry::load(&path);
    if registry.is_empty() && !path.is_file() {
        writeln!(
            out,
            "{}",
            cx.style
                .dim(&format!("registry empty — {path} does not exist yet"))
        )?;
        return Ok(CLEAN);
    }
    heading(out, cx.style, "PATH registry", path.as_str())?;
    for entry in registry.iter() {
        writeln!(
            out,
            "  {} {}",
            pad(&entry.name, 20),
            entry.owner.as_deref().unwrap_or("-")
        )?;
    }
    Ok(CLEAN)
}

fn fold(out: &mut impl Write, cx: &Cx) -> Result<u8> {
    publish::registry_helper(&cx.paths, "install_on_path_migrate_registries")?;
    writeln!(
        out,
        "folded any legacy per-meta registries into {}",
        cx.paths.registry()
    )?;
    Ok(CLEAN)
}

fn examine(out: &mut impl Write, cx: &Cx) -> Result<u8> {
    let relics = lane::all(&cx.paths);
    warn_unreadable(&relics);
    let registry = cx.registry();
    let report = doctor::Report::gather(&cx.paths, &relics, &registry);

    heading(
        out,
        cx.style,
        "Orphan registry entries",
        "(registered, no file in ~/.local/bin)",
    )?;
    if report.orphans.is_empty() {
        writeln!(out, "  {}", cx.style.green("(none)"))?;
    } else {
        for (name, owner) in &report.orphans {
            let owner = owner
                .as_deref()
                .map(|o| format!(" {}", cx.style.dim(&format!("(owner: {o})"))))
                .unwrap_or_default();
            writeln!(out, "  {}{owner}", cx.style.yellow(name))?;
        }
        writeln!(out, "  {}", cx.style.dim("fix: relic registry --prune"))?;
    }

    writeln!(out)?;
    heading(
        out,
        cx.style,
        "Unpublished entrypoints",
        "(declared by a relic, not in registry)",
    )?;
    if report.unpublished.is_empty() {
        writeln!(out, "  {}", cx.style.green("(none)"))?;
    } else {
        for (relic, entrypoint) in &report.unpublished {
            writeln!(
                out,
                "  {} {}",
                cx.style.yellow(entrypoint),
                cx.style.dim(&format!("({relic})"))
            )?;
        }
        writeln!(out, "  {}", cx.style.dim("fix: relic publish <relic>"))?;
    }

    writeln!(out)?;
    heading(
        out,
        cx.style,
        "Runtime stance",
        "(not rust, no runtime-exemption — informational)",
    )?;
    if report.stance.is_empty() {
        writeln!(out, "  {}", cx.style.green("(none)"))?;
    } else {
        for (name, runtime) in &report.stance {
            writeln!(out, "  {}", cx.style.dim(&format!("{name} ({runtime})")))?;
        }
        writeln!(
            out,
            "  {}",
            cx.style
                .dim("rewrite into the workspace, or record why not in the manifest")
        )?;
    }

    writeln!(out)?;
    heading(
        out,
        cx.style,
        "Unmanaged files",
        "(in ~/.local/bin, not in registry — informational)",
    )?;
    if report.unmanaged.is_empty() {
        writeln!(out, "  {}", cx.style.green("(none)"))?;
    } else {
        for name in &report.unmanaged {
            writeln!(out, "  {}", cx.style.dim(name))?;
        }
    }

    writeln!(out)?;
    let problems = report.problems();
    if problems == 0 {
        writeln!(
            out,
            "{} — registry, PATH lane, and entrypoints agree",
            cx.style.green("healthy")
        )?;
        return Ok(CLEAN);
    }
    writeln!(
        out,
        "{}",
        cx.style.yellow(&format!("{problems} issue(s) found"))
    )?;
    Ok(REFUSED)
}

/// The ratchet module is the gate's, and is re-exported for the tests that pin
/// its arithmetic.
const _: fn(&str) -> u64 = ratchet::count_in;
