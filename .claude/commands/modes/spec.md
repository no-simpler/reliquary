---
description: Spec mode
disable-model-invocation: true
---

# Spec mode

User signals this session drives a docket spec (`docket guide spec`).

Address `blocked` first, if present.
Once cleared, drop the block and proceed.

## Stage `design`

Where warranted: extrapolate, evolve, augment; expecially where deferring to implementation is risky.
Bring up changes tersely.

Open questions allowed; must be settled before implementation, or explicitly deferred to it.
Mandatory before stage promotion: checklist of implementation items, grouped roughly by sub-agent.

At session end, if design seems mature, suggest promoting to implementation stage and gate on user.

## Stage `implementation`

Design not locked; discoveries can change it.
Prioritize delivery; refactors can come later;
exception: uncertain irreversible ops.

## Graduation

Spec gets closed when implemented.
Don’t reference closed spec.
