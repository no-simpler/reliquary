//! Whether two GitHub accounts on one machine are still kept apart.
//!
//! A personal account and a benefactor account share `github.com`, and three
//! independent mechanisms separate them: which SSH key is offered, which git
//! identity signs, and which `gh` token is used. **All three must be right, and
//! any one of them can be right while another silently is not** — which is why
//! this is a station rather than a habit.
//!
//! What failure looks like is why the grades are hard. Both SSH keys are valid
//! GitHub credentials, so without the pin the *first key an agent offers* decides
//! which account you are, and neither agent's offer order is declared policy. A
//! commit lands under the wrong name, a push lands on the wrong account, and
//! nothing reports an error — the operation succeeds as somebody else.
//!
//! **Structure, never values.** This file is publicly tracked, so it names only
//! what `~/.config/CLAUDE.md` already names in public: the two host aliases and
//! the shape of the wiring. It reads no identity out of a file and puts none in a
//! finding; the tests it applies are "pinned", "different from each other", and
//! "the thing it points at exists".
//!
//! **`ssh -G` is the oracle, not the config file.** It is ssh's own resolution of
//! its own configuration, emitted as one lowercase keyword and value per line —
//! machine-readable, offline, and side-effect-free. Parsing `~/.ssh/config`
//! instead would be a second implementation of `Host` matching, `Match` blocks
//! and first-match-wins, which is how a checker comes to disagree with the thing
//! it checks.
//!
//! **What it cannot prove.** `ssh -T` says which account a key authenticates as,
//! and only a real repository operation proves organisation SSO access. Both need
//! the network and one needs a credential, so neither belongs in a detect-only
//! standing audit. This station proves the wiring is present and separate; the
//! wiring being *correct* is the network's answer.

use anyhow::Result;
use camino::Utf8PathBuf;

use relic_core::finding::{Detail, Finding, FixHint, Location, Outcome, StationId, Summary};
use relic_core::tool::Tool;

use crate::station::{Context, Station};

/// The host every personal repository is reached through.
const PERSONAL: &str = "github.com";

/// The alias that carries the benefactor identity. Same `HostName`, different
/// key — which is the whole of the mechanism.
const BENEFACTOR: &str = "github-benefactor";

/// The shim that selects a `gh` profile by working directory, `$HOME`-relative.
const GH_SHIM: &str = ".config/bin/gh";

/// Where ssh's own answer is asked for.
const SSH: &str = "ssh";

/// Where git's own answer is asked for.
const GIT: &str = "git";

/// How long ssh and git have to answer about their own configuration.
///
/// Generous: `~/.ssh/config` carries a `Match exec` predicate, so resolving a
/// host runs a program. Measured at 190 ms on this machine.
const BUDGET: std::time::Duration = std::time::Duration::from_secs(10);

/// One host's effective ssh configuration.
struct Resolved {
    /// What `HostName` it dials.
    hostname: Option<String>,
    /// Whether only the pinned identities are offered.
    identities_only: bool,
    /// The identities pinned, in the order ssh would offer them.
    identity_files: Vec<String>,
}

/// The station.
pub struct GitIdentity {
    id: StationId,
}

impl Default for GitIdentity {
    fn default() -> Self {
        Self {
            id: StationId::from_static("git-identity"),
        }
    }
}

impl Station for GitIdentity {
    fn id(&self) -> &StationId {
        &self.id
    }

    fn title(&self) -> &'static str {
        "the two GitHub identities are still pinned apart in all three mechanisms"
    }

    fn check(&self, cx: &Context) -> Result<Outcome> {
        let mut findings = Vec::new();
        match self.ssh_keys(cx) {
            Ok(Some(more)) => findings.extend(more),
            Ok(None) => {
                return Ok(Outcome::Skipped(Summary::lossy(
                    "ssh is not on the search path, so nothing can resolve a host",
                )));
            }
            Err(reason) => findings.push(self.id.broken(Summary::lossy(&reason))),
        }
        findings.extend(self.git_identity(cx));
        findings.extend(self.gh_profile(cx));
        Ok(Outcome::Ran(findings))
    }
}

impl GitIdentity {
    /// Mechanism one: which key is offered to which host.
    fn ssh_keys(&self, cx: &Context) -> Result<Option<Vec<Finding>>, String> {
        let Some(ssh) = find(cx, SSH) else {
            return Ok(None);
        };

        let personal = resolve(&ssh, PERSONAL)?;
        let benefactor = resolve(&ssh, BENEFACTOR)?;
        let mut findings = Vec::new();

        for (alias, resolved) in [(PERSONAL, &personal), (BENEFACTOR, &benefactor)] {
            if !resolved.identities_only {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "{alias} does not set IdentitiesOnly, so the agent's offer order picks the account"
                        )))
                        .detailed_with(Detail::new(
                            "Both keys are valid GitHub credentials. Without the pin, whichever \
                             the agent offers first decides who you are, and no agent's offer \
                             order is declared policy.",
                        ))
                        .fixed_by(FixHint::lossy(&format!(
                            "add `IdentitiesOnly yes` to the {alias} block in ~/.ssh/config"
                        ))),
                );
            }
            if resolved.identity_files.is_empty() {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!("{alias} pins no identity at all")))
                        .fixed_by(FixHint::lossy(&format!(
                            "add an `IdentityFile` to the {alias} block in ~/.ssh/config"
                        ))),
                );
            }
            for file in &resolved.identity_files {
                let at = Utf8PathBuf::from(expand(cx, file));
                if !at.exists() {
                    findings.push(
                        self.id
                            .broken(Summary::lossy(&format!(
                                "{alias} pins an identity file that is not on this machine"
                            )))
                            .at(Location::file(at))
                            .detailed_with(Detail::new(
                                "With IdentitiesOnly, an absent pin means no key is offered at \
                                 all, and every SSH git operation for that host fails. The keys \
                                 ride the encrypted archive: `yadm decrypt`.",
                            ))
                            .fixed_by(FixHint::lossy("yadm decrypt")),
                    );
                }
            }
        }

        if benefactor.hostname.as_deref() != Some(PERSONAL) {
            findings.push(
                self.id
                    .broken(Summary::lossy(&format!(
                        "{BENEFACTOR} does not dial {PERSONAL}"
                    )))
                    .fixed_by(FixHint::lossy(&format!(
                        "the alias exists to reach {PERSONAL} with the other key; set `HostName {PERSONAL}`"
                    ))),
            );
        }

        if !personal.identity_files.is_empty()
            && personal.identity_files == benefactor.identity_files
        {
            findings.push(
                self.id
                    .broken(Summary::lossy(
                        "both host aliases pin the same identity, so they are the same account",
                    ))
                    .fixed_by(FixHint::lossy(
                        "pin a different key per alias in ~/.ssh/config",
                    )),
            );
        }

        Ok(Some(findings))
    }

    /// Mechanism two: which identity signs, selected by directory.
    fn git_identity(&self, cx: &Context) -> Vec<Finding> {
        let Some(git) = find(cx, GIT) else {
            return vec![self.id.note(Summary::lossy(
                "git is not on the search path, so its identity wiring cannot be read",
            ))];
        };

        let listed = match config_of(&git, None) {
            Ok(listed) => listed,
            Err(reason) => return vec![self.id.broken(Summary::lossy(&reason))],
        };

        let includes: Vec<&(String, String)> = listed
            .iter()
            // git lowercases every key it lists, so this is a suffix match on a
            // key, not a filename extension.
            .filter(|(key, _)| {
                key.starts_with("includeif.gitdir:") && key.rsplit('.').next() == Some("path")
            })
            .collect();
        if includes.is_empty() {
            return vec![
                self.id
                    .broken(Summary::lossy(
                        "no directory-scoped git identity is configured",
                    ))
                    .detailed_with(Detail::new(
                        "Without an `includeIf gitdir:` the benefactor tree commits under the \
                         personal name, email and signing key, and nothing reports an error.",
                    ))
                    .fixed_by(FixHint::lossy(
                        "add an `[includeIf \"gitdir:…\"]` section to ~/.config/git/config",
                    )),
            ];
        }

        let top = value(&listed, "user.email");
        let mut findings = Vec::new();
        for (key, path) in includes {
            let scope = key
                .trim_start_matches("includeif.gitdir:")
                .trim_end_matches(".path");
            let at = Utf8PathBuf::from(expand(cx, path));
            if !at.is_file() {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "the git identity for {scope} points at a file that is not here"
                        )))
                        .at(Location::file(at))
                        .fixed_by(FixHint::lossy(
                            "it rides the encrypted archive: `yadm decrypt`",
                        )),
                );
                continue;
            }
            let scoped = match config_of(&git, Some(&at)) {
                Ok(scoped) => scoped,
                Err(reason) => {
                    findings.push(self.id.broken(Summary::lossy(&reason)));
                    continue;
                }
            };
            let scoped_email = value(&scoped, "user.email");
            if scoped_email.is_none() {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "the git identity for {scope} sets no address of its own"
                        )))
                        .at(Location::file(at.clone()))
                        .fixed_by(FixHint::lossy("set `user.email` in it")),
                );
            } else if scoped_email == top {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "the git identity for {scope} is the same address as the default one"
                        )))
                        .at(Location::file(at.clone()))
                        .detailed_with(Detail::new(
                            "The include is doing nothing: commits in that tree are authored as \
                             the default identity.",
                        )),
                );
            }
            if value(&scoped, "user.signingkey").is_none() {
                findings.push(
                    self.id
                        .broken(Summary::lossy(&format!(
                            "the git identity for {scope} signs with the default key"
                        )))
                        .at(Location::file(at))
                        .fixed_by(FixHint::lossy("set `user.signingkey` in it")),
                );
            }
        }
        findings
    }

    /// Mechanism three: which `gh` token, selected by working directory.
    fn gh_profile(&self, cx: &Context) -> Vec<Finding> {
        let shim = cx.at(GH_SHIM);
        if !shim.is_file() {
            return vec![
                self.id
                    .broken(Summary::lossy(
                        "the gh shim is not installed, so gh uses one account everywhere",
                    ))
                    .at(Location::file(shim))
                    .detailed_with(Detail::new(
                        "Selecting the account by hand is global mutable state, and one \
                         forgotten switch acts on benefactor repositories as the personal user.",
                    )),
            ];
        }

        let Ok(text) = fs_err::read_to_string(&shim) else {
            return vec![
                self.id
                    .broken(Summary::lossy("the gh shim could not be read"))
                    .at(Location::file(shim)),
            ];
        };
        let Some(dir) = config_dir(&text) else {
            return vec![
                self.id
                    .broken(Summary::lossy(
                        "the gh shim exports no profile directory, so it selects nothing",
                    ))
                    .at(Location::file(shim)),
            ];
        };

        let at = Utf8PathBuf::from(expand(cx, &dir));
        if at.is_dir() {
            return Vec::new();
        }
        vec![
            self.id
                .soft(Summary::lossy(
                    "the gh profile the shim selects has not been created yet",
                ))
                .at(Location::file(at))
                .detailed_with(Detail::new(
                    "The one interactive step on a new machine. Until it is done, gh inside that \
                     tree has no token and falls back to the account with no access there — \
                     fail-safe, and not yet working.",
                ))
                .fixed_by(FixHint::lossy(
                    "run `gh auth login` from inside the tree, so the shim points it at the right \
                     profile",
                )),
        ]
    }
}

/// A program on the injected search path.
fn find(cx: &Context, name: &str) -> Option<Tool> {
    cx.path()
        .iter()
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
        .map(|candidate| Tool::at_path(name, candidate.into_std_path_buf()))
}

/// Ask ssh how it resolves a host.
///
/// `-G` is ssh's own answer about its own configuration: one lowercase keyword
/// and value per line, and every `Host`, `Match` and first-match-wins rule
/// already applied. Reading `~/.ssh/config` instead would be a second
/// implementation of all of that.
fn resolve(ssh: &Tool, host: &str) -> Result<Resolved, String> {
    let mut command = ssh.command();
    command.arg("-G").arg(host);
    let exit = ssh
        .run_within(&mut command, BUDGET)
        .map_err(|error| format!("ssh could not resolve {host}: {error}"))?;
    if !exit.ok() {
        return Err(format!(
            "ssh could not resolve {host}: {}",
            exit.stderr.trim()
        ));
    }

    let mut resolved = Resolved {
        hostname: None,
        identities_only: false,
        identity_files: Vec::new(),
    };
    for line in exit.stdout.lines() {
        let Some((key, rest)) = line.split_once(' ') else {
            continue;
        };
        match key {
            "hostname" => resolved.hostname = Some(rest.to_owned()),
            "identitiesonly" => resolved.identities_only = rest == "yes",
            "identityfile" => resolved.identity_files.push(rest.to_owned()),
            _ => {}
        }
    }
    Ok(resolved)
}

/// Ask git for a configuration, as key/value pairs.
///
/// `-z` because a value may hold a newline, and the separator has to be one the
/// value cannot contain. `--includes` is deliberately absent: an `includeIf` is
/// applied by working directory, and this station is asking what the rules *are*,
/// not what they resolve to from wherever it happens to run.
fn config_of(git: &Tool, file: Option<&camino::Utf8Path>) -> Result<Vec<(String, String)>, String> {
    let mut command = git.command();
    command.arg("config");
    match file {
        Some(file) => {
            command.arg("--file").arg(file.as_str());
        }
        None => {
            command.arg("--global");
        }
    }
    command.args(["--list", "-z"]);

    let exit = git
        .run_within(&mut command, BUDGET)
        .map_err(|error| format!("git could not read its configuration: {error}"))?;
    if !exit.ok() {
        return Err(format!(
            "git could not read its configuration: {}",
            exit.stderr.trim()
        ));
    }
    Ok(exit
        .stdout
        .split('\0')
        .filter(|record| !record.is_empty())
        .map(|record| match record.split_once('\n') {
            Some((key, value)) => (key.to_owned(), value.to_owned()),
            None => (record.to_owned(), String::new()),
        })
        .collect())
}

/// The last value for a key, which is the one git would use.
fn value<'a>(listed: &'a [(String, String)], key: &str) -> Option<&'a str> {
    listed
        .iter()
        .rev()
        .find(|(candidate, _)| candidate == key)
        .map(|(_, value)| value.as_str())
}

/// A leading `~` as the home being checked, so a path is compared against the
/// machine the station was pointed at rather than the one it runs on.
fn expand(cx: &Context, path: &str) -> String {
    match path.strip_prefix("~/") {
        Some(rest) => cx.at(rest).into_string(),
        None => path.to_owned(),
    }
}

/// The profile directory the shim exports.
///
/// The shim is shell, so the value is read the only way a non-shell can read it:
/// the assignment as written. A shim that computed the path would be reported as
/// exporting nothing, which is the honest answer — this cannot follow shell.
fn config_dir(text: &str) -> Option<String> {
    text.lines()
        .filter_map(|line| line.trim().strip_prefix("export GH_CONFIG_DIR="))
        .map(|value| {
            value
                .trim()
                .trim_matches(|c| c == '"' || c == '\'')
                .replace("$HOME/", "~/")
                .replace("${HOME}/", "~/")
        })
        .find(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use camino::Utf8Path;
    use relic_core::finding::Severity;

    use super::*;

    /// A machine whose `ssh` and `git` are shims answering from files.
    ///
    /// Shimmed rather than run for real, for the reason `shell-lint` shims
    /// `shellcheck`: a test that calls the machine's own `ssh` answers for that
    /// machine's configuration, which is the one thing a fixture must not do.
    /// The tools' *contracts* — `ssh -G`'s keyword lines and `git config -z`'s
    /// NUL records — are pinned separately, against output captured from them.
    struct Machine {
        _dir: tempfile::TempDir,
        home: Utf8PathBuf,
        bin: Utf8PathBuf,
    }

    impl Machine {
        /// Everything wired correctly. Each test varies one thing.
        fn sound() -> Self {
            let dir = tempfile::tempdir().expect("a scratch dir");
            let home =
                Utf8PathBuf::from_path_buf(dir.path().to_path_buf()).expect("a utf-8 scratch dir");
            let bin = home.join("bin");
            fs_err::create_dir_all(&bin).expect("a bin dir");
            let machine = Self {
                _dir: dir,
                home,
                bin,
            };
            machine
                .key("personal")
                .key("benefactor")
                .ssh(
                    "hostname github.com\\nidentitiesonly yes\\nidentityfile ~/personal",
                    "hostname github.com\\nidentitiesonly yes\\nidentityfile ~/benefactor",
                )
                .global("user.email\nme@example.test")
                .scoped("user.email\nwork@example.test\0user.signingkey\nssh-ed25519 AAAA")
                .gh_shim("export GH_CONFIG_DIR=\"$HOME/.config/gh-benefactor\"")
                .gh_profile();
            machine
        }

        fn write(at: &Utf8Path, body: &str, mode: u32) {
            if let Some(parent) = at.parent() {
                fs_err::create_dir_all(parent).expect("a parent");
            }
            fs_err::write(at, body).expect("written");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt as _;
                fs_err::set_permissions(at, std::fs::Permissions::from_mode(mode)).expect("a mode");
            }
        }

        fn key(&self, name: &str) -> &Self {
            Self::write(&self.home.join(name), "ssh-ed25519 AAAA\n", 0o600);
            self
        }

        /// An `ssh` that answers `-G` from two canned resolutions.
        fn ssh(&self, personal: &str, benefactor: &str) -> &Self {
            Self::write(
                &self.bin.join(SSH),
                &format!(
                    "#!/bin/sh\ncase \"$2\" in\n  {PERSONAL}) printf '{personal}\\n' ;;\n  \
                     {BENEFACTOR}) printf '{benefactor}\\n' ;;\nesac\n"
                ),
                0o755,
            );
            self
        }

        /// A `git` that answers `config --list -z` from two canned files.
        fn git(&self) -> &Self {
            Self::write(
                &self.bin.join(GIT),
                &format!(
                    "#!/bin/sh\nfor a in \"$@\"; do\n  case \"$a\" in --file) f=1 ;; \
                     *) [ \"$f\" = 1 ] && [ -z \"$t\" ] && t=\"$a\" ;; esac\ndone\n\
                     if [ -n \"$t\" ]; then cat \"{home}/scoped\"; else cat \"{home}/global\"; fi\n",
                    home = self.home
                ),
                0o755,
            );
            self
        }

        fn global(&self, records: &str) -> &Self {
            Self::write(
                &self.home.join("global"),
                &format!(
                    "{records}\0includeif.gitdir:~/work/.path\n{}/scoped-config\0",
                    self.home
                ),
                0o644,
            );
            self.git()
        }

        fn scoped(&self, records: &str) -> &Self {
            Self::write(&self.home.join("scoped-config"), "[user]\n", 0o644);
            Self::write(&self.home.join("scoped"), &format!("{records}\0"), 0o644);
            self
        }

        fn gh_shim(&self, body: &str) -> &Self {
            Self::write(
                &self.home.join(GH_SHIM),
                &format!("#!/bin/bash\n{body}\n"),
                0o755,
            );
            self
        }

        fn gh_profile(&self) -> &Self {
            fs_err::create_dir_all(self.home.join(".config/gh-benefactor")).expect("a profile");
            self
        }

        fn findings(&self) -> Vec<Finding> {
            let outcome = GitIdentity::default()
                .check(&Context::new(self.home.clone(), vec![self.bin.clone()]))
                .expect("the station ran");
            match outcome {
                Outcome::Ran(findings) => findings,
                Outcome::Skipped(reason) => panic!("unexpectedly skipped: {reason}"),
            }
        }

        fn only(&self) -> Finding {
            let findings = self.findings();
            assert_eq!(findings.len(), 1, "{findings:?}");
            findings.into_iter().next().expect("one")
        }
    }

    #[test]
    fn three_mechanisms_all_wired_has_nothing_to_say() {
        assert!(Machine::sound().findings().is_empty());
    }

    #[test]
    fn a_host_that_does_not_pin_lets_the_agents_offer_order_pick_the_account() {
        let machine = Machine::sound();
        machine.ssh(
            "hostname github.com\\nidentityfile ~/personal",
            "hostname github.com\\nidentitiesonly yes\\nidentityfile ~/benefactor",
        );
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("IdentitiesOnly"),
            "{finding:?}"
        );
    }

    #[test]
    fn two_aliases_pinning_one_key_are_one_account_wearing_two_names() {
        let machine = Machine::sound();
        machine.ssh(
            "hostname github.com\\nidentitiesonly yes\\nidentityfile ~/personal",
            "hostname github.com\\nidentitiesonly yes\\nidentityfile ~/personal",
        );
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("same identity"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_pin_naming_a_key_that_is_not_here_offers_no_key_at_all() {
        let machine = Machine::sound();
        fs_err::remove_file(machine.home.join("benefactor")).expect("removed");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("not on this machine"));
        assert_eq!(
            finding.fix.as_ref().map(FixHint::as_str),
            Some("yadm decrypt"),
            "the keys ride the archive, so the remedy is the archive"
        );
    }

    #[test]
    fn an_alias_that_dials_elsewhere_is_not_the_second_account_for_this_host() {
        let machine = Machine::sound();
        machine.ssh(
            "hostname github.com\\nidentitiesonly yes\\nidentityfile ~/personal",
            "hostname elsewhere.test\\nidentitiesonly yes\\nidentityfile ~/benefactor",
        );
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("does not dial"));
    }

    #[test]
    fn no_directory_scoped_identity_means_one_name_commits_everywhere() {
        let machine = Machine::sound();
        Machine::write(
            &machine.home.join("global"),
            "user.email\nme@example.test\0",
            0o644,
        );
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("no directory-scoped"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_scoped_identity_repeating_the_default_address_is_the_include_doing_nothing() {
        let machine = Machine::sound();
        machine.scoped("user.email\nme@example.test\0user.signingkey\nssh-ed25519 AAAA");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("same address"),
            "{finding:?}"
        );
    }

    #[test]
    fn a_scoped_identity_that_signs_with_the_default_key_is_reported() {
        let machine = Machine::sound();
        machine.scoped("user.email\nwork@example.test");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(
            finding.summary.as_str().contains("default key"),
            "{finding:?}"
        );
    }

    #[test]
    fn an_include_pointing_at_a_file_that_is_not_here_says_where_it_comes_from() {
        let machine = Machine::sound();
        fs_err::remove_file(machine.home.join("scoped-config")).expect("removed");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("not here"), "{finding:?}");
    }

    #[test]
    fn no_gh_shim_means_gh_uses_one_account_everywhere() {
        let machine = Machine::sound();
        fs_err::remove_file(machine.home.join(GH_SHIM)).expect("removed");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("shim is not installed"));
    }

    #[test]
    fn a_shim_that_selects_nothing_is_a_shim_that_does_nothing() {
        let machine = Machine::sound();
        machine.gh_shim("exec gh \"$@\"");
        let finding = machine.only();
        assert_eq!(finding.severity, Severity::Broken);
        assert!(finding.summary.as_str().contains("exports no profile"));
    }

    #[test]
    fn a_profile_not_yet_created_is_soft_because_the_login_is_interactive() {
        let machine = Machine::sound();
        fs_err::remove_dir_all(machine.home.join(".config/gh-benefactor")).expect("removed");
        let finding = machine.only();
        assert_eq!(
            finding.severity,
            Severity::Soft,
            "fail-safe and not yet working is degraded, not disarmed"
        );
        assert!(
            finding
                .fix
                .as_ref()
                .is_some_and(|fix| fix.as_str().contains("gh auth login"))
        );
    }

    #[test]
    fn no_ssh_on_the_path_is_skipped_rather_than_graded_clean() {
        let machine = Machine::sound();
        fs_err::remove_file(machine.bin.join(SSH)).expect("removed");
        let outcome = GitIdentity::default()
            .check(&Context::new(
                machine.home.clone(),
                vec![machine.bin.clone()],
            ))
            .expect("the station ran");
        let Outcome::Skipped(reason) = outcome else {
            panic!("a station that cannot ask must say so");
        };
        assert!(reason.as_str().contains("ssh is not"), "{reason}");
    }

    #[test]
    fn the_ssh_g_shape_is_the_one_ssh_emits() {
        // Captured from `ssh -G` on this machine, 2026-08-29: one lowercase
        // keyword, a space, and the value. Pinned so a change in that contract
        // fails here rather than silently reporting a clean machine.
        let sample = "user git\nhostname github.com\nidentitiesonly yes\nidentityfile ~/.ssh/k";
        let mut resolved = Resolved {
            hostname: None,
            identities_only: false,
            identity_files: Vec::new(),
        };
        for line in sample.lines() {
            if let Some((key, rest)) = line.split_once(' ') {
                match key {
                    "hostname" => resolved.hostname = Some(rest.to_owned()),
                    "identitiesonly" => resolved.identities_only = rest == "yes",
                    "identityfile" => resolved.identity_files.push(rest.to_owned()),
                    _ => {}
                }
            }
        }
        assert_eq!(resolved.hostname.as_deref(), Some("github.com"));
        assert!(resolved.identities_only);
        assert_eq!(resolved.identity_files, vec!["~/.ssh/k".to_owned()]);
    }

    #[test]
    fn a_git_config_record_is_a_key_a_newline_and_a_value_that_may_hold_newlines() {
        let records = config_records("a.b\none\0c.d\ntwo\nstill two\0");
        assert_eq!(
            records,
            vec![
                ("a.b".to_owned(), "one".to_owned()),
                ("c.d".to_owned(), "two\nstill two".to_owned()),
            ],
            "-z is what makes a multi-line value readable at all"
        );
    }

    #[test]
    fn a_valueless_key_is_a_key_with_an_empty_value_and_not_a_dropped_record() {
        assert_eq!(
            config_records("a.b\0"),
            vec![("a.b".to_owned(), String::new())]
        );
    }

    /// The parse half of [`config_of`], without a subprocess.
    fn config_records(stdout: &str) -> Vec<(String, String)> {
        stdout
            .split('\0')
            .filter(|record| !record.is_empty())
            .map(|record| match record.split_once('\n') {
                Some((key, value)) => (key.to_owned(), value.to_owned()),
                None => (record.to_owned(), String::new()),
            })
            .collect()
    }
}
