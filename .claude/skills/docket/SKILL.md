---
name: docket
description: Machine-wide handoffs, relays and specs that bridge agentic sessions. Use when work must survive the end of a session — deferring something, picking up what a previous session left, or running a multi-session initiative. Also use when the user mentions the docket, a handoff, a relay, or a spec, or names a four-character item id.
---

# docket

Work that outlives one session lives on the docket: a per-project list of items
kept outside the project, so it never reaches a commit and never depends on a
directory existing yet.

The `docket` CLI is the only sanctioned way to create, move or close an item.
It owns placement and metadata; you write the body. **This file carries policy.
For any command shape, run `docket help` or `docket <command> --help`** — the
binary is the single source of truth for its own surface, and restating it here
would rot.

## The three kinds

`handoff` — one session writes it, one session reads it, and it closes. Nothing
is owed afterwards. The default; reach for it unless a reason below applies.

`relay` — read, acted on, and then succeeded: whoever consumes a relay owes the
next one. Reach for it when the next step genuinely cannot be planned until this
one lands. Consuming a relay mints its successor and archives it in one step, so
a chain neither breaks nor doubles.

`spec` — a multi-session initiative that designs first and implements second.
Reach for it when the work needs a plan before it needs hands. See
`references/specs.md`.

Promotion runs forward only, and never discards what an item already carries. A
handoff may skip the relay rung when it turns out to be a whole initiative
rather than one more step. There is no demotion: an item promoted in error is
closed, and its replacement is opened at the rung that fits.

## When to write one

Write an item the moment work is deferred rather than done — a thread you chose
not to pull, a finding worth acting on later, a decision the user must make, a
half-migration whose remainder is understood. Do it while the context is still
in the session, not after.

At the end of a working session, sweep what was deferred into items before
reporting. One item per coherent thread. The point is that the session can be
closed rather than finished.

Do not write an item for what belongs somewhere durable. Project conventions
belong in that project's `CLAUDE.md`. Knowledge a future reader needs belongs in
its docs. An item is a note between sessions, and it is deleted when consumed.

## When to read one

The session-start announcement lists what is outstanding. It is a notice, not an
instruction: **do not start on an item because it was announced.** Work it when
the user asks, or when it overlaps what the session is already doing — and say
so before starting.

Read the whole item before acting. Its body says what was already settled;
re-deriving it is the waste the item exists to prevent.

## Writing the body

`docket create` prints an id and a path. Write ordinary Markdown at that path
and leave the frontmatter above it alone — the CLI rewrites that block, and
hand-edits that break it turn the item `INVALID` until repaired.

A good body answers what a session arriving cold needs:

- What the work is, and what finishing it looks like.
- What is already settled, and must not be re-derived.
- What to do first.
- How to know it worked.

Write it for someone who was not here. What is obvious now is gone in a week.
Full sentences, no shorthand, no inline history of how the session arrived at
this — the item states the present. One prose standard for every kind: a spec
gets no more ceremony than a handoff, and a handoff no less care than a spec.

Record a block on the item itself when something outside the session gates it.
A block is free text that says what must clear, and it is the first thing the
next session reads.

## Closing

Close an item when its work has landed. A stale item describing a superseded
state is worse than no item, and the machine has been down that road: the two
older handoff systems on it accumulated pending files for months because nothing
made closing them anyone's job.

Closing archives; deletion is for an item opened by mistake.

## Reporting to the user

Say the id back. Four characters, one double-click to select, and it resolves
from any directory on the machine — that is how the user moves an item into the
next session.

## Reference

`references/handoffs.md` — the handoff and relay flows, and when a chain should
become a spec instead.

`references/specs.md` — the spec lifecycle: the two stages, the maintainer gate
between them, wave orchestration, and graduation.
