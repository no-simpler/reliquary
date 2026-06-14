# Reliquary TODO

Single live queue for Reliquary work.

> **House rule.** Completed items are **deleted** from this file, not archived —
> `yadm log` is the record of what was done. No DONE section. Add new work as a
> new section; keep each item independently sized so it can be picked up alone.

## Brew leaf cleanup (resume activity)
One-time pass over `brew leaves -r` + casks: each item either gets firmly wired
into Reliquary (Brewfile/bootstrap) or removed as garbage. Removals must be
scrubbed from every tracked Brewfile (plain **and** encrypted `@benefactor*`,
`@home`) and any bootstrap/env wiring.

- **Formulae (likely junk — per-item verdict needed):** `asciinema`, `ffmpeg`,
  `git-filter-repo`, `pango`, `rabbitmq-c`, `redis`, `whisper-cpp`, `dive`.
  `poppler` is a leaf too (PDF lib) — check adhoc dependents before cutting.
- **Casks (judgment calls):** `draw-things`, `discord`, `obsidian`,
  `protonvpn` (2nd VPN; viscosity tracked), `vivaldi` (6th browser), `sourcetree`,
  `keymapp` (ZSA hw), `robloxstudio`, `zoom`.
- **`google-cloud-sdk` is NOT a duplicate** — it's the *old token* for the renamed
  `gcloud-cli` cask (`Caskroom/google-cloud-sdk -> gcloud-cli` symlink; same install).
  Do not `brew uninstall` it — that deletes the shared files and breaks gcloud, and
  needs sudo. Only loose end: the stale old-name token still shows in `brew list --cask`.
  Dropping it cleanly is fiddly/risky; low priority, leave unless it actively bites.
- Procedure per item: `brew uninstall <x>` → `brew autoremove -n` for orphans →
  scrub from `~/.config/brew/Brewfile*` → regenerate affected `*.lock.json`.

## anaconda: wire-or-remove (+ fish conda gap)
The `anaconda` cask (~5GB, untracked) is installed, so the conda-init blocks in
`shell/env.d/040-env.sh` and `fish/conf.d/040-env.fish` are *live*. Decide:
- **Remove:** uninstall cask, then delete both now-dead anaconda blocks.
- **Keep/wire:** add to a Brewfile, then fix the fish gap — `CONDA_*` vars are
  exported in bash+zsh but not fish (fish's `conda shell.fish hook` isn't
  auto-activating `base`). Fix the fish init or document fish as deliberately
  unactivated.

## Move shell rc files inward
Relocate `.bashrc`/`.bash_profile`/`.zshrc`/`.zprofile`/`.hushlogin` from `$HOME`
root into `~/.config/`. zsh: set `ZDOTDIR=$HOME/.config/zsh` in a minimal
`~/.zshenv` (only file that can't move), move the rest into `~/.config/zsh/`.
bash: keep one-line forwarders at root, or symlink from a bootstrap snippet.
Update bootstrap snippets to install forwarders/symlinks on new machines.

## Tracked-on-disk root dotfiles still untracked
`~/.zshenv` and `~/.profile` (rustup-generated one-liners). Decide: track as
canonical, or document "leave to rustup" in README. NOTE: if rc files move
inward via `ZDOTDIR`, `~/.zshenv` becomes load-bearing and MUST be tracked.

## Sensitive/token-bearing configs — encrypt or document exclusion
Classify each as encrypt-and-track or skip-and-document: `~/.config/acli/*.yaml`,
`~/.config/raycast/config.json`, `~/.config/gh/hosts.yml`, `~/.docker/config.json`,
`~/.kube/config`, `~/.gnupg/gpg-agent.conf` (config only, never keys). If tracked,
add an opaque pattern to `~/.config/yadm/encrypt`.

## Cross-shell env var parity (excl. conda — see anaconda item)
Promote `PAGER`, `LESS`, `LSCOLORS`, `LS_COLORS` (currently zsh-only, set by
zinit/omz plugins) into `040-env.{sh,fish}` so `less`/`ls` behave identically
regardless of launching shell. Leave color vars (`010-colors.sh`) and zsh/posh
internals as-is.

## Plugin reproducibility
- vim-plug: document `:PlugInstall`/`:PlugUpdate` in bootstrap notes.
- TPM: add a TPM update step to `~/.config/bin/up` (currently only initial install).
- fisher: verify `up`'s `fisher update` actually applies pinned `fish_plugins`.

## yadm doctor self-check command
Single `~/.config/bin/` command running: `yadm check`, `yadm verify`,
shell startup smoke-tests (`<shell> -ic 'echo ok'`), `check-shell-parity`, and a
`$PATH`-duplicate sanity check. Could also run as a post-checkout hook. (Overlaps the
dream pre-pass in `~/.config/.claude/DREAM.md` — that's the manual-invocation home; a
`yadm doctor` would be the standalone CLI surface for the same mechanical checks.)

## Bash prompt parity gap
`050-prompt.{fish,zsh}` use oh-my-posh; `050-prompt.bash` has a hand-rolled `PS1`.
Either port bash to oh-my-posh (`oh-my-posh init bash`) or document bash as the
deliberate no-posh fallback shell.

## 060-fzf is zsh-only
Bash has no fzf key bindings (fish uses its own Fisher plugin). Add `060-fzf.bash`
(`fzf --bash`), or merge into `060-fzf.sh` with shell detection calling `fzf --<shell>`.

## Encryption pattern hygiene
`~/.config/yadm/encrypt` spans themes (benefactor/mm/bt). Consider themed
sub-archives via `yadm encrypt -A`, or consolidating glob patterns. Keep names opaque.

## oh-my-posh config placement
`~/.config/oh-my-posh/dreamsofautonomy.toml` is the only theme. Fine as-is unless
you want `POSH_THEMES_PATH` discovery — then update `040-env.*`.

---
_Reference (not a todo): confirmed-correct exclusions and the original inventory
live in the audit plan `~/.claude/plans/this-macos-machine-syncs-manages-groovy-zephyr.md`._
