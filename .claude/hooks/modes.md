# Session modes

A **mode** is a reusable paragraph of behavioral directives, enabled per session, bitflag-style.
Two first-party entry points over one file per mode:

- **`/afk`** — native slash command. Own message (stackable: `/afk /fe`). For mid-session enabling.
- **`+afk`** — a `+token` at the start of any prompt line, picked up by the `UserPromptSubmit` hook
  (`modes.py`) and appended to **that turn's** context. Fires on every prompt, not just the first —
  so a mode can ride along in the opening prompt, prose first, in a single message, or be switched
  on by any later message just the same.

## Adding a mode

Drop one file at `~/.claude/commands/modes/<name>.md` (machine-wide) or
`<project>/.claude/commands/modes/<name>.md` (project-specific). No registry, nothing to enumerate.

```
---
description: <one line — shown only in the /-menu>
disable-model-invocation: true
---

# <NAME>

<imperative, self-contained directives — must read correctly whether expanded via /name or appended
by the hook>
```

`disable-model-invocation: true` keeps the mode out of model context (never auto-invoked, never
advertised) while leaving `/name` user-invocable. A mode is a behavioral toggle, not a procedure or
a task — keep the body directives, not steps.

## `+token` syntax and hook contract

- A line contributes tokens only if its first non-whitespace char is `+`. The hook reads the
  **leading run** of whitespace-separated tokens; the rest of the line stays as task text.
- Token: exactly one leading `+`, then alphanumerics with `-`/`_` allowed inside. So `C++`, `a+b`,
  `+5` in prose never trigger.
- Tokens resolve by basename against `commands/modes/` only — **home first, then project**
  (mirrors native `/afk` precedence, so `/afk` and `+afk` never diverge; a project cannot override a
  home mode of the same name — use a distinct name). Order preserved, duplicates collapsed.
- The hook can only **append** context, never strip the prompt — so the `+token` text remains; the
  injected preamble marks it as a selector. It is a **strict no-op** unless ≥1 token matches a mode
  file, and it **fails open** (any error → the prompt is untouched).

Wiring lives in `~/.claude/settings.json` under `hooks.UserPromptSubmit`.

## Where `+tokens` are picked up

`UserPromptSubmit` fires per *submission*, not per session — the mode text arrives as its own
context attachment on every qualifying prompt. Verified end-to-end (2026-08-08, CC 2.1.226): opening
prompt; any follow-up message; first prompt after `/clear`; a message queued while a turn is still
running; a prompt submitted in plan mode; a `+token` on a later line of a multi-line prompt; a
project-scoped mode file; the initial prompt passed as a CLI argument; `claude -p` with the prompt
as an argument or piped on stdin; and a session resumed with `--continue`.

**The one gap — rejection feedback.** Text typed while *rejecting* a plan or a tool-permission
prompt reaches the model as a `tool_result`, not a prompt submission, so the hook never sees it and
a `+token` there is inert. (Verified on plan rejection; permission rejection carries the same
`tool_result` shape.) This blocks only *newly activating* a mode — one already active in the session
keeps applying, its directives being in context already. To switch a mode on at that moment, send it
as an ordinary message afterwards, or use `/name`.
