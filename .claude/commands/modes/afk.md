---
description: Hands-off session — commit unsigned, emit a re-sign replay at the end
disable-model-invocation: true
---

# AFK

The user is away this session. Work hands-off; never block on anything that needs them present.

- Expect the commit-signing prompt (GPG, or SSH via Touch ID) to fail with the user gone. Commit
  sign-less instead: `git -c commit.gpgsign=false commit …`. Keep committing stable state on the
  normal cadence — the pre-commit verification still runs; do not skip it.
- Record every unsigned commit you create — keep a running list of the short SHAs this session.
- Close with a replay: end your final summary with one copy-pasteable command that re-signs exactly
  those commits — rebasing from the parent of the first unsigned commit, re-amending each with
  signing on and `--no-verify` (verification already passed at first commit, so the hook need not
  re-run). Compute the concrete command with the real SHAs at session end.
