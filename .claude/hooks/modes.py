#!/usr/bin/env python3
"""UserPromptSubmit hook — activate session "modes" from `+token` lines.

A mode is a native slash-command file under a `commands/modes/` tree — home,
project (`$CLAUDE_PROJECT_DIR/.claude/`), or one shipped by a skills-dir plugin
(`<base>/skills/<plugin>/commands/`). Typing `+afk` at the start of any prompt
line appends that mode's directives to *that turn's* context. The hook runs on
every prompt submission, not just the first, so a mode can ride along with the
opening prompt in a single message, or be switched on by any later message just
the same.

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
  - Prompt submissions only. Text typed to *reject* a plan or a tool-permission
    prompt reaches the model as a `tool_result`, not a prompt, so this hook never
    sees it — a `+token` there cannot newly activate a mode.

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


def _base_roots(base):
    """`commands/modes/` trees under one `.claude` base: the tree itself, then
    the one each skills-dir plugin ships.

    A directory under `<base>/skills/` is adopted by Claude Code as a plugin, so
    a mode it ships is as much a mode as any other. Marketplace plugins (under
    `~/.claude/plugins/`) are deliberately out of scope: a `+token` must not be
    able to pull directives out of third-party code.
    """
    roots = [base / "commands" / "modes"]
    skills = base / "skills"
    if skills.is_dir():
        try:
            plugins = sorted(d for d in skills.iterdir()
                             if d.is_dir() and not d.name.startswith("."))
        except OSError:
            plugins = []
        roots.extend(d / "commands" / "modes" for d in plugins)
    return roots


def _mode_file(name, home_root, project_root):
    """First `<name>.md` under any `commands/modes/` tree, personal(home)-first."""
    bases = [home_root / ".claude"]
    if project_root is not None:
        bases.append(project_root / ".claude")

    roots, seen = [], set()
    for base in bases:
        for root in _base_roots(base):
            try:
                key = root.resolve()
            except OSError:
                continue
            if key in seen:  # e.g. session cwd is $HOME
                continue
            seen.add(key)
            roots.append(root)

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
