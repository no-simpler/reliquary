# Reliquary - Dotfile Repository

Public dotfile repo managed by [yadm](https://yadm.io/) (yet another dotfiles manager).
Repo: [`no-simpler/reliquary`](https://github.com/no-simpler/reliquary).

yadm is a thin git wrapper whose work tree is `$HOME` and git dir is `~/.local/share/yadm/repo.git`.
All standard git commands work via `yadm <cmd>`.
Only explicitly `yadm add`-ed files are tracked; everything else is ignored.

## Bedrock

The **bedrock** is Reliquary's guaranteed substrate — the minimal set of system-wide deps ensured
present, configured, and fully PATH-accessible (with their sub-APIs) on every machine, so every other
repo the author owns may assume it without re-checking. When a tool needs more, it dockerizes rather
than growing bedrock. Members (v1): **bash** (>=5 on PATH — macOS `/bin/bash` 3.2 is shadowed, not
used), **python3** (latest, *never* minor-pinned; brew owns the interpreter and self-heals on `brew
upgrade`; uv supplements it but doesn't own it; per-app floors live in relic manifests' `MIN_RUNTIME_VERSION`),
**uv** (+`uvx`), **docker** (full API: CLI +`compose`+`buildx`; any impl), **git**, **curl**, **just**
(latest, never pinned — the host-side task entrypoint into every repo, and the ignition for the
docker-isolated ones), **cargo** (latest, never pinned — rustup owns the toolchain the way brew owns
python3; sub-APIs `rustc`, `cargo fmt`, `cargo clippy`, because `relic test` runs the latter two and
the relic lane builds from source).

- **Install:** base `brew/Brewfile`, members tagged `# bedrock` (macOS only for now; Linux is a TODO).
  `cargo` is the one exception — rustup owns it, installed by `yadm/snippets/shared/11-rustup.sh`,
  untagged. The install mechanism is per-member; the guarantee is uniform.
- **Verify:** `assay bedrock` (cross-platform, offline, side-effect-free; exit 0/1/2).
- **Enforce:** wired into `yadm doctor` (so the dream pre-pass and `yadm update` cover it) and re-asserted
  at the end of bootstrap (`yadm/snippets/shared/98-bedrock.sh`).
- **Bootstrap caveat:** snippets are sourced into the running (stock 3.2) bash, which the early
  modern-bash install does *not* upgrade mid-run — so **every bootstrap snippet stays bash-3.2-safe**.

Full philosophy and the contract for sibling repos: `~/.config/reliquary/BEDROCK.md`.

## Encryption

Sensitive files are GPG-encrypted into `~/.local/share/yadm/archive` and tracked in the public repo.
Patterns are listed in `~/.config/yadm/encrypt`.
The password for `yadm encrypt`, `yadm decrypt`, and `yadm verify` is fetched from 1Password implicitly (Touch ID prompt).
The `yadm-wrapper` script (see below) tracks archive SHA256 in `~/.local/state/yadm/last_decrypted` to detect encrypt/decrypt drift.

**Convention:** Encryption patterns in `encrypt` are intentionally obfuscated — they should not reveal what they protect. When adding new patterns, use opaque names that don't hint at content. Do not describe or document the contents of encrypted files in any tracked file (including this one). Future sessions can read encrypted file contents locally after decryption. The same convention governs `~/.config/yadm/unmanaged` (see "Deliberately not tracked" below): a path kept out of both lanes still gets a reason, and the reason must not name what the path is for.

**The identity guard.** What may never reach the public plaintext tree is defined once, as data, in `~/.config/yadm/hooks/identity-guard.toml` — encrypted, because the definition names what it protects. `identity-guard.py` beside it is the only reader and the only place its parts compose; a second composer is how two consumers of one definition come to disagree. Two consumers read it through that: `warden` refuses a commit whose staged files trip it, and `assay`'s `yadm-coverage` station runs the same test backwards over the whole tree, treating a hit on an *untracked* file as positive evidence that it belongs in the encrypt lane. Never restate that definition anywhere else, and never inline it into a public file. It is unavailable before the first `yadm decrypt`; both consumers say so rather than silently passing, and both assert the pattern is non-empty before using it — an empty regex matches every line, which would accuse every file instead of failing.

## yadm operations

**Tracked-file discovery.** The work tree is `$HOME`, not the cwd. Tracked files live across `$HOME` — `.bashrc`, `.zshrc`, `.bash_profile`, `.zshenv`, `.bash_env`, `.config/...`, `.ssh/config`, `.local/share/yadm/archive`, etc. Don't assume a file at `$HOME` root is untracked just because it sits outside the cwd. Check tracking with `yadm ls-files <path>`. See full dirty state with `yadm status` (always reports relative to `$HOME`).

**yadm is whitelist-based (footgun).** yadm blanket-ignores `$HOME` and tracks only files explicitly `yadm add`-ed. A file you just created is therefore **not** under version control until you add it — never assume a new file is tracked; verify with `yadm ls-files <path>` and add it deliberately. Conversely, a clean `yadm status` means "nothing *tracked* changed", not "nothing worth saving". There is also no usable blanket add: `yadm add -A` / `yadm add .` will not pick up new files (they're ignored), so every new path must be named explicitly in `yadm add`.

**Encrypted files are invisible to `yadm ls-files`.** Some files are tracked only *inside* the encrypted archive (`~/.local/share/yadm/archive`), not as individual git entries — so `yadm ls-files` will not list them. The patterns in `~/.config/yadm/encrypt` are also **not** authoritative for what is actually archived (a pattern may match nothing on this machine). To enumerate exactly what is encrypted-tracked, list the archive: `yadm decrypt -l` (Touch ID). The complete tracked set = `yadm ls-files` (plaintext) **plus** that archive listing — `yadm ls-all` (wrapper subcommand) prints both in one go (Touch ID, for the archive half).

**Reading `yadm status`** (easy to misread):
- `M ` / `R `: tracked file modified or renamed → almost always belongs in the next commit
- `A `: newly staged for tracking
- `??`: untracked (file exists but isn't `yadm add`-ed; deciding to track is a separate judgment call)

**Default staging policy.** When committing in this repo, stage every `M` / `R` line unless explicitly told to exclude one. Reliquary bundles whatever is dirty; splitting the working tree by topic isn't the house style. Skipping an `M` line on the assumption it's "unrelated" has historically been wrong.

**Path availability.** `yadm` is on `$PATH` in non-interactive bash and zsh: `~/.zshenv` and `~/.bash_env` (via `$BASH_ENV`) source the `env.d/*.sh` files, which put both Homebrew and `~/.config/bin` on `$PATH`. Crucially, `env.d/999-path.sh` runs **last** (highest-numbered) and forces `~/.config/bin` **ahead** of Homebrew — it has to come after 040-env's own gcloud/cargo/OrbStack prepends and after any pre-polluted inherited `$PATH`, which previously demoted it. `~/.config/bin/yadm` is a symlink to the wrapper — so bare `yadm <cmd>` resolves to the **wrapper** (not brew's yadm) in *every* shell, interactive or not, including wrapper-only subcommands (`check`/`verify`/`update`/`own`/`disown`/`ls-all`). No alias and no explicit `~/.config/bin/yadm-wrapper` path are needed anymore. (The wrapper finds the real yadm by scanning `$PATH` outside `~/.config/bin`, so it never recurses.)

**Authorization.** `yadm commit` and `yadm push` are pre-approved — run them yourself; never ask first and never hand the commit/push off to the user to run. Be aware that **`yadm commit` triggers a Touch ID prompt** — commits are SSH-signed through 1Password — as does `yadm encrypt`. If the user is AFK the prompt times out and the command fails; that just means the user is away, not a real error. Surface it plainly and let them retry when back — don't thrash, retry in a loop, or try to work around the signing.

**Unless a `ske` window is open** (see "Touch ID window" below): while one is, commit/push/encrypt run silently and the AFK failure disappears. Check with bare `ske`. You cannot open one yourself — `ske <duration>` needs a Touch ID, by design.

## Repository structure

### Shell configuration (`~/.config/shell/`)

Two-tier layout — the directory name *is* the contract:
- `~/.config/shell/env.d/` = **always-on** (PATH, tool init, locale, auth env). Sourced by `~/.zshenv` (every zsh) and `~/.bash_env` (every non-interactive bash via `$BASH_ENV`; also from `~/.bash_profile` for login bash). Idempotent — re-sourcing is safe.
- `~/.config/shell/interactive.d/` = **interactive only** (plugins, prompt, completions, aliases, update checks). Sourced by `~/.zshrc` / `~/.bashrc` behind their interactive gates.

Sourced-on-demand libraries do **not** live here — shared shell logic belongs to the subsystem that owns it, under `~/.config/reliquary/lib/` (see "Externally-managed PATH lane" below).

Filename suffix selects shell:
- `NNN-name.sh` = shared (bash + zsh only — fish cannot parse POSIX syntax)
- `NNN-name.fish` = fish-only
- `NNN-name.zsh` = zsh-only
- `NNN-name.bash` = bash-only

Numbering controls load order:
```
env.d/         : 040-env  070-fixes  150-benefactor  999-path
interactive.d/ : 020-plugins  030-config  050-prompt  060-fzf
                 080-check  090-funcs  100-aliases{,-git,-docker,-yadm}
```
Additional encrypted shell files may exist (see `~/.config/yadm/encrypt`).

When adding a new file, ask: does it set env / PATH that agents need (→ `env.d/`), or does it configure interactive UX (→ `interactive.d/`)? Side-effecting files (anything that prints to stdout, runs `tty`, or spawns subprocesses) belong in `interactive.d/` unless they can be made silent and idempotent.

Shell var `$D__SHELL` is set to `zsh` / `bash` / `fish` in the respective entry-point file (always, including non-interactively) and used throughout for shell-specific branching.

**Fish**: `~/.config/fish/conf.d/*.fish` is auto-loaded by fish regardless of interactivity (fish convention; no `env.d/` split). Interactive-only fish files self-gate at the top with `status is-interactive; or return`. Env-pure fish files (`040-env`, `070-fixes`, `150-benefactor`, `00-sdkman-guard`, `sdk`) intentionally have no gate.

Non-interactive entry points:
- zsh → `~/.zshenv` globs `env.d/*.sh`
- bash → `$BASH_ENV` (set to `~/.bash_env` by `~/.zshenv`) globs `env.d/*.sh`; `~/.bash_profile` also sources `~/.bash_env` for login bash
- fish → conf.d auto-load (env-pure files run unconditionally)

**Root-dotfile placement (`ZDOTDIR`).** `~/.zshenv` lives at `$HOME` root because zsh reads it when `ZDOTDIR` is unset at startup — so it's the one file that *can't* move (on a fresh login `ZDOTDIR` isn't set yet, and root `~/.zshenv` is what sets it). It exports `ZDOTDIR="$HOME/.config/zsh"`, so zsh's remaining startup files live inward as **`~/.config/zsh/.zprofile`** and **`~/.config/zsh/.zshrc`** (yadm restores them at those tracked paths on clone — no symlink/forwarder needed). **Caveat — `ZDOTDIR` can be *inherited*:** zsh reads root `~/.zshenv` *only* when `ZDOTDIR` is unset; a zsh that starts with `ZDOTDIR` already exported (a nested shell, or Claude Code's command shell launched from an environment that already set it) reads **`$ZDOTDIR/.zshenv`** instead and never touches the root file. So **`~/.config/zsh/.zshenv` also exists** and is load-bearing: it `source`s root `~/.zshenv` (keeping it authoritative) so `env.d` (PATH, tool init, locale, auth env) loads either way. Without it, inherited-`ZDOTDIR` zsh silently skips *all* of `env.d` — the bug that made bare `yadm` resolve to brew's instead of the wrapper. No double-source: a fresh login reads only root `~/.zshenv` (and setting `ZDOTDIR` mid-file doesn't trigger a second read in the same pass); an inherited start reads only `$ZDOTDIR/.zshenv`. Bash's `~/.bash_env`, `~/.bash_profile`, `~/.bashrc` **deliberately stay at root**: bash has no `ZDOTDIR`, so relocating them would require forwarder stubs or symlinks at root anyway — pure indirection with no decluttering. `~/.hushlogin` also stays at root (login reads it there literally). Don't try to "fix" the bash/hushlogin asymmetry; it's intentional.

**No `~/.profile`.** It's intentionally absent. cargo's PATH is owned by `shell/env.d/040-env.{sh,fish}` (which source `~/.cargo/env` / `fish_add_path`), so rustup's habitual `. ~/.cargo/env` injection into `~/.profile` is redundant cruft. The bootstrap installs rustup with `--no-modify-path` (`yadm/snippets/shared/11-rustup.sh`) so it never recreates it. If a `~/.profile` reappears, a rustup reinstall bypassed that flag — delete it.

Pre/post hooks: `~/.pre.{zsh,sh}` and `~/.post.{zsh,sh}` are sourced if present (not tracked; machine-local overrides).

### Personal bin (`~/.config/bin/`)

Executable scripts on `$PATH` (added via `env.d/040-env.sh`):
- `bbs` - interactive Brewfile scope selector (applies `Brewfile@<scope>` files)
- `gh` - shadows Homebrew's `gh` (same PATH trick as `yadm-wrapper`); exports `GH_CONFIG_DIR` to the benefactor profile when the *physical* cwd is under `~/Developer/benefactor/`, else leaves the personal default. See "Two GitHub identities" below
- `pb` - lists personal bin executables, shows which are yadm-managed
- `pm` - print message: typed, coloured terminal output (notice/info/success/warning/error), the interactive counterpart to the bootstrap's `util/00-print.sh`
- `timeout` - GNU-style `timeout(1)` shim, so scripts can rely on it being present
- `up` - system-wide updater (brew, rust, zinit, vim-plug, gcloud, tpm, relics, relic build cache); writes timestamp to `~/.local/state/up/last_upped_at`
- `compose-gc` - reclaims Docker Compose state left behind by dead git worktrees of the current repo, sweeping by label (no compose file needed) across both worktree layouts; `down <path>` is the profile-complete teardown of one stack. `-n` dry-runs. Used by the `transplant-worktrees` skill as its single Docker surface
- `gpg-yadm-op` - GPG wrapper that fetches symmetric passphrase from 1Password (Touch ID) for yadm encrypt/decrypt; tries `ske read` first, falls back to `op read` (never hard-depend — it decrypts the attic `ske` lives in)
- `ske-prompt` - prints the open `ske` Touch ID window for the oh-my-posh right prompt; silent when closed (`sh`, not bash: `$BASH_ENV` would cost ~230ms per render)
- `yadm-wrapper` - wraps yadm with custom subcommands (see below); also reachable as `yadm` via the `~/.config/bin/yadm` symlink (shadows brew's yadm — see "Path availability")
- Additional encrypted scripts may exist (see `~/.config/yadm/encrypt`)

### Externally-managed PATH lane (`~/.local/bin/`)

On `$PATH` via `env.d/040-env.sh` (the same loop that adds `~/.config/bin/`), but **not** YADM-tracked.
Canonical lane for executables managed by external meta-projects (halo, bb).

- A single registry file `~/.local/bin/.reliquary-managed` lists every managed binary, one per line, as `<name>[<TAB><owner>]`. The **owner** column is optional, per-entry provenance (the publishing meta-project). `#` comments and blank lines are ignored; membership is keyed on the first field.
- Do not `yadm add` anything from `~/.local/bin/`, including the registry file.
- Do not hand-edit the registry; it is written by the publish helper. (Legacy per-meta `.<name>-managed` files are folded into the single registry automatically — by bootstrap, `relic migrate`, and first publish.)
- The binaries are regenerable: re-run the owning meta-project's publish flow. See that meta-project's `CLAUDE.md` for the protocol.

The publish helper lives here: `~/.config/reliquary/lib/install-on-path.sh` (yadm-tracked, sourced on demand by each meta-project's publish scripts). Callers invoke it as `META_NAME=<name> source "$HOME/.config/reliquary/lib/install-on-path.sh"`; `META_NAME` is **optional** (when set it becomes the owner column and gates collision detection). **PATH names must be unique** — the helper fails fast if a name is already owned by a different relic, already resolves elsewhere on `$PATH`, or collides with a foreign file. One canonical implementation across all meta-projects — do not duplicate into individual meta-repos.

Sanctioned sidesteps: a meta-project may bypass the helper for advanced cases (template substitution, self-update, embedded provenance). The `bb` CLI is the canonical example. Those callers stay responsible for not stomping on YADM-tracked files.

### Relic graduation (`~/.config/relics/`, `~/.config/reliquary/`, `~/.config/attic/`)

Personal CLI utils have a three-stage lifecycle. A **relic** is a personal tool the author keeps:

- **Stage 1 — one-shot util**: single file in `~/.config/bin/` (status quo; `bbs`, `pb`, `up`, etc.).
- **Stage 2 — in-house relic**: directory at `~/.config/relics/<name>/`, yadm-tracked, with a manifest (`relic.toml`) and optional `src/`, `tests/`, `scripts/`. Published onto PATH via the shared lib. The `relic` CLI itself is the first Stage-2 relic.
- **Stage 3 — external relic**: independent repo at `~/Developer/<name>/` (`bb`, `halo` today). The dependency is strictly **unidirectional** (relic → reliquary, via `install-on-path.sh`). Reliquary's "known external relics" list in `GRADUATION.md` is a best-effort convenience, not authoritative; it can also discover registrants via the registry's owner column, but doesn't chase this exhaustively.

**Relics are Rust by default.** Any other `RUNTIME` records a `RUNTIME_EXEMPTION` in its manifest saying why; `relic doctor` lists the ones that have not, informationally and never as a failure, and that list is the worklist. Existing non-Rust relics (`relic`, `blab`, `nexus`, `ske`) are being addressed one at a time — rewritten into the workspace, or exempted.

**`~/.config/relics/` is a cargo workspace.** The lane root carries `Cargo.toml` (one `members` whitelist, `[workspace.dependencies]`, and the only `[profile]` blocks cargo honours), `Cargo.lock`, `rustfmt.toml`, and one gitignored `target/`. A relic carries none of those itself. `crates/` is the shared-library boundary — a member with no manifest, which is what makes it invisible to `relic`, to bootstrap, and to `up`, all of which gate on a readable manifest. `crates/relic-core` is the first: git as a capability (the constructor that strips the ambient `GIT_*` environment, so a relic run from a git hook does not answer for the hook's repository) and one meaning for a path (`project_key`, shared by `docket` and `midden`). Code moves there when a *second* relic needs it, not in anticipation. A Rust relic therefore carries **no `scripts/publish.sh`, no `scripts/test.sh`, and no `entrypoints/`** — the lib's rust branch builds, tests and publishes manifest-declared names out of the workspace `target/release/`.

**Never add an attic relic to `members`**: a member's name lands in the publicly tracked `Cargo.lock`, and `.config/attic/**` is an encrypt pattern that would tar its `target/` into the archive. `GRADUATION.md` records what to do instead, including the `yadm encrypt` exclusion to write *before* the first attic Rust relic exists.

The build cache is held down by `[profile.dev] debug = "line-tables-only"` and by `up`'s **Cargo build cache** step (`cargo sweep --installed`, then `--time 30`, above a 2 GiB ceiling, skipped silently below it or when `cargo-sweep` is absent). `yadm doctor` reports the size.

The `relic` CLI (`relic list|status|publish|test|mutants|update|scaffold|registry|migrate|doctor`) is the user-facing surface over all of this — see `GRADUATION.md`. `scaffold <name>` is how a relic starts — never hand-lay one without reading that reference first. It promotes a Stage-1 `~/.config/bin` util (or a fresh idea) into a Stage-2 relic — RUNTIME from `-r/--runtime`, else a promoted script's shebang, else `rust`; a non-Rust choice needs `-e/--exempt "<why>"`. A Rust scaffold writes a member `Cargo.toml` and appends to `members`; an interpreted one publishes and stages the result in yadm. `registry` takes `--migrate`/`--prune`; `doctor` is a read-only registry ↔ PATH ↔ entrypoints health check that also carries the runtime-stance worklist.

**The roster is `relic list`, not a list in this file.** Each relic's doctrine lives in its own `CLAUDE.md`, and the sections below exist only for the relics whose *system-wide* integration needs explaining — a hook, a command surface, a Touch ID vector. A relic that is simply a tool on `$PATH` (`ernest`, which measures prose density and backs `/modes:deprose`) gets no section here, because a hand-maintained roster in the root file is a roster that silently goes stale.

`~/.config/reliquary/` holds the meta — canonical docs (`GRADUATION.md`, `HARDENING.md`, `AGENTIC-TOOLING.md`), the shared libraries (`lib/relic.sh`, `lib/install-on-path.sh`), the relic skeleton (`template/`), the agentic-pattern template bank (`templates/` — note the plural, distinct from the singular relic skeleton).

### Session docket (`docket`)

Machine-wide handoffs, relays and specs — the transient items that bridge agentic sessions.
Public relic (`~/.config/relics/docket/`, Rust).

Items live in a per-machine depot at `~/.claude/docket/`, grouped by project and deliberately
untracked **by yadm** — the system around them (relic, hook, skill) is tracked and travels between
machines, the items themselves do not. A project keys to the **main checkout root** of its git
repository, so worktrees share one docket, and to the resolved working directory outside a
repository. Ids are four characters and unique across the machine, so one resolves from any
directory.

The depot is **its own git repository**, created on first use and never given a remote — history
is machine-local, like the items. docket commits every change it makes, and commits whatever was
edited through the path it prints before adding its own. Closing an item therefore *removes* it:
`git -C ~/.claude/docket log --diff-filter=D --name-only` is where a closed item lives, and docket
grows no command that restates that. The repository carries its own identity and
`commit.gpgsign=false`, so no depot commit ever reaches for Touch ID. Everything git-dependent
lives behind one module (`src/git.rs`), switched on by git's presence — without git, items can
still be opened and read, but none can be closed. *How* git is invoked is not docket's: that
belongs to `crates/relic-core`, along with the project key it shares with `midden`.

A `SessionStart` hook in `~/.claude/settings.json` runs `docket announce --hook`, which is silent
when nothing is outstanding. The skill at `~/.claude/skills/docket/` is a trigger stub, and its
`description` is the only channel that reaches an agent without a tool call.

**The binary is the single source of truth for its own surface** — reference in `docket --help` and
`docket help`, doctrine in `docket guide`. Do not restate either here or anywhere else; the relic's
`CLAUDE.md` records why.

### Friction corpus (`midden`)

Machine-wide record of what the harness cost an agent — a directive that was missing, contradicted,
stale, or a fact that had to be hunted for. Public relic (`~/.config/relics/midden/`, Rust).

Sibling to docket, and deliberately its inverse: docket carries transient work **forward**, midden
accumulates durable evidence **about the second brain itself**. Notes live in a flat, machine-local,
untracked corpus at `~/.claude/midden/`. Flat because cross-project pattern detection is the point —
`project` is a field, not a directory, and one ambiguous directive met from three repositories folds
into one note rather than three.

Nothing announces it. There is **no hook and no skill**: `SessionEnd` is observe-only and cannot
inject context, `Stop` fires every turn, and a corpus filled reflexively is one nobody reads. The
sole surface is `~/.claude/commands/modes/feedback.md` — `+feedback` as a standing directive, or
`/modes:feedback` invoked once at session end.

Size is policed at write time, not only by `gc`: field caps, plus a fingerprint over kind, target and
claim so a recurrence bumps a counter instead of writing a second file. `midden gc` runs from the
relic's `scripts/update.sh`, which `up` already invokes.

**The binary is the single source of truth for its own surface** — reference in `midden --help` and
`midden help`, doctrine in `midden guide`. Do not restate either here or anywhere else; the relic's
`CLAUDE.md` records why.

### Cruft sweep (`decruft`)

Removes inert OS metadata and interpreter caches. Public relic
(`~/.config/relics/decruft/`, Rust); `up` runs it as a step.

Two lanes, because "may this be deleted?" has two best answers. Inside a git repository the
**repository** answers — only ignored, untracked paths are candidates, so a per-repository
unignore is respected and a tracked file is never a candidate. Outside one (the XDG data dir)
there is nobody to ask, so the answer is by name and the set of names is small.

Editor swap and lock files stay: gitignored keeps them out of commits, but a live one is
crash-recovery state. Dependency and build trees stay — inert, but expensive to rebuild.
Vendored interpreter trees are skipped whole. A directory left empty is reported, never
deleted. `-n` dry-runs.

**The binary is the single source of truth for its own surface** — `decruft --help`. Its
`CLAUDE.md` carries the deviations from the shell script it replaced, each naming the test
that pins it.

### Machine verification (`assay`)

Every check that answers "is this machine still the one the repo describes" — one roster
of stations over one finding type. Public relic (`~/.config/relics/assay/`, Rust).

An **aggregator**, not a tool that absorbs everything. A station lives here only when
nothing else owns the check; a relic that knows its own invariants keeps them and answers
`doctor --format json`, which the registry adapter collects. The dependency direction never
reverses — `assay` reads a published protocol, never another repository's source.

`yadm doctor` is a thin caller: it runs `assay` and folds the exit status into its own,
keeping only the two archive checks that are genuinely dotfile-specific. `up` calls the
`brew-health` station, bootstrap's `98-bedrock.sh` calls `bedrock`, and `relic test`'s bash
branch calls `shell-lint`. Adding a check means adding a **station**, not extending the
wrapper's bash — `.claude/DREAM.md` says the same.

Detect-only, offline and side-effect-free unless `--deep`; exit `0`/`1`/`2` graded from the
findings, `3` when `assay` itself could not run.

**The binary is the single source of truth for its own surface** — `assay --help`, and
`assay --list` for the roster. Its `CLAUDE.md` carries the station contract, what was taken
from prior art and what deliberately was not, and one deviation table per retired script.

### Commit guard (`warden`)

Refuses staged content that must never reach a public tree. Public relic
(`~/.config/relics/warden/`, Rust), invoked by `~/.config/yadm/hooks/pre_commit`.

The hook is three lines and a break-glass; everything it decides is in the relic, so the
guard survives the hook changing. It reads the encrypted definition (see "The identity guard"
above) as data, and `~/.config/warden/config.toml` — public — for what this machine has
decided to leave alone. An absent config guards everything.

It gates the **staged set**, not the whole tree. The whole-tree sweep is a standing audit and
belongs to one.

**Fail-closed, with one escape.** yadm runs its *own* `pre_commit`, and `git commit
--no-verify` does not skip it — so a missing binary would leave you unable to commit the fix
for it. `YADM_HOOK_BREAK_GLASS=1` commits anyway, prints that it did, and appends to
`~/.local/state/warden/break-glass.log`. Loud and traced; not a flag to reach for twice.

**The binary is the single source of truth for its own surface** — `warden --help`. Its
`CLAUDE.md` carries the deviations from the shell hook it replaced, each naming the test that
pins it.

### Touch ID window (`ske`)

`ske` ("skeleton key") opens a **time-boxed window** in which 1Password Touch ID prompts are
suppressed for dev operations, then closes it automatically. It exists so long agentic
sessions stop needing a human at the fingerprint sensor. Attic relic (`~/.config/attic/ske/`);
full reference in its `CLAUDE.md`.

```
ske              window state      ske 1h    open (or re-arm to) 1 hour
ske off          close now         ske doctor  wiring + registry health
```

`ske <duration>` is **absolute** — it always means "end at `now + duration`", never "extend
by". Capped at 8h (`--force` exceeds). **Re-arming costs one Touch ID**, which is what keeps
window extension human-only: an agentic session cannot silently extend itself, because its
no-TTY shell always prompts.

**Why it sidesteps rather than relaxes:** 1Password's lock policy and `sshAgent.sshSessionDuration`
live in an HMAC-authenticated `settings.json` (hand-edits are rejected), `agent.toml` has no
authorization fields, there is no programmatic unlock, and service accounts can't reach the
`Private` vault. All verified — don't re-litigate. ske does one Touch ID-gated extraction into
its **own** ssh-agent (`ssh-add -t`) plus a memory-only broker, both expiring on a monotonic clock.

**The crux:** `op`'s own biometric cache is scoped **per TTY session**. Agentic shells have no
TTY, and neither do backgrounded post-commit hooks — so op's cache helps exactly the cases ske
exists for *not at all*.

**The roof.** `~/.config/attic/ske/ske.conf` is the authoritative inventory of every Touch ID
vector (`[keys]`, `[secrets]`, `[unwired]` — the last records vectors deliberately *not* routed
through ske, so a decision is recorded rather than forgotten). `ske read <op-ref>` is the single
sanctioned way anything gets a 1Password secret: integrating a future use-case is one word at
the call site (`op read` → `ske read`) plus a line in the registry. `ske doctor` sweeps for
`op://` refs that bypass ske and flags them, so a new vector can't hide; it's wired into
`yadm doctor`.

SSH needs no per-vector work: `~/.ssh/config` (host-scoped `Match exec`, predicate `ssh-add -l`)
and `gpg.ssh.program` → `ske-sign` cover every present and future SSH vector.

**Fail-closed by construction.** The window's authority is the agent's own key set — expiry, a
dead agent, or a stale socket all route back to 1Password. There is deliberately **no
`expires_at` file**; `grant.json` is display-only. Nothing to revert, nothing to corrupt.

**Costs, so they're not a surprise:** the `Match exec` block adds ~280ms per *git-host* SSH
connection, forever, window or not (non-git hosts pay 0ms — that's why it's host-scoped).

**Security.** While open, anything running as you can sign with your git keys and read the
brokered secrets with no prompt. Keys are never exported (ssh-agent grants *use*, not
possession). The realistic threat is a **forgotten window**, not an attacker — hence bare `ske`
status and the append-only log at `~/.local/state/ske/log`. `ske off` cannot flush 1Password's
own ~10-min per-TTY cache and says so. **ske is an interactive convenience, never an
unattended/CI auth path.**

### Two GitHub identities

A personal account and a benefactor account share `github.com`. Three independent mechanisms keep
them apart, and **all three must be right** — verify each separately, since any one of them can be
correct while another silently is not.

- **SSH key** — `~/.ssh/config` pins one key per host with `IdentitiesOnly`: `github.com` gets
  `id_personal.pub`, the `github-benefactor` alias gets `id_benefactor.pub` with
  `HostName github.com`. Both keys are valid GitHub credentials, so without the pin the *first key
  an agent offers* decides which account you are — and neither agent's offer order is declared
  policy (ske's follows `ske.conf`; 1Password has no `agent.toml`). The `IdentityFile` entries name
  **public** keys: with `IdentitiesOnly` they select which agent identity to offer, and the private
  halves never leave 1Password. `github-benefactor` is also in the ske `Match host` list.
- **git identity** — `includeIf gitdir:~/Developer/benefactor/` swaps in the benefactor name, email,
  and signing key, and rewrites the organization's remotes onto the `github-benefactor` alias via
  `url.insteadOf`. Scoped by directory, so nothing needs per-repository setup.
- **`gh` token** — the `gh` shim above, keyed on the physical cwd.

Both `.pub` files ride the encrypted archive, arriving in the same `yadm decrypt` as `~/.ssh/config`
itself — so there is never a moment on a fresh machine where the config pins an `IdentityFile` that
does not exist yet, which would offer no keys at all and break every SSH git operation.

The one manual step on a new machine is `gh auth login` for the benefactor profile (run it from
inside the benefactor tree so the shim points it at the right config dir), plus authorizing the SSH
authentication key for the organization's SSO. Both are interactive by nature.

Check them with `ssh -T git@github.com`, `ssh -T git@github-benefactor`, and `gh auth status` run
from inside and outside the tree. Note that `ssh -T` proves *which account*, not organization
access — SSO authorization only shows up on a real repository operation.

### Agentic templates (`~/.config/reliquary/templates/`)

Canonical bank of reusable templates for recurring agentic project patterns — the hub that pattern knowledge flows out of and back into, so per-project second brains stop drifting laterally. Each template is a **menu with a spine**: `[CORE]` directives every project keeps, `[OPTIONAL — <when>]` modules a project keeps only if applicable; instantiate by subtraction. Public-repo rule: templates are 100% domain-free (source projects are idea-sources only). Members: `DREAM.md` (dreaming procedure), `ROOT-CLAUDE.md` (root standing directives + satellites; `ROOT-` prefix prevents auto-load), `PROPAGATE.md` (centralized fan-out runbook) + `bin/discover-template-targets` (read-only target enumeration). Usage and governance live in `templates/CLAUDE.md`.

`~/.config/attic/` is the **private relic lane** — the whole subtree is encrypted (the `.config/attic/**` pattern in `~/.config/yadm/encrypt`). Same anatomy inside as public relics, and its own cargo workspace: a member's name and version land in its workspace's `Cargo.lock`, and the public one is tracked in plaintext. `!**/target` keeps the build tree out of the archive; the attic `Cargo.lock` rides along inside it deliberately. `[workspace.dependencies]` is not inheritable across workspaces, so the attic root redeclares what it shares — keep it a strict subset copied from the public table. An attic member may depend on `relics/crates/*` by path, and when a public relic's `relic test` covers a shared crate it additionally runs the attic workspace's format, lints and suite, because that is the one thing a lane boundary cannot carry. `up` and `yadm doctor` sweep and report both lanes' build caches.

Manifest-declared `BREW_DEPS` and `MIN_RUNTIME_VERSION` are **load-bearing**: `relic::check_deps` fails closed at publish time. When a relic graduates to Stage 3, its deps should be reflected in the appropriate Brewfile — the manifest stays the source of truth.

Bootstrap re-publishes all relics via `~/.config/yadm/snippets/shared/12-publish-relics.sh`. `up` runs `relic::update` per relic; opt out with `UP_SKIP_RELICS=1 up` or `up --no-relics`.

Full reference: `~/.config/reliquary/GRADUATION.md`.

### yadm wrapper (`~/.config/bin/yadm-wrapper`)

Reachable as `yadm` in **every** shell — `~/.config/bin/yadm` is a symlink to this script, and `~/.config/bin` is forced ahead of Homebrew on `$PATH` so it shadows brew's yadm (no alias involved). For bash/zsh that ordering is asserted **last** by `env.d/999-path.sh` (a mid-040 placement let later prepends re-win); fish does its own ordering in `040-env.fish`. Adds custom commands:
- `yadm own` / `yadm disown` - switch remote between SSH and HTTPS
- `yadm encrypt` / `yadm decrypt` - delegates to yadm + records archive SHA256
- `yadm check` - compares archive SHA256 to detect drift
- `yadm verify` - decrypts archive to tmpdir and diffs against disk
- `yadm ls-all` - complete tracked set: `yadm ls-files` (plaintext) + archive listing (`decrypt -l`, Touch ID)
- `yadm doctor` - dotfiles health self-check. The machine's checks are `assay`'s and reached by running it; what stays here is what is genuinely yadm's — encrypted-archive drift, and the `--full` archive verify — plus one `ske doctor` call, the only check `assay` cannot collect until `ske` answers `doctor --format json`. Detect-only and Touch-ID-free. `--full` adds the `verify` deep check; `--quiet`/`-q` runs silently and prints the report only on a failure/warning (flags compose). Used by the dream pre-pass (`~/.config/.claude/DREAM.md`) and, in `--quiet` form, by `yadm update`
- `yadm update` - `pull --ff-only`, then `doctor --quiet` (silent when healthy; surfaces drift/regressions the pull introduced — the quiet doctor already covers the encrypted-archive check)
- All other commands pass through to real yadm, followed by an encrypted-files check

Because the wrapper shadows brew's yadm on `$PATH`, bare `yadm <cmd>` — including wrapper-only subcommands — works in interactive *and* non-interactive shells alike (see "Path availability" above). The wrapper resolves the real yadm by scanning `$PATH` outside `~/.config/bin`, so it never recurses into itself.

### Bootstrap (`~/.config/yadm/bootstrap`)

Sourcing order: `lib/` -> `util/` -> OS-specific (`macos/` or `linux/`) -> `shared/`.
Snippet dirs live under `~/.config/yadm/snippets/`. Files are `NN-name.sh`, sorted and sourced in order.

Key macOS snippets: homebrew install, brewfile apply, mas (App Store), directory creation, quartz filters, tilde-switch.
Shared snippets: pbin setup, tpm (tmux plugin manager), rustup, benefactor `gh` profile seeding.
Util snippets: print helpers, copy helpers, symlink helpers.
Lib snippets: PATH invariants (`bootstrap::brew_shellenv`, `bootstrap::path_prepend`). Sourced
first, so they define functions and nothing else — `print_*` does not exist yet. This is where a
snippet's *postcondition* lives when a guard might skip the work that would otherwise establish
it: `01-homebrew.sh` once evaluated `brew shellenv` only on the branch that installed Homebrew,
so every later snippet on a machine that already had it ran against the inherited `$PATH`.

### Brewfiles (`~/.config/brew/`)

- `Brewfile` - base (always applied during bootstrap)
- `Brewfile@<scope>` - optional scopes applied interactively via `bbs`
- Some scoped files are tracked publicly, others are encrypted (see `~/.config/yadm/encrypt`)
- `undeclared` - request-installed packages deliberately declared nowhere, one `<name>  # reason` per line; read by `assay`'s `brew-health` station
- `Brewfile*.lock.json` - **deliberately not tracked** (never `yadm add`-ed; yadm's whitelist keeps them out by default — no gitignore needed). Homebrew is rolling-release, so pinned bottle SHAs expire and aren't reinstallable, while the lock churns on every `brew upgrade`. The Brewfiles are the source of truth (track-latest intent); locks are regenerated locally by `brew bundle`/`bbs`.

**When a formula disappears upstream, look for a cask before dropping the tool.** homebrew-core carries only OSI-licensed software, so a relicensing upstream gets the formula deprecated and then deleted, and homebrew-cask picks up the vendor's prebuilt binary. `tap_migrations.json` in homebrew-core records the redirect. `sentry-cli` is the worked example: it relicensed to FSL-1.1-MIT at 2.58.3, so both benefactor Brewfiles declare `cask "sentry-cli"`. A cask entry keeps automatic updates — `up`'s `brew upgrade --cask --force` pass covers it — so this stays inside Homebrew rather than falling back to the npm or cargo manifest lanes. Casks ship only the binary, so any shell completions the formula used to install must be regenerated into the tracked completion dirs (`zsh/completion/`, `fish/completions/`).

`assay`'s `brew-health` station guards the whole class: it fails on an entry that stopped resolving and warns on one that is deprecated, so this is caught in `yadm doctor` rather than on the next machine's bootstrap.

### Non-brew package manifests

Two lanes parallel to the Brewfiles, same shape — a committed manifest, restored at bootstrap,
refreshed by `up`. One entry per line; `#` comments and blanks ignored:

- `~/.config/cargo/crates.txt` → `yadm/snippets/shared/13-cargo-bins.sh`; `up` runs `cargo install-update -a`
- `~/.config/npm/globals.txt` → `yadm/snippets/shared/14-npm-globals.sh`; `up` runs a blanket `npm update -g`

Bootstrap is the only thing that *installs* from these manifests — `up` merely upgrades what is
already present. So a package added to a manifest on a running machine must also be installed by
hand once; the manifest is what makes it reproducible on the *next* machine.

Reach for a manifest lane only when Homebrew has no formula. `node` itself is a brew formula, and
because brew's node sets npm's global prefix to `/opt/homebrew`, npm globals land in
`/opt/homebrew/lib/node_modules/` with a shim in `/opt/homebrew/bin/` — they look brew-installed
but are not, and `brew list` will not show them.

### Claude Code lanes (`~/.claude/`)

`~/.claude/skills/` is not a skills folder — it is a **plugin auto-load root**. Claude Code adopts
every non-dot entry under it (directory or symlink) as a local plugin named after the directory
(`<name>@skills-dir`), with no install step and no `enabledPlugins` entry. A plugin root may carry
`.claude-plugin/plugin.json` plus any of `commands/`, `agents/`, `skills/`, `output-styles/`,
`workflows/`, `routines/`, `hooks/`, `.mcp.json`, `.lsp.json`. The familiar one-`SKILL.md` directory
is just the degenerate single-skill plugin.

That gives the tree two lanes, and **position decides the lane** — never a per-file judgment:

- **Top level = public**, plaintext-tracked one directory at a time (`docket`, `php-lsp`,
  `transplant-worktrees`).
- **`~/.claude/skills/attic/` = private**, swept whole by the single `.claude/skills/attic/**`
  pattern in `yadm/encrypt`. It is a plugin in its own right, so *every* private surface fits inside
  it — a skill under `attic/skills/<name>/`, a mode under `attic/commands/modes/<name>.md`, and
  later an agent, an output-style, an MCP or LSP server — and each arrives already covered, with no
  new pattern to remember. `attic` means here what it means at `~/.config/attic/`: the private lane.

Naming follows from the plugin shape: a private skill is addressed `attic:<name>`, a private command
`/attic:<dir>:<name>`. Modes are the exception by design — `~/.claude/hooks/modes.py` searches every
skills-dir plugin's `commands/modes/`, so `+<name>` is identical whichever lane the file sits in.

Enforcement runs both ways and is already wired into `yadm doctor`. A private file left in the
public lane trips the `yadm-coverage` station's R4 (the `pre_commit` identity guard, run backwards) and
fails. A public file that lands inside `attic/` is merely over-protected — and if it is also
`yadm add`-ed, R1 (a path in both lanes) fails. Nothing is tracked publicly *and* archived. The
public lane still needs one `yadm add` per file, because that is what plaintext tracking is; R5
nags until it happens, a whole new skill directory included.

### PHP language server

The first tool admitted under `~/.config/reliquary/AGENTIC-TOOLING.md` — the bar and registration
protocol for third-party tools exposed machine-wide for agents to reach for.

Intelephense (npm global, premium tier) is registered for **every** project as a Claude Code LSP
plugin at `~/.claude/skills/php-lsp/` — a directory under `~/.claude/skills/` auto-loads as a
user-level plugin (`php-lsp@skills-dir`), so no install command and no `enabledPlugins` entry are
needed. The same directory carries a `SKILL.md`, because the `LSP` tool is *deferred* and the
server starts lazily: without something announcing it, agents do not go looking and the server
never launches at all. Both files are deliberately free of secrets and of `$HOME`-absolute paths,
so they stay publicly trackable and pass the `pre_commit` identity guard.

That is possible because the manifest omits `licenceKey` and `storagePath` entirely and lets
intelephense use its own defaults, which resolve under `$XDG_CONFIG_HOME` (unset here, so
`~/.config/intelephense/`): `global/` for the licence, `workspace/` for the per-project index. The
index is a regenerable cache — cold build is seconds, and yadm's whitelist keeps it untracked with
no gitignore needed. `global/` also accumulates a machine-specific activation cache whose
*filename embeds the key*, which is why that directory is `0700` and why the encrypt pattern names
a single file rather than globbing the directory.

### Other tracked configs

- `ghostty` - terminal emulator config
- `git/attributes` - `* text=auto`, binary markers for `*.png`/`*.plist`
- `vim/vimrc` - Vim configuration (Vim 9.2+ native XDG support)
- `oh-my-posh` - shell prompt theme
- `quartz-filters` - macOS PDF compression filters
- `tmux` - tmux configuration
- `zsh/completion/docker` - docker completions for zsh
- `zed` - editor settings and keymap
- `~/.claude/skills/transplant-worktrees/` - folds Claude Code worktree branches onto main (rebase + ff-merge), then reclaims their Docker state via `compose-gc`. Domain-free, so it travels; the private skills live in the `attic/` lane above

### Deliberately not tracked (audited)

**`~/.config/yadm/unmanaged` is the authoritative list** — one line per path or glob, with the reason. `assay`'s `yadm-coverage` station reads it, so a decision recorded there is a decision that stops resurfacing; anything under a scanned root that is in neither lane and not declared is reported as undecided. Add to that file first; this section carries only the entries whose reasoning needs more than a line.

The shape of the judgment: these were reviewed and **intentionally excluded** — neither plaintext-tracked nor encrypted. All are regenerated by their tool's normal auth/setup flow on a new machine, so the sync value is low and the leak/footgun risk is not worth it:

- `~/.config/raycast/config.json` - holds a live `rca_…` access token; regenerable, machine-local.
- `~/.kube/config` - embeds a live orbstack `client-key-data` and benefactor GKE cluster names; GKE contexts re-fetch via `gcloud container clusters get-credentials`, orbstack regenerates its own.
- `~/.docker/config.json` - no secrets today (`auths: {}`, osxkeychain + gcloud credHelpers), but tracking it risks a future `docker login` writing base64 creds into `auths{}`; the credHelper config regenerates on setup.
- `~/.config/gh/hosts.yml` - just SSH-protocol pref + public username; `gh auth login` regenerates it, and tracking risks `gh` writing an `oauth_token` into a plaintext file. (`~/.config/gh/config.yml` — protocol pref plus aliases, no credentials — *is* tracked.)
- `~/.config/gh-benefactor/` - the benefactor `gh` profile; same reasoning as `hosts.yml`, and the non-secret half is regenerated by `yadm/snippets/shared/15-gh-benefactor.sh`.
- `~/.gnupg/gpg-agent.conf` + `~/.gnupg/pinentry-ide.sh` - IDE-generated (PhpStorm) GPG-signing pinentry config and its shim. No secrets, but both hardcode absolute `$HOME` paths carrying the local username, which the `pre_commit` identity guard bars from the public plaintext tree; encrypting them isn't worth it since the IDE regenerates both on its next GPG run.

### Repository documentation (`~/.github/`)

- `README.md` - install/usage instructions, rendered on the GitHub repo page
- `LICENSE.md` - MIT license

These live in `~/.github/` (not `~/.config/`) because GitHub only renders READMEs from the repo root or `.github/` directory.

### Hooks

Encrypted hooks may exist (see `~/.config/yadm/encrypt`).

## yadm aliases (from `interactive.d/100-aliases-yadm.sh`)

```
ya='yadm add'          yf='yadm fetch ...'    yrs='yadm restore --staged'
ys='yadm status'       yc='yadm commit'       yp='yadm push'
ypt='yadm push --tags' yff='yadm merge --ff-only @{u}'
ypff='yadm pull --ff-only'
ylf / ywlf = yadm log (80/140 char wide)
```

## Workflow

**Adding/updating dotfiles:**
1. Edit the file
2. `ya <file>` (or `yadm add <file>`)
3. `ys` to verify
4. `yc` to commit
5. `yp` to push

**New machine setup:** see `~/.github/README.md` - curl yadm, clone, bootstrap, then decrypt + bootstrap again.

**Updating:** `yadm update` (pull + encrypted-files check) or `ypff`.
