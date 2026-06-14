# Reliquary TODO

Single live queue for Reliquary work.

> **House rule.** Completed items are **deleted** from this file, not archived —
> `yadm log` is the record of what was done. No DONE section. Add new work as a
> new section; keep each item independently sized so it can be picked up alone.

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
_Reference (not a todo): confirmed-correct config exclusions are documented in
`~/.config/CLAUDE.md` → "Deliberately not tracked (audited)"._
