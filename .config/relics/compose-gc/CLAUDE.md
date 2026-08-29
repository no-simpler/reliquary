# `compose-gc` — reclaim what a removed worktree left in the daemon

A worktree removed without tearing its stack down first leaves Docker state
behind with the compose file gone. `docker compose down` needs that file, so the
leftovers are permanently out of its reach: a surviving stack keeps a headless
browser (sometimes a whole database) alive, and even a fully-exited one pins its
volumes for good. Sweeping by **label** needs no compose file.

Published onto PATH; the `transplant-worktrees` skill is its caller, and its
single Docker surface.

## Two shapes of leftover

A container-only sweep goes blind the moment a stack's containers are gone and
its volumes are not — which is the steady state after any teardown that omitted
`-v`. So there are two:

- **Abandoned** — containers whose `com.docker.compose.project.config_files`
  label points into a worktree of this repository that no longer exists.
- **Stranded** — a project with no containers left at all. Volumes carry no
  config-file label, so there is no directory to trace: the provenance test is
  the **volume shape**, the set of declared names with the project prefix off,
  which must equal the main stack's. That is what holds a sibling repository's
  cache and an IDE's helper volumes out of range, and why the sweep no-ops
  safely when the main stack has never been raised.

Liveness is checked twice, because either alone is wrong: the worktree directory
still being there, and git still registering it.

`src/docker.rs` is everything the daemon is asked, `src/layout.rs` which stacks
are this repository's, `src/plan.rs` what that makes of them, `src/render.rs` how
it reads. Planning is **pure over one snapshot**, so `--dry-run` and a real run
are one computation rather than two that agree.

## Two transcribed tables, and how to re-derive them

Both carry the obligation `~/.config/reliquary/HARDENING.md` puts on a table
read out of a third-party binary. Both were **read against Docker Compose
v5.1.2**.

**Compose's project-name normalization** (`ProjectName::derived`) — lowercase,
drop everything outside `[a-z0-9_-]`, trim leading `_` and `-`. Re-derive by
raising a trivial stack from a directory named for each case and reading `name`
back:

```
docker compose --project-directory <dir> config --format json
```

**The compose file names** (`compose::CANDIDATES`) — `compose.yaml`,
`compose.yml`, `docker-compose.yaml`, `docker-compose.yml`. Re-derive by putting
exactly one of them in an empty directory and asking
`docker compose --project-directory <dir> config -q` whether a project is there.

Neither table decides anything on its own: the normalization is only ever a
*fallback* for a name Compose was not asked for, and the file list only ever
tells an absent stack from a broken one.

## Deviations from `bin/compose-gc`

The method is in `~/.config/reliquary/HARDENING.md`; this is the list.

| deviation | why | pinned by |
| --- | --- | --- |
| Reclamation is **per project**, containers first and all of them | The script reclaimed after *each* container, so a two-service stack asked for its volumes back while its own second container still held them. Reproduced live: three abandoned projects, five refusals printed, **exit 1 on a sweep that had actually succeeded** | `every_container_goes_before_the_first_volume` |
| One project is **one** entry in the count | The same per-container loop counted three projects as five and re-printed each project's volumes once per container | `plan::tests::a_project_is_one_reaping_however_many_containers_it_has` |
| A failed teardown is classified by **filesystem and postcondition**, never by Docker's English | `*"no configuration file"*` and `*"Resource is still in use"*` are messages meant for a person: reworded or localized, the first silently reports success and the second silently reports failure as success | `a_stack_that_will_not_load_is_a_failure_not_an_absence`, `a_resource_that_outlives_a_teardown_is_named` |
| A resource that outlived a teardown is **named** | The script echoed Docker's whole output and left the reader to find it | `a_resource_that_outlives_a_teardown_is_named` |
| A refusal carries the daemon's own message | Every removal sent stderr to `/dev/null`, so `could not be removed` was the whole diagnosis | `render::tests::a_refusal_carries_the_daemons_own_words` |
| `down <path>` on a directory that is not there is a **usage error** | It reported `nothing to tear down`, so a typo passed for a clean teardown. The skill calls `down` *before* `git worktree remove`, so the path is always meant to exist | `tearing_down_a_directory_that_is_not_there_is_a_misuse` |
| `down --dry-run` says whether there is anything there | It claimed it *would* tear down any path at all, having checked nothing | `a_directory_designating_no_stack_is_nothing_to_tear_down` |
| The main project's name is **asked**, not assumed | The script compared a project name against the repository's directory name verbatim. Compose lowercases and strips, so a capitalized repository matched nothing and disabled the stranded sweep outright — and `name:` in the file or `COMPOSE_PROJECT_NAME` in the environment outrank the directory either way | `compose_names_the_main_project_when_its_stack_is_down` |
| A live worktree's **declared** name takes it out of range | A stranded volume set is matched on name alone, so a live worktree whose compose file sets `name:` was this sweep's to destroy. Paid for only when something is otherwise about to be reaped | `a_live_worktrees_declared_name_takes_it_out_of_range` |
| Liveness is tested on the **worktree root**, not the compose file's own directory | A stack declared in a subdirectory of a removed worktree was judged by a path nobody had removed | `layout::tests::a_compose_file_below_a_worktree_resolves_to_that_worktree` |
| A project this repository only **half** owns is reported, not reaped | The script decided per container, so one container it could not place did not stop it removing the rest | `plan::tests::a_project_this_repository_only_half_owns_is_reported_not_reaped` |
| Container labels are read as **JSON**, in a second pass | `docker ps --format` returns label columns as delimited text, and `config_files` is itself a comma-joined list of arbitrary paths | `every_container_goes_before_the_first_volume` (the argv contract) |
| Every daemon call is **time-bounded** | The script would wait forever on a daemon that had stopped answering, from a skill that runs unattended | `docker::{QUERY, REMOVE, TEARDOWN}` |
| Output ordering is by project name | Docker's answer order is not a promise, and a report that reorders itself between runs cannot be diffed | `plan::tests::the_order_is_the_same_whatever_order_docker_answered_in` |

## Known limits, recorded rather than fixed

A leftover consisting of **a network alone** — no containers, no volumes — is not
swept. There is no provenance test available for one: a network carries the
project label but nothing that ties it to a directory, and the volume shape that
authorizes the stranded sweep does not exist. `docker compose down` removes the
network, so this is the residue of a teardown that failed rather than of one that
was skipped, and the abandoned sweep covers it whenever a container survives too.

The sweep is **not side-effect-free**, unlike the `assay` stations: it talks to
the daemon, which on a machine configured for it may start the daemon. That is
inherent — the job is mutating Docker state.
