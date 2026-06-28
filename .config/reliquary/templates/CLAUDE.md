# Agentic-pattern template bank

This directory (`~/.config/reliquary/templates/`) is Reliquary's canonical bank of reusable
templates for agentic project patterns. It exists because the same patterns — the dreaming
maintenance procedure, the root standing-directive brain — recur across every project we own but
drift laterally when each project reinvents them. The bank is the **hub**: a single source the
patterns flow out of, and back into.

It serves sibling repos the way `BEDROCK.md` and `GRADUATION.md` do, which is why it lives under
`reliquary/` rather than under Reliquary's own `.claude/`.

> **`template/` vs `templates/`.** `~/.config/reliquary/template/` (singular) is the *relic
> skeleton* — unrelated. `~/.config/reliquary/templates/` (plural) is *this* bank. Don't conflate them.

## What's in the bank

- `DREAM.md` — the dreaming (periodic self-healing maintenance) procedure.
- `ROOT-CLAUDE.md` — the root standing-directive brain + its always-load satellites.
- `PROPAGATE.md` — the runbook for fanning a template update out to projects (see use-case 4).

Each template is a **menu with a spine**, not a finished file (see below).

## The menu-with-a-spine rule

A template is a *union* of ideas, not a contract to adopt wholesale. Every template is organized as:

- a **`[CORE]`** spine — near-universal directives, kept almost verbatim by every project; and
- **`[OPTIONAL — <when>]`** modules — broadly-but-not-universally applicable ideas, each tagged
  with the condition under which it's worth keeping.

**Instantiate by subtraction.** Keep the spine, keep the optionals whose "when" matches the
project, delete the rest, and fill in `<placeholders>` with the project's real content. A project
never pulls every idea blindly just because the template lists it — the template offers; the
project chooses.

## The hub-and-spoke lifecycle

The template is the hub; each project's second brain is a spoke. Four use-cases, two directions:

1. **Establish** (hub → one spoke). A project has no such procedure yet: instantiate the template
   into it by subtraction.
2. **Bring up to date** (hub → one spoke). A project already has the procedure: reconcile it
   against the current template — fold in new/changed spine directives, offer newly-relevant
   optionals, preserve the project's own content.
3. **Promote innovation back** (spoke → hub). A project improves its own procedure and the
   improvement looks widely applicable: generalize it and fold it back into the template (strip
   all domain specifics first — see scrubbing). This is how the bank stays current. It usually
   happens in that project's own session.
4. **Centrally propagate** (hub → many spokes). After an improvement lands in a template, a
   dedicated Reliquary session fans it out to every applicable project. This is what `PROPAGATE.md`
   automates: discover candidates, let the user pick, harmonize each pick. Do **not** overwrite —
   merge the template's intent into each project's existing file, preserving domain content.

The natural rhythm is: improve one spoke → promote up to the hub (3) → propagate the hub out to
the other spokes (4).

## Governance — the templates are living

- **Bidirectional, always.** Templates feed projects (1, 2, 4) *and* projects feed templates (3).
  A template is never frozen; treat a widely-applicable project innovation as a templating debt.
- **Templates are dream-pass-able.** The same compaction / defragmentation / deduplication
  discipline the `DREAM.md` template describes applies to the bank itself — run a pass over these
  files when they accrete cruft or contradiction.
- **Public-repo scrubbing (hard constraint).** Reliquary is public. Templates must be 100%
  domain-free — zero business / company / person names, zero private-project tool surface, zero
  identifying specifics. Source projects (including private ones) are *idea sources only*; every
  concept lands generalized. The bank openly uses meta-vocabulary (`agent`, `CLAUDE.md`, `second
  brain`, `dream pass`) — that is fine and already public in Reliquary's own brain. The
  committed-file deniability rule a template may *describe* governs *project* committed files, not
  these templates.

## Naming conventions

- A template that mirrors a file which the harness would auto-load (a real `CLAUDE.md`) is named
  with a guard prefix — `ROOT-CLAUDE.md`, like the depot/meta `DEPOT_CLAUDE.md` indirection — so
  the template itself is never mistaken for live instructions.
- This bank's own `CLAUDE.md` is named literally, *on purpose*: it auto-loads when an agent works
  inside `templates/`, putting these usage rules in context exactly when they're needed.

## Candidate future templates

Surfaced by the survey that seeded the bank; build them when a second project would benefit:

- Specs system (`.claude/specs/` multi-session design+implementation).
- Handoff system + `SessionStart` hooks (cross-session / cross-project coordination).
- LLM-wiki shape (index catalog, flat unique slugs, three-layer source→wiki→schema).
- Meta / aspect / depot submodule topology.
- `.claude/settings.json` aspect-permission template.
