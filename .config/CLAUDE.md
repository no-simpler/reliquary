# Reliquary - Dotfile Repository

Public dotfile repo managed by [yadm](https://yadm.io/) (yet another dotfiles manager).
Repo: [`no-simpler/reliquary`](https://github.com/no-simpler/reliquary).

yadm is a thin git wrapper whose work tree is `$HOME` and git dir is `~/.local/share/yadm/repo.git`.
All standard git commands work via `yadm <cmd>`.
Only explicitly `yadm add`-ed files are tracked; everything else is ignored.

## Encryption

Sensitive files are GPG-encrypted into `~/.local/share/yadm/archive` and tracked in the public repo.
Patterns are listed in `~/.config/yadm/encrypt`.
The password for `yadm encrypt`, `yadm decrypt`, and `yadm verify` is fetched from 1Password implicitly (Touch ID prompt).
The `yadm-wrapper` script (see below) tracks archive SHA256 in `~/.local/state/yadm/last_decrypted` to detect encrypt/decrypt drift.

**Convention:** Encryption patterns in `encrypt` are intentionally obfuscated — they should not reveal what they protect. When adding new patterns, use opaque names that don't hint at content. Do not describe or document the contents of encrypted files in any tracked file (including this one). Future sessions can read encrypted file contents locally after decryption.

## yadm operations

**Tracked-file discovery.** The work tree is `$HOME`, not the cwd. Tracked files live across `$HOME` — `.bashrc`, `.zshrc`, `.bash_profile`, `.zshenv`, `.bash_env`, `.config/...`, `.ssh/config`, `.local/share/yadm/archive`, etc. Don't assume a file at `$HOME` root is untracked just because it sits outside the cwd. Check tracking with `yadm ls-files <path>`. See full dirty state with `yadm status` (always reports relative to `$HOME`).

**Reading `yadm status`** (easy to misread):
- `M ` / `R `: tracked file modified or renamed → almost always belongs in the next commit
- `A `: newly staged for tracking
- `??`: untracked (file exists but isn't `yadm add`-ed; deciding to track is a separate judgment call)

**Default staging policy.** When committing in this repo, stage every `M` / `R` line unless explicitly told to exclude one. Reliquary bundles whatever is dirty; splitting the working tree by topic isn't the house style. Skipping an `M` line on the assumption it's "unrelated" has historically been wrong.

**Path availability.** `yadm` is on `$PATH` in non-interactive bash and zsh: `~/.zshenv` and `~/.bash_env` (via `$BASH_ENV`) source `env.d/040-env.sh`, which puts Homebrew on `$PATH`. Use bare `yadm <cmd>` — no need for `/opt/homebrew/bin/yadm`. The wrapper alias is interactive-only (see below); wrapper-only subcommands still need explicit `~/.config/bin/yadm-wrapper <cmd>`.

**Authorization.** `yadm commit` and `yadm push` are pre-approved — no need to ask before either. `yadm encrypt` triggers Touch ID; announce it before running so the user is ready to approve.

## Repository structure

### Shell configuration (`~/.config/shell/`)

Two-tier layout — the directory name *is* the contract:
- `~/.config/shell/env.d/` = **always-on** (PATH, tool init, locale, auth env). Sourced by `~/.zshenv` (every zsh) and `~/.bash_env` (every non-interactive bash via `$BASH_ENV`; also from `~/.bash_profile` for login bash). Idempotent — re-sourcing is safe.
- `~/.config/shell/interactive.d/` = **interactive only** (plugins, prompt, completions, aliases, update checks). Sourced by `~/.zshrc` / `~/.bashrc` behind their interactive gates.

Filename suffix selects shell:
- `NNN-name.sh` = shared (bash + zsh only — fish cannot parse POSIX syntax)
- `NNN-name.fish` = fish-only
- `NNN-name.zsh` = zsh-only
- `NNN-name.bash` = bash-only

Numbering controls load order:
```
env.d/         : 040-env  070-fixes  150-benefactor
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

Pre/post hooks: `~/.pre.{zsh,sh}` and `~/.post.{zsh,sh}` are sourced if present (not tracked; machine-local overrides).

### Personal bin (`~/.config/bin/`)

Executable scripts on `$PATH` (added via `env.d/040-env.sh`):
- `bbs` - interactive Brewfile scope selector (applies `Brewfile@<scope>` files)
- `pb` - lists personal bin executables, shows which are yadm-managed
- `up` - system-wide updater (brew, rust, zinit, vim-plug, gcloud, tpm); writes timestamp to `~/.local/state/up/last_upped_at`
- `gpg-yadm-op` - GPG wrapper that fetches symmetric passphrase from 1Password (Touch ID) for yadm encrypt/decrypt
- `yadm-wrapper` - wraps yadm with custom subcommands (see below)
- Additional encrypted scripts may exist (see `~/.config/yadm/encrypt`)

### yadm wrapper (`~/.config/bin/yadm-wrapper`)

Aliased as `yadm` in interactive shells. Adds custom commands:
- `yadm own` / `yadm disown` - switch remote between SSH and HTTPS
- `yadm encrypt` / `yadm decrypt` - delegates to yadm + records archive SHA256
- `yadm check` - compares archive SHA256 to detect drift
- `yadm verify` - decrypts archive to tmpdir and diffs against disk
- `yadm update` - `pull --ff-only` + check encrypted files
- All other commands pass through to real yadm, followed by an encrypted-files check

The alias is interactive-only. From non-interactive shells, invoke `~/.config/bin/yadm-wrapper <cmd>` for wrapper-only subcommands; bare `yadm <cmd>` works for everything else (see "yadm operations" above).

### Bootstrap (`~/.config/yadm/bootstrap`)

Sourcing order: `lib/` -> `util/` -> OS-specific (`macos/` or `linux/`) -> `shared/`.
Snippet dirs live under `~/.config/yadm/snippets/`. Files are `NN-name.sh`, sorted and sourced in order.

Key macOS snippets: homebrew install, brewfile apply, mas (App Store), directory creation, choosy, iterm2, quartz filters, tilde-switch.
Shared snippets: pbin setup, tpm (tmux plugin manager), rustup.
Util snippets: print helpers, copy helpers, symlink helpers.

### Brewfiles (`~/.config/brew/`)

- `Brewfile` - base (always applied during bootstrap)
- `Brewfile@<scope>` - optional scopes applied interactively via `bbs`
- Some scoped files are tracked publicly, others are encrypted (see `~/.config/yadm/encrypt`)

### Other tracked configs

- `alacritty`, `ghostty` - terminal emulator configs
- `choosy` - browser chooser
- `git/attributes` - `* text=auto`, binary markers for `*.png`/`*.plist`
- `vim/vimrc` - Vim configuration (Vim 9.2+ native XDG support)
- `mpv` - media player config
- `oh-my-posh` - shell prompt theme
- `quartz-filters` - macOS PDF compression filters
- `tmux` - tmux configuration
- `zsh/completion/docker` - docker completions for zsh

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
