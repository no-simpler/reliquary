---
description: Spec mode
disable-model-invocation: true
---

# Spec mode

User signals this session drives a docket spec (`docket guide spec`).
Address `blocked` first, if present; once cleared, drop the block and proceed.

When spec evolves at any stage, summarize changes to user tersely and at altitude.

## Stage `design`

Where warranted: extrapolate and augment; expecially where deferring to implementation is risky.
Evolve spec within artistic license outlined by user; if not given — err on the side of caution.

Open questions allowed temporarily: settle before implementation or explicitly defer to it.
Mandatory during design: scratchpad-style checklist of implementation items, grouped roughly by sub-agent.

At session end, if design seems mature, suggest promoting to implementation stage and gate on user.

## Stage `implementation`

Design is locked, but discoveries deferred to implementation may still change it.
Unexpected surprises may force a change on the fly — a higher bar.
Insurmountable blockers may stop the show — the highest bar.

Prioritize delivery with adaptation; corrective refactors can come later; necessary follow-ups can be put on the docket.
Exception: uncertain irreversible ops.

## Graduation

Spec gets closed when implemented.
Don’t reference closed spec.
