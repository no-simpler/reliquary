# Reliquary - Dotfile Repository

Public dotfile repo managed by [yadm](https://yadm.io/) (yet another dotfiles manager).
Repo: [`no-simpler/reliquary`](https://github.com/no-simpler/reliquary).

yadm is a thin git wrapper whose work tree is `$HOME` and git dir is `~/.local/share/yadm/repo.git`.
All standard git commands work via `yadm <cmd>`.
Only explicitly `yadm add`-ed files are tracked; everything else is ignored.

## Bedrock

The **bedrock** is Reliquary's guaranteed substrate — the minimal set of system-wide deps ensured
present, configured, and fully PATH-accessible (with their sub-APIs) on every machine, so every other
repo the author owns may assume it without re-checking. When a tool needs more, it dockerizes rather
than growing bedrock. Members (v1): **bash** (>=5 on PATH — macOS `/bin/bash` 3.2 is shadowed, not
used), **python3** (latest, *never* minor-pinned; brew owns the interpreter and self-heals on `brew
upgrade`; uv supplements it but doesn't own it; per-app floors live in relic manifests' `MIN_RUNTIME_VERSION`),
**uv** (+`uvx`), **docker** (full API: CLI +`compose`+`buildx`; any impl), **git**, **curl**.

- **Install:** base `brew/Brewfile`, members tagged `# bedrock` (macOS only for now; Linux is a TODO).
- **Verify:** `bin/check-bedrock` (cross-platform, offline, side-effect-free; exit 0/1/2).
- **Enforce:** wired into `yadm doctor` (so the dream pre-pass and `yadm update` cover it) and re-asserted
  at the end of bootstrap (`yadm/snippets/shared/98-bedrock.sh`).
- **Bootstrap caveat:** snippets are sourced into the running (stock 3.2) bash, which the early
  modern-bash install does *not* upgrade mid-run — so **every bootstrap snippet stays bash-3.2-safe**.

Full philosophy and the contract for sibling repos: `~/.config/reliquary/BEDROCK.md`.

## Encryption

Sensitive files are GPG-encrypted into `~/.local/share/yadm/archive` and tracked in the public repo.
Patterns are listed in `~/.config/yadm/encrypt`.
The password for `yadm encrypt`, `yadm decrypt`, and `yadm verify` is fetched from 1Password implicitly (Touch ID prompt).
The `yadm-wrapper` script (see below) tracks archive SHA256 in `~/.local/state/yadm/last_decrypted` to detect encrypt/decrypt drift.

**Convention:** Encryption patterns in `encrypt` are intentionally obfuscated — they should not reveal what they protect. When adding new patterns, use opaque names that don't hint at content. Do not describe or document the contents of encrypted files in any tracked file (including this one). Future sessions can read encrypted file contents locally after decryption.

## yadm operations

**Tracked-file discovery.** The work tree is `$HOME`, not the cwd. Tracked files live across `$HOME` — `.bashrc`, `.zshrc`, `.bash_profile`, `.zshenv`, `.bash_env`, `.config/...`, `.ssh/config`, `.local/share/yadm/archive`, etc. Don't assume a file at `$HOME` root is untracked just because it sits outside the cwd. Check tracking with `yadm ls-files <path>`. See full dirty state with `yadm status` (always reports relative to `$HOME`).

**yadm is whitelist-based (footgun).** yadm blanket-ignores `$HOME` and tracks only files explicitly `yadm add`-ed. A file you just created is therefore **not** under version control until you add it — never assume a new file is tracked; verify with `yadm ls-files <path>` and add it deliberately. Conversely, a clean `yadm status` means "nothing *tracked* changed", not "nothing worth saving". There is also no usable blanket add: `yadm add -A` / `yadm add .` will not pick up new files (they're ignored), so every new path must be named explicitly in `yadm add`.

**Encrypted files are invisible to `yadm ls-files`.** Some files are tracked only *inside* the encrypted archive (`~/.local/share/yadm/archive`), not as individual git entries — so `yadm ls-files` will not list them. The patterns in `~/.config/yadm/encrypt` are also **not** authoritative for what is actually archived (a pattern may match nothing on this machine). To enumerate exactly what is encrypted-tracked, list the archive: `yadm decrypt -l` (Touch ID). The complete tracked set = `yadm ls-files` (plaintext) **plus** that archive listing — `yadm ls-all` (wrapper subcommand) prints both in one go (Touch ID, for the archive half).

**Reading `yadm status`** (easy to misread):
- `M ` / `R `: tracked file modified or renamed → almost always belongs in the next commit
- `A `: newly staged for tracking
- `??`: untracked (file exists but isn't `yadm add`-ed; deciding to track is a separate judgment call)

**Default staging policy.** When committing in this repo, stage every `M` / `R` line unless explicitly told to exclude one. Reliquary bundles whatever is dirty; splitting the working tree by topic isn't the house style. Skipping an `M` line on the assumption it's "unrelated" has historically been wrong.

**Path availability.** `yadm` is on `$PATH` in non-interactive bash and zsh: `~/.zshenv` and `~/.bash_env` (via `$BASH_ENV`) source the `env.d/*.sh` files, which put both Homebrew and `~/.config/bin` on `$PATH`. Crucially, `env.d/999-path.sh` runs **last** (highest-numbered) and forces `~/.config/bin` **ahead** of Homebrew — it has to come after 040-env's own gcloud/cargo/OrbStack prepends and after any pre-polluted inherited `$PATH`, which previously demoted it. `~/.config/bin/yadm` is a symlink to the wrapper — so bare `yadm <cmd>` resolves to the **wrapper** (not brew's yadm) in *every* shell, interactive or not, including wrapper-only subcommands (`check`/`verify`/`update`/`own`/`disown`/`ls-all`). No alias and no explicit `~/.config/bin/yadm-wrapper` path are needed anymore. (The wrapper finds the real yadm by scanning `$PATH` outside `~/.config/bin`, so it never recurses.)

**Authorization.** `yadm commit` and `yadm push` are pre-approved — run them yourself; never ask first and never hand the commit/push off to the user to run. Be aware that **`yadm commit` triggers a Touch ID prompt** — commits are SSH-signed through 1Password (`op-ssh-sign`, per the global git config) — as does `yadm encrypt`. If the user is AFK the prompt times out and the command fails; that just means the user is away, not a real error. Surface it plainly and let them retry when back — don't thrash, retry in a loop, or try to work around the signing.

## Repository structure

### Shell configuration (`~/.config/shell/`)

Three-tier layout — the directory name *is* the contract:
- `~/.config/shell/env.d/` = **always-on** (PATH, tool init, locale, auth env). Sourced by `~/.zshenv` (every zsh) and `~/.bash_env` (every non-interactive bash via `$BASH_ENV`; also from `~/.bash_profile` for login bash). Idempotent — re-sourcing is safe.
- `~/.config/shell/interactive.d/` = **interactive only** (plugins, prompt, completions, aliases, update checks). Sourced by `~/.zshrc` / `~/.bashrc` behind their interactive gates.
- `~/.config/shell/lib/` = **explicit-source-only** (libraries that downstream callers `source` on demand; never auto-loaded). Used to centralize logic shared across meta-projects so it lives in exactly one place. Currently empty: its first occupant, `install-on-path.sh`, graduated to `~/.config/reliquary/lib/` as relic infrastructure (see "Externally-managed PATH lane" below) — the tier remains the contract for any future shared library.

Filename suffix selects shell:
- `NNN-name.sh` = shared (bash + zsh only — fish cannot parse POSIX syntax)
- `NNN-name.fish` = fish-only
- `NNN-name.zsh` = zsh-only
- `NNN-name.bash` = bash-only

Numbering controls load order:
```
env.d/         : 040-env  070-fixes  150-benefactor  999-path
interactive.d/ : 010-colors  020-plugins  030-config  050-prompt
                 060-fzf  080-check  090-funcs  100-aliases{,-git,-docker,-yadm}
```
Additional encrypted shell files may exist (see `~/.config/yadm/encrypt`).

When adding a new file, ask: does it set env / PATH that agents need (→ `env.d/`), or does it configure interactive UX (→ `interactive.d/`)? Side-effecting files (anything that prints to stdout, runs `tty`, or spawns subprocesses) belong in `interactive.d/` unless they can be made silent and idempotent.

Shell var `$D__SHELL` is set to `zsh` / `bash` / `fish` in the respective entry-point file (always, including non-interactively) and used throughout for shell-specific branching.

**Fish**: `~/.config/fish/conf.d/*.fish` is auto-loaded by fish regardless of interactivity (fish convention; no `env.d/` split). Interactive-only fish files self-gate at the top with `status is-interactive; or return`. Env-pure fish files (`040-env`, `070-fixes`, `150-benefactor`, `00-sdkman-guard`, `sdk`) intentionally have no gate.

Non-interactive entry points:
- zsh → `~/.zshenv` globs `env.d/*.sh`
- bash → `$BASH_ENV` (set to `~/.bash_env` by `~/.zshenv`) globs `env.d/*.sh`; `~/.bash_profile` also sources `~/.bash_env` for login bash
- fish → conf.d auto-load (env-pure files run unconditionally)

**Root-dotfile placement (`ZDOTDIR`).** `~/.zshenv` lives at `$HOME` root because zsh reads it when `ZDOTDIR` is unset at startup — so it's the one file that *can't* move (on a fresh login `ZDOTDIR` isn't set yet, and root `~/.zshenv` is what sets it). It exports `ZDOTDIR="$HOME/.config/zsh"`, so zsh's remaining startup files live inward as **`~/.config/zsh/.zprofile`** and **`~/.config/zsh/.zshrc`** (yadm restores them at those tracked paths on clone — no symlink/forwarder needed). **Caveat — `ZDOTDIR` can be *inherited*:** zsh reads root `~/.zshenv` *only* when `ZDOTDIR` is unset; a zsh that starts with `ZDOTDIR` already exported (a nested shell, or Claude Code's command shell launched from an environment that already set it) reads **`$ZDOTDIR/.zshenv`** instead and never touches the root file. So **`~/.config/zsh/.zshenv` also exists** and is load-bearing: it `source`s root `~/.zshenv` (keeping it authoritative) so `env.d` (PATH, tool init, locale, auth env) loads either way. Without it, inherited-`ZDOTDIR` zsh silently skips *all* of `env.d` — the bug that made bare `yadm` resolve to brew's instead of the wrapper. No double-source: a fresh login reads only root `~/.zshenv` (and setting `ZDOTDIR` mid-file doesn't trigger a second read in the same pass); an inherited start reads only `$ZDOTDIR/.zshenv`. Bash's `~/.bash_env`, `~/.bash_profile`, `~/.bashrc` **deliberately stay at root**: bash has no `ZDOTDIR`, so relocating them would require forwarder stubs or symlinks at root anyway — pure indirection with no decluttering. `~/.hushlogin` also stays at root (login reads it there literally). Don't try to "fix" the bash/hushlogin asymmetry; it's intentional.

**No `~/.profile`.** It's intentionally absent. cargo's PATH is owned by `shell/env.d/040-env.{sh,fish}` (which source `~/.cargo/env` / `fish_add_path`), so rustup's habitual `. ~/.cargo/env` injection into `~/.profile` is redundant cruft. The bootstrap installs rustup with `--no-modify-path` (`yadm/snippets/shared/11-rustup.sh`) so it never recreates it. If a `~/.profile` reappears, a rustup reinstall bypassed that flag — delete it.

Pre/post hooks: `~/.pre.{zsh,sh}` and `~/.post.{zsh,sh}` are sourced if present (not tracked; machine-local overrides).

### Personal bin (`~/.config/bin/`)

Executable scripts on `$PATH` (added via `env.d/040-env.sh`):
- `bbs` - interactive Brewfile scope selector (applies `Brewfile@<scope>` files)
- `pb` - lists personal bin executables, shows which are yadm-managed
- `up` - system-wide updater (brew, rust, zinit, vim-plug, gcloud, tpm); writes timestamp to `~/.local/state/up/last_upped_at`
- `check-shell-parity` - detects POSIX↔fish alias/abbr/function name drift across the paired `shell/interactive.d/*.sh` ↔ `fish/conf.d/*.fish` files; exits non-zero on drift (run by the dream procedure in `~/.config/.claude/DREAM.md`)
- `gpg-yadm-op` - GPG wrapper that fetches symmetric passphrase from 1Password (Touch ID) for yadm encrypt/decrypt
- `yadm-wrapper` - wraps yadm with custom subcommands (see below); also reachable as `yadm` via the `~/.config/bin/yadm` symlink (shadows brew's yadm — see "Path availability")
- Additional encrypted scripts may exist (see `~/.config/yadm/encrypt`)

### Externally-managed PATH lane (`~/.local/bin/`)

On `$PATH` via `env.d/040-env.sh` (the same loop that adds `~/.config/bin/`), but **not** YADM-tracked.
Canonical lane for executables managed by external meta-projects (halo, bb).

- A single registry file `~/.local/bin/.reliquary-managed` lists every managed binary, one per line, as `<name>[<TAB><owner>]`. The **owner** column is optional, per-entry provenance (the publishing meta-project). `#` comments and blank lines are ignored; membership is keyed on the first field.
- Do not `yadm add` anything from `~/.local/bin/`, including the registry file.
- Do not hand-edit the registry; it is written by the publish helper. (Legacy per-meta `.<name>-managed` files are folded into the single registry automatically — by bootstrap, `relic migrate`, and first publish.)
- The binaries are regenerable: re-run the owning meta-project's publish flow. See that meta-project's `CLAUDE.md` for the protocol.

The publish helper lives here: `~/.config/reliquary/lib/install-on-path.sh` (yadm-tracked, sourced on demand by each meta-project's publish scripts). Callers invoke it as `META_NAME=<name> source "$HOME/.config/reliquary/lib/install-on-path.sh"`; `META_NAME` is **optional** (when set it becomes the owner column and gates collision detection). **PATH names must be unique** — the helper fails fast if a name is already owned by a different relic, already resolves elsewhere on `$PATH`, or collides with a foreign file. One canonical implementation across all meta-projects — do not duplicate into individual meta-repos.

Sanctioned sidesteps: a meta-project may bypass the helper for advanced cases (template substitution, self-update, embedded provenance). The `bb` CLI is the canonical example. Those callers stay responsible for not stomping on YADM-tracked files.

### Relic graduation (`~/.config/relics/`, `~/.config/reliquary/`, `~/.config/attic/`)

Personal CLI utils have a three-stage lifecycle. A **relic** is a personal tool the author keeps:

- **Stage 1 — one-shot util**: single file in `~/.config/bin/` (status quo; `bbs`, `pb`, `up`, etc.).
- **Stage 2 — in-house relic**: directory at `~/.config/relics/<name>/`, yadm-tracked, with a manifest (`relic.sh`), an `entrypoints/` directory, and optional `src/`, `tests/`, `scripts/`. Published onto PATH via the shared lib. The `relic` CLI itself is the first Stage-2 relic.
- **Stage 3 — external relic**: independent repo at `~/Developer/<name>/` (`bb`, `halo` today). The dependency is strictly **unidirectional** (relic → reliquary, via `install-on-path.sh`). Reliquary's "known external relics" list in `GRADUATION.md` is a best-effort convenience, not authoritative; it can also discover registrants via the registry's owner column, but doesn't chase this exhaustively.

The `relic` CLI (`relic list|status|publish|test|update|scaffold|registry|migrate|doctor`) is the user-facing surface over all of this — see `GRADUATION.md`. `scaffold <name>` promotes a Stage-1 `~/.config/bin` util (or a fresh idea) into a Stage-2 relic — infers RUNTIME from the script's shebang (or `-r/--runtime`), publishes, and stages the result in yadm. `registry` takes `--migrate`/`--prune`; `doctor` is a read-only registry ↔ PATH ↔ entrypoints health check.

`~/.config/reliquary/` holds the meta — canonical docs (`GRADUATION.md`), the shared libraries (`lib/relic.sh`, `lib/install-on-path.sh`), the relic skeleton (`template/`), and deferred-work handoffs (`design/`: `relic graduate`).

`~/.config/attic/` is the **private relic lane** — the whole subtree is encrypted (the `.config/attic/**` pattern in `~/.config/yadm/encrypt`). Same anatomy inside as public relics.

Manifest-declared `BREW_DEPS` and `MIN_RUNTIME_VERSION` are **load-bearing**: `relic::check_deps` fails closed at publish time. When a relic graduates to Stage 3, its deps should be reflected in the appropriate Brewfile — the manifest stays the source of truth.

Bootstrap re-publishes all relics via `~/.config/yadm/snippets/shared/12-publish-relics.sh`. `up` runs `relic::update` per relic; opt out with `UP_SKIP_RELICS=1 up` or `up --no-relics`.

Full reference: `~/.config/reliquary/GRADUATION.md`.

### yadm wrapper (`~/.config/bin/yadm-wrapper`)

Reachable as `yadm` in **every** shell — `~/.config/bin/yadm` is a symlink to this script, and `~/.config/bin` is forced ahead of Homebrew on `$PATH` so it shadows brew's yadm (no alias involved). For bash/zsh that ordering is asserted **last** by `env.d/999-path.sh` (a mid-040 placement let later prepends re-win); fish does its own ordering in `040-env.fish`. Adds custom commands:
- `yadm own` / `yadm disown` - switch remote between SSH and HTTPS
- `yadm encrypt` / `yadm decrypt` - delegates to yadm + records archive SHA256
- `yadm check` - compares archive SHA256 to detect drift
- `yadm verify` - decrypts archive to tmpdir and diffs against disk
- `yadm ls-all` - complete tracked set: `yadm ls-files` (plaintext) + archive listing (`decrypt -l`, Touch ID)
- `yadm doctor` - dotfiles health self-check (shell resolution, startup smoke tests, `$PATH`-dup sanity, parity, archive drift); detect-only, Touch-ID-free. `--full` adds the `verify` deep check; `--quiet`/`-q` runs silently and prints the report only on a failure/warning (flags compose). Used by the dream pre-pass (`~/.config/.claude/DREAM.md`) and, in `--quiet` form, by `yadm update`
- `yadm update` - `pull --ff-only`, then `doctor --quiet` (silent when healthy; surfaces drift/regressions the pull introduced — the quiet doctor already covers the encrypted-archive check)
- All other commands pass through to real yadm, followed by an encrypted-files check

Because the wrapper shadows brew's yadm on `$PATH`, bare `yadm <cmd>` — including wrapper-only subcommands — works in interactive *and* non-interactive shells alike (see "Path availability" above). The wrapper resolves the real yadm by scanning `$PATH` outside `~/.config/bin`, so it never recurses into itself.

### Bootstrap (`~/.config/yadm/bootstrap`)

Sourcing order: `lib/` -> `util/` -> OS-specific (`macos/` or `linux/`) -> `shared/`.
Snippet dirs live under `~/.config/yadm/snippets/`. Files are `NN-name.sh`, sorted and sourced in order.

Key macOS snippets: homebrew install, brewfile apply, mas (App Store), directory creation, quartz filters, tilde-switch.
Shared snippets: pbin setup, tpm (tmux plugin manager), rustup.
Util snippets: print helpers, copy helpers, symlink helpers.

### Brewfiles (`~/.config/brew/`)

- `Brewfile` - base (always applied during bootstrap)
- `Brewfile@<scope>` - optional scopes applied interactively via `bbs`
- Some scoped files are tracked publicly, others are encrypted (see `~/.config/yadm/encrypt`)
- `Brewfile*.lock.json` - **deliberately not tracked** (never `yadm add`-ed; yadm's whitelist keeps them out by default — no gitignore needed). Homebrew is rolling-release, so pinned bottle SHAs expire and aren't reinstallable, while the lock churns on every `brew upgrade`. The Brewfiles are the source of truth (track-latest intent); locks are regenerated locally by `brew bundle`/`bbs`.

### Other tracked configs

- `ghostty` - terminal emulator config
- `git/attributes` - `* text=auto`, binary markers for `*.png`/`*.plist`
- `vim/vimrc` - Vim configuration (Vim 9.2+ native XDG support)
- `oh-my-posh` - shell prompt theme
- `quartz-filters` - macOS PDF compression filters
- `tmux` - tmux configuration
- `zsh/completion/docker` - docker completions for zsh

### Deliberately not tracked (audited)

These were reviewed and **intentionally excluded** — neither plaintext-tracked nor encrypted. All are regenerated by their tool's normal auth/setup flow on a new machine, so the sync value is low and the leak/footgun risk is not worth it:

- `~/.config/raycast/config.json` - holds a live `rca_…` access token; regenerable, machine-local.
- `~/.kube/config` - embeds a live orbstack `client-key-data` and benefactor GKE cluster names; GKE contexts re-fetch via `gcloud container clusters get-credentials`, orbstack regenerates its own.
- `~/.docker/config.json` - no secrets today (`auths: {}`, osxkeychain + gcloud credHelpers), but tracking it risks a future `docker login` writing base64 creds into `auths{}`; the credHelper config regenerates on setup.
- `~/.config/gh/hosts.yml` - just SSH-protocol pref + public username; `gh auth login` regenerates it, and tracking risks `gh` writing an `oauth_token` into a plaintext file.
- `~/.gnupg/gpg-agent.conf` + `~/.gnupg/pinentry-ide.sh` - IDE-generated (PhpStorm) GPG-signing pinentry config and its shim. No secrets, but both hardcode absolute `$HOME` paths carrying the local username, which the `pre_commit` identity guard bars from the public plaintext tree; encrypting them isn't worth it since the IDE regenerates both on its next GPG run.

### Repository documentation (`~/.github/`)

- `README.md` - install/usage instructions, rendered on the GitHub repo page
- `LICENSE.md` - MIT license

These live in `~/.github/` (not `~/.config/`) because GitHub only renders READMEs from the repo root or `.github/` directory.

### Hooks

Encrypted hooks may exist (see `~/.config/yadm/encrypt`).

## yadm aliases (from `interactive.d/100-aliases-yadm.sh`)

```
ya='yadm add'          yf='yadm fetch ...'    yrs='yadm restore --staged'
ys='yadm status'       yc='yadm commit'       yp='yadm push'
ypt='yadm push --tags' yff='yadm merge --ff-only @{u}'
ypff='yadm pull --ff-only'
ylf / ywlf = yadm log (80/140 char wide)
```

## Workflow

**Adding/updating dotfiles:**
1. Edit the file
2. `ya <file>` (or `yadm add <file>`)
3. `ys` to verify
4. `yc` to commit
5. `yp` to push

**New machine setup:** see `~/.github/README.md` - curl yadm, clone, bootstrap, then decrypt + bootstrap again.

**Updating:** `yadm update` (pull + encrypted-files check) or `ypff`.
