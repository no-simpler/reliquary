# `mantra`

Session modes. A mode is a reusable paragraph of behavioural directives, switched
on for a session by a `+token` at the start of a prompt line.

**The binary is the single source of truth for its own surface.** Reference lives
in `mantra --help` and `mantra help <topic>`; doctrine lives in `mantra guide
<topic>`. Do not restate either here, in a README, or in a code comment — a second
copy is a copy that goes stale while reading as though it had not.

## What replaced what

`~/.claude/hooks/modes.py` was a pure expander: one `UserPromptSubmit` hook, one
delivery, at one moment. Everything below is a deviation from it, and each names
the test that pins it.

| Deviation | Why | Pinned by |
|---|---|---|
| Modes live in `modes/`, not `commands/modes/` | The richer metadata needs a schema Claude Code's command parser does not own and may tighten. `description` and `disable-model-invocation` are refused, so a half-migrated file is loud | `mode::tests::a_stale_key_from_the_command_lane_is_refused` |
| Lookup is one `join` per root, not a recursive glob | The expander's cost was a function of the whole tree's size. Nothing nests a mode, and the bound is worth more than the possibility | `resolve::tests::a_nested_file_is_not_reachable` |
| A mode file may carry no metadata at all | Instantiate by subtraction: the default schedule is what the expander did | `mode::tests::a_file_without_metadata_takes_the_default_schedule` |
| A subagent is never answered | Its window is its own, and its modes are not this session's | `a_subagent_is_never_answered` |
| The `/modes:<name>` slash lane is gone | Two entry points over one file, one of which could not carry a schedule | — |

## Decisions worth not relitigating

**The first delivery is always the body.** A refrain is for *re*-delivery. This is
what makes a deferred mode work: `paced` carries no `on: activate`, so it says
nothing until the session is large enough, and when it does it says the whole
thing rather than an abbreviation of something nobody has read.

**Compaction is not a trigger.** Restating every active mode after the context was
summarised away is a property of the runtime, not a choice a mode gets to make — a
mode that opted out would stop existing at the first `/compact`. Keeping it out of
the vocabulary is also what keeps the vocabulary honest: every entry in it is a
decision a mode author actually has.

**A trigger clause is a one-key mapping, not an externally tagged enum.** YAML
spells one of those `!Tag value`, and the corpus is written as mappings. The
hand-written `Deserialize` in `trigger.rs` buys the error message: a misspelt
clause is answered by naming the three that exist.

**A mark ahead of the window is clamped back onto it** (`schedule::advance`).
Compaction shrinks the window and the transcript reporting it is written
asynchronously, so a mark set from a lagging read would otherwise sit permanently
out of reach and its clause would never fire again. The clamp solves that class
rather than the one case.

**State is a cache; the transcript is the record.** Every path that finds no usable
state rebuilds by re-reading `+token` activations out of the transcript, where they
were written in the user's own words. Losing the state file costs a rebuild, never
a mode. That is also the fork story: a fork gets a new session id and therefore no
state at all.

## Constraints

**Nothing may reach stdout except an injection.** `UserPromptSubmit` promotes any
exit-zero stdout straight into the model's context, so an error printed there is an
instruction to the model. `hook` answers `None` on every failure and `main` prints
errors to stderr.

**Nothing may be read from beside the executable.** `install_on_path` copies the
built binary onto `$PATH`; a path relative to it points at a copy.

**A rename is a rename.** No serde alias, no fallback read, no dual spelling.

Every test sets `MANTRA_ROOT`, and `HOME` points at the same scratch tree — the
resolver reads `$HOME/.claude`, so a test that sets only one of them reads the live
corpus.
