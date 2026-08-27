# `relic-core` — shared crate, not a relic

A workspace member under `crates/`, with **no `relic.sh`** on purpose: that is
what keeps `relic list|status|doctor`, the bootstrap snippet and `up` from seeing
it. It publishes nothing and owns no PATH name. See "The Rust lane" in
`~/.config/reliquary/GRADUATION.md`.

## The bar for living here

Code moves in when a **second** relic needs it, and not before. A crate that
collects what might one day be shared is a god crate, and every relic that
depends on it pays for the collection. `docket` and `midden` still have
overlapping `ui`, `render`, `id` and `field` modules; that is not sufficient
reason on its own.

**No dependencies.** Everything here shells out to git or reads the filesystem.
A dependency added here is pushed onto every relic that wants a project key.

## What is here, and why it is one crate and not two

`git` — git as a zero-sized capability. `Git::command` is the *only* sanctioned
constructor, because it is what strips the ambient `GIT_*` environment: `GIT_DIR`
outranks `-C`, so a relic run from a git hook would otherwise answer for the
hook's repository rather than the user's. It also forbids anything that can block
(`GIT_TERMINAL_PROMPT=0`, null stdin) — a relic runs from session hooks and from
`up`, where a credential prompt is a hang, not a question. `RELIC_GIT` is the seam:
a path overrides the binary, an empty value disables the layer so tests reach the
ungit path.

`path` — one meaning for a path. `resolve_lenient` is absolute and symlink-free as
far as the path exists and lexical past that, so a directory keys the same however
it was spelled and whether or not it exists yet.

They are one crate because `project_key` is the composition of both, and it is the
thing this crate exists for: `docket` and `midden` each assembled the halves
themselves and drifted apart. Anything that can be assembled differently will be.

`fs` — replacing a file's contents without ever exposing a partial one. Admitted on
the same evidence: both stores had written the identical tmp-then-rename by hand,
and both named the temporary with `with_extension("tmp")`, which *replaces* the
extension — so `a.md` and `a.json` shared one temporary, and two writers to one path
truncated each other. Centralising it was also the moment to make it correct:
a unique dot-prefixed temporary beside the destination, the parent directory synced
after the rename (without which the entry is not durable, which is the whole point),
and a drop guard so no error path leaves litter. It returns `io::Result` — the
no-dependency rule reaches the error type too, and callers add their own context.

## Adding a caller

Depend by path (`relic-core = { path = "../crates/relic-core" }`) and delete the
local copy — do not wrap it, and do not keep a fallback. A second spelling of a
shared key is the bug this crate closes.
