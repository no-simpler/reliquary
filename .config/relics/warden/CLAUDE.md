# `warden` — the commit guard

Refuses staged content that must never reach a public tree. Published onto PATH
and invoked by `~/.config/yadm/hooks/pre_commit`, which is three lines and a
break-glass; everything it decides is here.

## Shape

- `src/definition.rs` — *what* counts, read from the encrypted
  `yadm/hooks/identity-guard.toml`, and the one place its parts compose. Two
  composers is how two consumers of one definition come to disagree.
- `src/scan.rs` — the test. Knows nothing about where the definition came from,
  or about git.
- `src/config.rs` — what this machine has decided to leave alone
  (`~/.config/warden/config.toml`, public). Absent means guard everything.
- `src/staged.rs` — the set a commit is about.
- `src/main.rs` — argument parsing, one loop, and the report.

A library plus a thin binary, because the thing that invokes it changes and the
logic should not: today a yadm hook, tomorrow an ordinary git hook or a station
inside an aggregator.

## git, and which repository it answers for

`relic_core::git::Git` strips the ambient `GIT_*` environment so a relic run
from a hook does not answer for the hook's repository. Here that repository is
precisely the question, so `staged.rs` goes through `relic_core::tool::Tool` and
inherits the environment — the same call `ernest --changed` makes, for the same
reason.

## Fail-closed, and its sharp edge

yadm runs its *own* `pre_commit`, and `git commit --no-verify` does not skip it.
A missing binary would leave you unable to commit the fix for it. Hence
`YADM_HOOK_BREAK_GLASS=1`, which logs to `~/.local/state/warden/break-glass.log`
and says so on stderr. Loud, traced, and not a flag to reach for twice.

Everything else refuses: an absent definition, one that parses but would match
nothing, one that will not compile, an unknown key in either file, and content
that is not text.

## Deviations from `yadm/hooks/pre_commit`

The method is in `~/.config/reliquary/HARDENING.md`; this is the list.

| deviation | why | pinned by |
| --- | --- | --- |
| The **staged set**, not every tracked file | A guard over what is being committed was doing ~100× the work on every commit; that was the 2–4 s, not the regex. The whole-tree sweep is a standing audit and belongs to one | `staged::an_unstaged_file_is_not_this_commit` |
| Binary content is **refused by default**, with an allowlist in `config.toml` | The old hook prompted the human per binary file. Under a non-interactive commit `read` failed, `CONFIRM` came back empty, and it aborted reporting *"Commit aborted by user."* — a fail-closed that lied about why | `unreadable_content_is_refused_rather_than_skipped`, `an_allowed_binary_passes` |
| Paths are **NUL-delimited** | `for FILE in $FILES` was unquoted, so a tracked path with a space in it broke the guard into fragments. In the hook whose job is preventing leaks | `a_path_with_a_space_is_one_path`, `staged::tests::a_nul_delimited_list_keeps_paths_with_spaces_whole` |
| Terms are matched **literally** | The old hook interpolated each keyword into `grep -i`, so a `.` in one would have matched any character | `a_term_is_matched_literally` |
| One pass per file, not one per term | The old hook re-grepped every matching file once per keyword, forking each time, on top of a `file` fork per file | — |
| The alfred binary carve-out is **gone** | Nothing under it is tracked any more; it was dead code guarding a path that does not exist | — |
| An empty staged set **says so** | Legal, but the same output would follow from git answering for the wrong repository, and silent failure is the one outcome the critical path forbids | `staged::an_empty_staged_set_says_so` |
