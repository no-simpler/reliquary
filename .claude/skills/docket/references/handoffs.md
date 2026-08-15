# Handoffs and relays

The two lower rungs of the ladder. Both are single files consumed by one
session; they differ only in whether that session owes a successor.

For command shapes run `docket help ladder` and `docket <command> --help`.

## Choosing the rung

Open a **handoff** by default. Most deferred work is one pickup, however large:
a migration whose steps are all known, an investigation whose question is
already framed, a decision waiting on the user.

Open a **relay** only when the next step depends on the outcome of this one in a
way you cannot usefully guess. The test is whether you could write the second
item now. If you could, write a handoff and put both steps in it. If writing it
would be invention, open a relay.

Open a **spec** instead when the work needs design before hands — when the shape
is contested, when there are locked decisions worth recording with their
rationale, or when it will take more sessions than you can enumerate.

A relay that has run several hops without converging is telling you it was
always a spec. Promote it; the chain provenance survives.

## The handoff flow

Write it, and it waits. A session picks it up, does the work, and closes it.
Nothing else happens.

If the pickup session discovers the work is larger than the item claims, it does
not silently expand scope. Either it does the work and says so, or it updates
the item to describe what it now knows and leaves it open.

## The relay flow

A relay is consumed with `docket relay`, which does three things at once:
opens the successor, carries the chain identity and hop number forward, and
archives the predecessor. Never open the successor by hand and close the
predecessor separately — that is how a chain silently forks or loses its
provenance.

The successor is a fresh item and needs its own title and description. Do not
title it "continued" or "part 2": name what it now asks for. Its body is written
from what this session learned, not copied forward. Carry forward only what is
still live; the predecessor is archived, and nothing reads it again.

Set a block on the successor when the next session is gated on something
external. Finishing a hop with a block recorded is a complete outcome, not a
failure.

## Cross-project items

An item can be opened for any project, including one whose directory does not
exist yet — the key resolves the same way once it does. The item records where
it was written from, so a finding raised in one project and owed to another
keeps its provenance.

This is the sanctioned channel between projects that cannot read each other. Do
not reach into a sibling project to leave a note in its tree.

## Repair

An item whose frontmatter stops parsing stays in every listing marked `INVALID`,
with the line and column at fault. `docket doctor` reports it; `docket set`
rebuilds the frontmatter around whatever still reads. Repair it when you see it
rather than working around it — an item nobody can list is an item nobody will
close.
