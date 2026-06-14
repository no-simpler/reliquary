# Reliquary TODO

Single live queue for Reliquary work.

> **House rule.** Completed items are **deleted** from this file, not archived —
> `yadm log` is the record of what was done. No DONE section. Add new work as a
> new section; keep each item independently sized so it can be picked up alone.

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

## yadm doctor: optional post-checkout hook
`yadm doctor` exists as a wrapper subcommand + dream pre-pass step. Not yet wired as a
yadm post-checkout hook (would run the detect-only checks automatically after `yadm pull`/
clone). Decide whether that's wanted; hooks live under `~/.config/yadm/hooks/` (encrypted).

## Bash prompt parity gap
`050-prompt.{fish,zsh}` use oh-my-posh; `050-prompt.bash` has a hand-rolled `PS1`.
Either port bash to oh-my-posh (`oh-my-posh init bash`) or document bash as the
deliberate no-posh fallback shell.

## Encryption pattern hygiene
`~/.config/yadm/encrypt` spans themes (benefactor/mm/bt). Consider themed
sub-archives via `yadm encrypt -A`, or consolidating glob patterns. Keep names opaque.

## oh-my-posh config placement
`~/.config/oh-my-posh/dreamsofautonomy.toml` is the only theme. Fine as-is unless
you want `POSH_THEMES_PATH` discovery — then update `040-env.*`.

---
_Reference (not a todo): confirmed-correct exclusions and the original inventory
live in the audit plan `~/.claude/plans/this-macos-machine-syncs-manages-groovy-zephyr.md`._
