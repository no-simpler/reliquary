---
triggers:
  - on: activate
  - every: { tokens: 40000 }
refrain: 'Second brain is `clc`-managed — in a non-main worktree every brain change needs `clc save`, or it is lost on transplant.'
---

# `clc` mode

Agentic second brain (`**/CLAUDE.md` + `.claude/**`) is gitignored and managed by `clc` util.
In non-main worktrees, brain changes must be `clc save`-ed, so they are not lost when worktree is later transplanted.
Git commits run `clc save` in post-commit hooks automatically.
