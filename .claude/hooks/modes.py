#!/usr/bin/env python3
"""UserPromptSubmit hook — activate session "modes" from `+token` lines.

A mode is a native slash-command file under `commands/modes/` (home or, for
project-specific modes, `$CLAUDE_PROJECT_DIR/.claude/commands/modes/`). Typing
`+afk` at the start of any prompt line appends that mode's directives to turn-1
context, so modes ride along with the opening prompt in a single message.

Boundaries baked in by design:
  - The hook can only *append* context; it cannot edit/strip the prompt. The
    `+token` text stays put — the injected preamble tells the model to read it
    as a selector, not task content.
  - Strict no-op unless at least one token resolves to a `modes/` file. Unmatched
    tokens are ignored entirely.
  - Personal(home)-first resolution, mirroring native `/afk` precedence, so
    `/afk` and `+afk` always resolve the same file.
  - Fail-open: any error, or nothing to do, exits 0 with no output. The prompt is
    never affected by this hook.

See `~/.claude/hooks/modes.md` for the framework and the mode-file format.
"""
import json
import os
import re
import sys
from pathlib import Path

# A single token: exactly one leading '+', then an alphanumeric run that may
# carry '-'/'_' *inside* (must start and end alphanumeric). Single char (`+a`) ok.
_TOKEN = re.compile(r"\+(?![+])([A-Za-z0-9](?:[A-Za-z0-9_-]*[A-Za-z0-9])?)")

# The leading run of a qualifying line: optional indent, then one-or-more tokens
# each followed by horizontal whitespace or end-of-line. Stops at the first thing
# that isn't a token, leaving the rest of the line as untouched task text.
_LEADING_RUN = re.compile(
    r"^[ \t]*(?:\+(?![+])[A-Za-z0-9](?:[A-Za-z0-9_-]*[A-Za-z0-9])?(?:[ \t]+|$))+"
)


def _tokens_from_prompt(prompt):
    """Ordered, de-duplicated token names from lines that start with `+`."""
    seen = set()
    ordered = []
    for line in prompt.splitlines():
        if not line.lstrip(" \t").startswith("+"):
            continue
        run = _LEADING_RUN.match(line)
        if not run:
            continue
        for name in _TOKEN.findall(run.group(0)):
            if name not in seen:
                seen.add(name)
                ordered.append(name)
    return ordered


def _mode_file(name, home_root, project_root):
    """First `<name>.md` under a `commands/modes/` tree, personal(home)-first."""
    roots = [home_root / ".claude" / "commands" / "modes"]
    if project_root is not None:
        proj_modes = project_root / ".claude" / "commands" / "modes"
        if proj_modes.resolve() != roots[0].resolve():  # skip when session cwd is $HOME
            roots.append(proj_modes)
    for root in roots:
        if not root.is_dir():
            continue
        for path in sorted(p for p in root.rglob(name + ".md") if p.is_file()):
            return path
    return None


def _strip_frontmatter(text):
    """Drop a leading `---` … `---` YAML block, if present."""
    lines = text.splitlines()
    if lines and lines[0].strip() == "---":
        for i in range(1, len(lines)):
            if lines[i].strip() == "---":
                return "\n".join(lines[i + 1:]).lstrip("\n")
    return text


_PREAMBLE = (
    "The user activated session modes via `+<name>` lines in the prompt. Those "
    "`+` tokens are mode selectors — treat them as markers, not as task content. "
    "The mode directives below are ACTIVE and BINDING for this session; apply each "
    "unless the user explicitly overrides it."
)


def main():
    data = json.loads(sys.stdin.read())
    prompt = data.get("prompt") or ""
    names = _tokens_from_prompt(prompt)
    if not names:
        return

    home_root = Path.home()
    pd = os.environ.get("CLAUDE_PROJECT_DIR") or data.get("cwd")
    project_root = Path(pd) if pd else None

    blocks = []
    for name in names:
        path = _mode_file(name, home_root, project_root)
        if path is None:
            continue
        try:
            body = _strip_frontmatter(path.read_text(encoding="utf-8")).strip()
        except OSError:
            continue
        if body:
            blocks.append((name, body))

    if not blocks:  # strict no-op: nothing matched a modes/ file
        return

    parts = [_PREAMBLE]
    for name, body in blocks:
        parts.append("\n===== MODE: {} =====\n{}".format(name, body))
    sys.stdout.write(json.dumps({
        "hookSpecificOutput": {
            "hookEventName": "UserPromptSubmit",
            "additionalContext": "\n".join(parts),
        }
    }))


if __name__ == "__main__":
    try:
        main()
    except Exception:  # fail-open — a hook bug must never break prompt submission
        pass
    sys.exit(0)
