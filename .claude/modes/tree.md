---
triggers:
  - on: activate
  - every: { tokens: 40000 }
refrain: Work in the current worktree, not the main checkout.
---

# Tree mode

User highlights that you must work in current worktree, not main checkout.

If there is Docker Compose stack — it doesn’t have exposed ports in tree; expose temporarily when needed.
Gitignored local-only config from main checkout may need to be copied.
