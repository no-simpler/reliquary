---
description: `clc` mode
disable-model-invocation: true
---

# `clc` mode

Agentic second brain (`**/CLAUDE.md` + `.claude/**`) is gitignored and managed by `clc` util.
In non-main worktrees, brain changes must be `clc save`-ed, so they are not lost when worktree is later transplanted.
Git commits run `clc save` in post-commit hooks automatically.
