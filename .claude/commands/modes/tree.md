---
description: Tree mode
disable-model-invocation: true
---

# Tree mode

User highlights that you must work in current worktree, not main checkout.

If you use Chrome with extension, you need to: expose a port temporarily (port is exposed by default only in main checkout); enable `APP_AUTH_OPEN_BOOK=true` in `.env.local` (gitignored) to bypass auth.

Agentic second brain (`**/CLAUDE.md` + `.claude/**`) is gitignored and managed by `clc` util.
Brain changes must be `clc save`-ed, so they are not lost when worktree is later transplanted.
Git commits run `clc save` in post-commit hooks automatically.

Agentic handoffs (`.claude/handoffs/<name>.md`) live **only** in main checkout.
When working on handoffs, narrowly reach into main checkout.
Thus, changes only to handoffs do not need `clc save`.
