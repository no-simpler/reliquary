# Session modes

A **mode** is a reusable paragraph of behavioral directives, enabled per session, bitflag-style.
Two first-party entry points over one file per mode:

- **`/afk`** — native slash command. Own message (stackable: `/afk /fe`). For mid-session enabling.
- **`+afk`** — a `+token` at the start of any prompt line, picked up by the `UserPromptSubmit` hook
  (`modes.py`) and appended to turn-1 context. Lets modes ride along in the opening prompt, prose
  first, in a single message.

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
