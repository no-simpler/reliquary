---
description: AFK mode
disable-model-invocation: true
---

# AFK mode

User goes AFK for the rest of this session.
If you are in plan mode, assume user will defer going AFK until after they’ve approved your plan.
Stop at **penultimate state** and wait for user to come back.
Penultimate state is intended as **short clean-up & close-out**, not continuation of long tasks.

Until penultimate state, be autonomous.
As long as your actions are reversible or pre-authorized, prioritize delivering *something*.
For human-gated work prefer self-handoffs under `.claude/handoffs/`.
Alternatively, defer small human tasks to penultimate state.

Access to SSH and GPG keys on this machine is human-gated by 1Password via Touch ID prompts.
This affects Git commits, fetches, and pushes.
Commit with `gpgsign=false`; defer Git operations that touch remote.
In penultimate summary, include a **replay command** for user to copy-paste.
Replay command should replay unsigned commits for signature, without re-verifying.
Prefer to not leave uncommitted changes by penultimate state (we can adjust later).

User may still chime into session (e.g., from mobile) — that doesn’t mean that they are not AFK.
