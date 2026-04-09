# Reliquary - Dotfile Repository

Public dotfile repo managed by [yadm](https://yadm.io/) (yet another dotfiles manager).
Repo: [`no-simpler/reliquary`](https://github.com/no-simpler/reliquary).

yadm is a thin git wrapper whose work tree is `$HOME` and git dir is `~/.local/share/yadm/repo.git`.
All standard git commands work via `yadm <cmd>`.
Only explicitly `yadm add`-ed files are tracked; everything else is ignored.

## Critical: GPG constraints

- **Commits are GPG-signed.** `git commit` (and `yadm commit`) triggers a full-screen GPG passphrase prompt that breaks Claude Code. **Never run commit/merge/rebase automatically.** Hand off to the user with exact instructions.
- **yadm encrypt/decrypt also invoke GPG.** Same constraint applies. Never run `yadm encrypt` or `yadm decrypt` automatically.

## Encryption

Sensitive files are GPG-encrypted into `~/.local/share/yadm/archive` and tracked in the public repo.
Patterns are listed in `~/.config/yadm/encrypt`.
The `yadm-wrapper` script (see below) tracks archive SHA256 in `~/.local/state/yadm/last_decrypted` to detect encrypt/decrypt drift.

**Convention:** Encryption patterns in `encrypt` are intentionally obfuscated — they should not reveal what they protect. When adding new patterns, use opaque names that don't hint at content. Do not describe or document the contents of encrypted files in any tracked file (including this one). Future sessions can read encrypted file contents locally after decryption.

## Repository structure

### Shell configuration (`~/.config/shell/`)

Files are sourced alphanumerically by `~/.zshrc` (and `~/.bashrc`). Convention:
- `NNN-name.sh` = shared (bash + zsh only — fish cannot parse POSIX syntax)
- `NNN-name.fish` = fish-only
- `NNN-name.zsh` = zsh-only
- `NNN-name.bash` = bash-only

Numbering controls load order:
```
010-colors    020-plugins   030-config    040-env
050-prompt    060-fzf       070-fixes     080-check
090-funcs     100-aliases   100-aliases-{git,docker,yadm}
```
Additional encrypted shell files may exist (see `~/.config/yadm/encrypt`).

Shell var `$D__SHELL` is set to `zsh` or `bash` in the respective rc file and used throughout for shell-specific branching.

Pre/post hooks: `~/.pre.{zsh,sh}` and `~/.post.{zsh,sh}` are sourced if present (not tracked; machine-local overrides).

### Personal bin (`~/.config/bin/`)

Executable scripts on `$PATH` (added via `040-env.sh`):
- `bbs` - interactive Brewfile scope selector (applies `Brewfile@<scope>` files)
- `pb` - lists personal bin executables, shows which are yadm-managed
- `up` - system-wide updater (brew, rust, zinit, vim-plug, gcloud, tpm); writes timestamp to `~/.local/state/up/last_upped_at`
- `yadm-wrapper` - wraps yadm with custom subcommands (see below)
- Additional encrypted scripts may exist (see `~/.config/yadm/encrypt`)

### yadm wrapper (`~/.config/bin/yadm-wrapper`)

Aliased as `yadm` in shell. Adds custom commands:
- `yadm own` / `yadm disown` - switch remote between SSH and HTTPS
- `yadm encrypt` / `yadm decrypt` - delegates to yadm + records archive SHA256
- `yadm check` - compares archive SHA256 to detect drift
- `yadm update` - `pull --ff-only` + check encrypted files
- All other commands pass through to real yadm, followed by an encrypted-files check

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

### Hooks

Encrypted hooks may exist (see `~/.config/yadm/encrypt`).

## yadm aliases (from `100-aliases-yadm.sh`)

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
4. `yc` to commit (triggers GPG prompt)
5. `yp` to push

**New machine setup:** see `~/.github/README.md` - curl yadm, clone, bootstrap, then decrypt + bootstrap again.

**Updating:** `yadm update` (pull + encrypted-files check) or `ypff`.
