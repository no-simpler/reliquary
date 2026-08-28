# `relic-core` — shared crate, not a relic

A workspace member under `crates/`, with **no `relic.sh`** on purpose: that is
what keeps `relic list|status|doctor`, the bootstrap snippet and `up` from seeing
it. It publishes nothing and owns no PATH name. See "The Rust lane" in
`~/.config/reliquary/GRADUATION.md`.

## The bar for living here

**Split by kind — see the reuse ladder in `~/.config/reliquary/HARDENING.md`.**

**Platform code is reuse-first.** Colour resolution, atomic writes, locking,
subprocess-as-capability, PATH resolution: the second consumer exists by
construction, because every relic needs them. One implementation, adopted on
sight, so relics behave identically rather than each hand-rolling a ladder.

**Domain code keeps the second-consumer gate.** An item ladder, a prose metric, a
window state machine — the wrong abstraction costs more than the duplication, so
it moves in when a *second* relic actually needs it and not before. A crate that
collects what might one day be shared is a god crate, and every dependent pays
for the collection.

**Dependencies are judged, not counted.** Reach for a maintained ecosystem crate
before writing one; the counterweight is supply-chain hygiene (`cargo deny`), not
a budget. What does not belong here is a dependency pulled in for one relic's
convenience — a platform crate's cost is paid by every relic that wants a project
key.

## What is here

`tool` — an external program as a capability, and the generalisation `git` is
now built on. Holding a `Tool` proves the program is there. Two properties are
guaranteed **by construction**, because a caller who has to remember them will
not: the locale is `C` (house rule 1 — where no machine-readable interface
exists, the message must at least be the one the parser was written against),
and stdin is closed (a relic runs from hooks and from `up`, where a prompt is a
hang). `resolve` takes the override *value* rather than reading a variable, so
the seam is testable without mutating a process environment — the same shape
`ui::Format::resolve` takes, and the shape `unsafe_code = "forbid"` forces, since
`std::env::set_var` is unsafe in edition 2024.

`git` — git as a zero-sized capability. `Git::command` is the *only* sanctioned
constructor, because it is what strips the ambient `GIT_*` environment: `GIT_DIR`
outranks `-C`, so a relic run from a git hook would otherwise answer for the
hook's repository rather than the user's. It also forbids anything that can block
(`GIT_TERMINAL_PROMPT=0`, null stdin) — a relic runs from session hooks and from
`up`, where a credential prompt is a hang, not a question. `RELIC_GIT` is the seam:
a path overrides the binary, an empty value disables the layer so tests reach the
ungit path. `detect` resolves the program on PATH once per process and no longer
forks `git --version`: `which` proves an executable file, and a git that is
present but broken fails the first real invocation with its own message, which is
more legible than `detect` returning `None` — and that keeps a fork off the
session-start hook path.

`path` — one meaning for a path, and the crate's path *type*. Everything here is
`camino::Utf8Path`, because a key is program data: `to_string_lossy` maps two
directories onto one key, and serde's `PathBuf` refuses a path it cannot spell deep
inside a save rather than at the edge. `utf8`, `cwd` and `home` are the only places a
filesystem path becomes one. `resolve_lenient` is absolute and symlink-free as far as
the path exists and lexical past that, so a directory keys the same however it was
spelled and whether or not it exists yet — and a symlink that resolves somewhere
unnameable is *reported*, because skipping it would return a different key rather
than no key.

`git` and `path` are one crate because `project_key` is the composition of both,
and it is the thing this crate started as: `docket` and `midden` each assembled
the halves themselves and drifted apart. Anything that can be assembled
differently will be.

`ui` — one answer to "who is reading this". `Format` (human / agent / json) and
`ColorChoice`, resolved by the ladder `AGENTIC-TOOLING.md` states: an explicit flag,
then the relic's own `<NAME>_UI`, then `CLAUDECODE`, then tty-ness. Colour
detection is `anstream`'s, the same ladder clap walks, so no relic re-derives
`NO_COLOR`/`CLICOLOR_FORCE`/`TERM=dumb`. `docket` and `midden` had this twice and
it differed by 25 lines — the env var name and one helper. Ambient authority is
injected: `Format::resolve` takes a `FormatInputs`, and `from_process` is the one
place that reads the environment, so every branch of the rule is testable without
mutating a process no two tests can safely share.

`fmt` — one spelling for the quantities relics report. `age`/`age_days`/`plural`,
with the **clock as a parameter**. A function that reaches for `Timestamp::now()`
can only be tested at the resolution of the machine's clock, and a render loop
that reaches for it per row can report two different "now"s in one table — which
is why `View` carries one instant for the whole frame.

`lock` — advisory file locking with the rule made unrepresentable. **Bound the
wait, never the hold**: `Wait` has no `Forever` variant, so an unbounded wait
cannot be asked for. Both stores took `File::lock()`, which waits forever — a
`docket set` blocked on another session was a hung terminal with no way out.
`try_acquire` is a separate entry point rather than `acquire(…).ok()`, because
`.ok()` folds a real filesystem error into "somebody else has it", which is the
silent-failure shape this crate exists to close. std's own file locking is used
directly: it went stable in 1.89, and an ecosystem crate that only re-exports it
is a dependency for nothing.

`fs` — replacing a file's contents without ever exposing a partial one. Admitted on
the same evidence: both stores had written the identical tmp-then-rename by hand,
and both named the temporary with `with_extension("tmp")`, which *replaces* the
extension — so `a.md` and `a.json` shared one temporary, and two writers to one path
truncated each other. Centralising it was also the moment to make it correct:
a unique dot-prefixed temporary beside the destination, the parent directory synced
after the rename (without which the entry is not durable, which is the whole point),
and a drop guard so no error path leaves litter. Every filesystem call goes through
`fs_err`, so the `io::Result` it returns already names the path; a caller's context
supplies the verb.

## Adding a caller

Depend by path (`relic-core = { path = "../crates/relic-core" }`) and delete the
local copy — do not wrap it, and do not keep a fallback. A second spelling of a
shared key is the bug this crate closes.
