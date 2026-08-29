//! Everything this program asks the daemon, and everything it tells it to do.
//!
//! Every question is asked through a **declared machine interface** — an id
//! list, or a Go template emitting JSON — and never through a message meant for
//! a person. The retired script branched on Docker's English error strings,
//! which is a check that stops checking the moment the daemon is localized or
//! reworded.

use std::collections::BTreeMap;
use std::time::Duration;

use camino::{Utf8Path, Utf8PathBuf};
use relic_core::tool::{self, Tool};

use crate::project::ProjectName;

/// The label Compose stamps a project's every resource with.
pub const PROJECT_LABEL: &str = "com.docker.compose.project";
/// The label carrying the compose files a container was raised from.
pub const CONFIG_FILES_LABEL: &str = "com.docker.compose.project.config_files";

/// The environment variable that replaces the `docker` binary, or disables it.
pub const DOCKER_OVERRIDE: &str = "COMPOSE_GC_DOCKER";

/// A question the daemon should answer at once.
const QUERY: Duration = Duration::from_secs(30);
/// Removing one resource.
const REMOVE: Duration = Duration::from_secs(120);
/// A whole stack coming down, images and all.
const TEARDOWN: Duration = Duration::from_secs(900);

/// A Compose container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Container {
    /// Its id.
    pub id: String,
    /// The project it belongs to.
    pub project: ProjectName,
    /// The directory its first compose file lived in, when it had one.
    pub config_dir: Option<Utf8PathBuf>,
}

/// A Compose volume or network: a name and the project that owns it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resource {
    /// Its name, for a volume; its id, for a network.
    pub id: String,
    /// The project it belongs to.
    pub project: ProjectName,
}

/// Everything the daemon holds for Compose, in one reading.
///
/// Read whole and grouped in memory rather than re-queried per project: three
/// invocations regardless of how many projects there turn out to be, and one
/// consistent snapshot to plan against.
#[derive(Clone, Debug, Default)]
pub struct Inventory {
    /// Every container carrying a project label.
    pub containers: Vec<Container>,
    /// Every volume carrying one.
    pub volumes: Vec<Resource>,
    /// Every network carrying one.
    pub networks: Vec<Resource>,
}

impl Inventory {
    /// Volume names belonging to `project`.
    #[must_use]
    pub fn volumes_of(&self, project: &ProjectName) -> Vec<String> {
        Self::ids_of(&self.volumes, project)
    }

    /// Network ids belonging to `project`.
    #[must_use]
    pub fn networks_of(&self, project: &ProjectName) -> Vec<String> {
        Self::ids_of(&self.networks, project)
    }

    /// Container ids belonging to `project`.
    #[must_use]
    pub fn containers_of(&self, project: &ProjectName) -> Vec<String> {
        let mut ids: Vec<String> = self
            .containers
            .iter()
            .filter(|container| &container.project == project)
            .map(|container| container.id.clone())
            .collect();
        ids.sort_unstable();
        ids
    }

    fn ids_of(resources: &[Resource], project: &ProjectName) -> Vec<String> {
        let mut ids: Vec<String> = resources
            .iter()
            .filter(|resource| &resource.project == project)
            .map(|resource| resource.id.clone())
            .collect();
        ids.sort_unstable();
        ids
    }
}

/// What a teardown attempt did.
#[derive(Clone, Debug)]
pub struct Teardown {
    /// Whether `docker compose down` exited zero.
    pub ok: bool,
    /// What it said, for a caller that has decided to report a failure.
    pub stderr: String,
}

/// The daemon, proven reachable.
#[derive(Debug)]
pub struct Docker {
    tool: Tool,
}

/// Why the daemon could not be asked.
#[derive(Debug)]
pub enum Absent {
    /// No `docker` on `PATH`.
    NotInstalled,
    /// It is installed, but it did not answer.
    Unreachable,
}

impl Docker {
    /// Resolve `docker` and prove the daemon answers.
    ///
    /// The probe is a real query rather than a version string: a client that
    /// runs and a daemon that answers are two different facts, and only the
    /// second one makes a sweep possible.
    ///
    /// # Errors
    ///
    /// [`Absent`], distinguishing a machine without Docker from one whose
    /// daemon is not running — a caller reports them differently even though
    /// neither is a failure.
    pub fn connect() -> Result<Self, Absent> {
        let tool =
            Tool::find_with_override("docker", DOCKER_OVERRIDE).ok_or(Absent::NotInstalled)?;
        let docker = Self { tool };
        let mut command = docker.tool.command();
        command.args(["ps", "-q"]);
        match docker.tool.run_within(&mut command, QUERY) {
            Ok(exit) if exit.ok() => Ok(docker),
            Ok(_) | Err(_) => Err(Absent::Unreachable),
        }
    }

    /// Read every Compose-labelled container, volume and network.
    ///
    /// # Errors
    ///
    /// When any of the three queries could not be run or refused.
    pub fn inventory(&self) -> Result<Inventory, tool::Error> {
        Ok(Inventory {
            containers: self.containers()?,
            volumes: self.resources(
                &["volume", "ls"],
                "{{.Name}} {{.Label \"com.docker.compose.project\"}}",
            )?,
            networks: self.resources(
                &["network", "ls"],
                "{{.ID}} {{.Label \"com.docker.compose.project\"}}",
            )?,
        })
    }

    /// Containers, read in two passes so no field has to be escaped.
    ///
    /// `docker ps --format` would answer in one, but its label columns are
    /// delimited text and `config_files` is itself a comma-joined list of
    /// arbitrary paths. `inspect` emits the label map as JSON, which has an
    /// answer for every byte a path can hold.
    fn containers(&self) -> Result<Vec<Container>, tool::Error> {
        let mut command = self.tool.command();
        command.args([
            "ps",
            "-a",
            "--no-trunc",
            "--filter",
            &format!("label={PROJECT_LABEL}"),
            "--format",
            "{{.ID}}",
        ]);
        let ids: Vec<String> = self
            .tool
            .capture_within(&mut command, QUERY)?
            .stdout
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(ToOwned::to_owned)
            .collect();
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut command = self.tool.command();
        command.args(["inspect", "--type", "container", "--format"]);
        command.arg("{{.Id}} {{json .Config.Labels}}");
        command.args(&ids);
        // A container that vanished between the two passes makes `inspect`
        // refuse the whole batch, so the exit status is data here: the lines it
        // did emit are still true, and each carries its own id.
        let exit = self.tool.run_within(&mut command, QUERY)?;
        Ok(exit.stdout.lines().filter_map(Self::container).collect())
    }

    /// One `inspect` line: an id, a space, and the label map as JSON.
    fn container(line: &str) -> Option<Container> {
        let (id, json) = line.split_once(' ')?;
        let labels: BTreeMap<String, String> = serde_json::from_str(json).ok()?;
        let project = ProjectName::observed(labels.get(PROJECT_LABEL)?)?;
        // Compose joins the files it read with commas; the first is the one
        // that decided the project directory.
        let config_dir = labels
            .get(CONFIG_FILES_LABEL)
            .and_then(|files| files.split(',').next())
            .filter(|first| !first.is_empty())
            .and_then(|first| Utf8Path::new(first).parent())
            .map(Utf8Path::to_owned);
        Some(Container {
            id: id.to_owned(),
            project,
            config_dir,
        })
    }

    /// Volumes or networks, whose names and project labels are both drawn from
    /// an alphabet with no spaces in it.
    fn resources(&self, verb: &[&str], format: &str) -> Result<Vec<Resource>, tool::Error> {
        let mut command = self.tool.command();
        command.args(verb);
        command.args([
            "--filter",
            &format!("label={PROJECT_LABEL}"),
            "--format",
            format,
        ]);
        Ok(self
            .tool
            .capture_within(&mut command, QUERY)?
            .stdout
            .lines()
            .filter_map(|line| {
                let (id, project) = line.trim().split_once(' ')?;
                Some(Resource {
                    id: id.to_owned(),
                    project: ProjectName::observed(project.trim())?,
                })
            })
            .collect())
    }

    /// Remove one container, forcibly, with its anonymous volumes.
    ///
    /// # Errors
    ///
    /// What the daemon said, when it refused.
    pub fn remove_container(&self, id: &str) -> Result<(), tool::Error> {
        self.remove(&["rm", "-f", "-v", id])
    }

    /// Remove one named volume.
    ///
    /// # Errors
    ///
    /// What the daemon said, when it refused.
    pub fn remove_volume(&self, name: &str) -> Result<(), tool::Error> {
        self.remove(&["volume", "rm", name])
    }

    /// Remove one network.
    ///
    /// # Errors
    ///
    /// What the daemon said, when it refused.
    pub fn remove_network(&self, id: &str) -> Result<(), tool::Error> {
        self.remove(&["network", "rm", id])
    }

    fn remove(&self, args: &[&str]) -> Result<(), tool::Error> {
        let mut command = self.tool.command();
        command.args(args);
        self.tool.capture_within(&mut command, REMOVE).map(|_| ())
    }

    /// Bring one project directory's stack down, every profile, with its
    /// volumes and any orphan it left.
    ///
    /// # Errors
    ///
    /// Only when `docker` could not be started or outlasted its budget. A
    /// non-zero exit is [`Teardown::ok`], not an error: whether it means
    /// anything depends on facts the caller holds and this does not.
    pub fn compose_down(&self, dir: &Utf8Path) -> Result<Teardown, tool::Error> {
        let mut command = self.tool.command();
        command.args([
            "compose",
            "--project-directory",
            dir.as_str(),
            "--profile",
            "*",
        ]);
        command.args(["down", "-v", "--remove-orphans"]);
        let exit = self.tool.run_within(&mut command, TEARDOWN)?;
        Ok(Teardown {
            ok: exit.ok(),
            stderr: exit.stderr,
        })
    }
}

impl Docker {
    /// The project name Compose itself would use for a directory.
    ///
    /// Asked rather than derived, because `name:` in the file and
    /// `COMPOSE_PROJECT_NAME` in the environment both outrank the directory,
    /// and a stranded volume set is matched on name alone.
    ///
    /// `None` when Compose could not read a project there, which is a fact
    /// about this program's blindness rather than about the directory.
    #[must_use]
    pub fn compose_project_name(&self, dir: &Utf8Path) -> Option<ProjectName> {
        #[derive(serde::Deserialize)]
        struct Config {
            name: String,
        }
        let mut command = self.tool.command();
        command.args(["compose", "--project-directory", dir.as_str()]);
        command.args(["config", "--format", "json"]);
        let exit = self.tool.run_within(&mut command, QUERY).ok()?;
        if !exit.ok() {
            return None;
        }
        let config: Config = serde_json::from_str(&exit.stdout).ok()?;
        ProjectName::observed(&config.name)
    }
}
