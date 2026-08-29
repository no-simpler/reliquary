# Agentic tooling — the admission bar

Reliquary is the machine-wide owner of tools that outlive any single repository. This document
governs one narrow class of them: third-party tools put on `$PATH` (or wired into the harness)
**for agents to reach for**, across every project on the machine.

It is a judging aid. `GRADUATION.md` covers relics the author writes; `BEDROCK.md` covers the
guaranteed substrate. This covers what gets exposed to an agent and why.

## The posture

Expose, do not prescribe. The point is to widen the set of small iteration loops an agent *can*
reach for when a task calls for one — never to mandate a tool, and never to route work through it
by default. A tool that only pays off when something forces its use has not earned admission.

## The bar

A candidate clears **all five** or it is not admitted.

1. **Otherwise unreachable.** Not already a dependency of the project's own image. A dockerized
   project keeps its container up during normal work, and a round-trip into a running container
   costs on the order of 100 ms — so for anything the project already ships, PATH availability buys
   back the exec hop and nothing else. *This is the gate most candidates fail.*
2. **Project-agnostic.** No per-project config, no package-manager autoloader, no compiled
   container, no generated artifact. It reads source and nothing else. A tool that needs the
   project's own build state is a project tool wearing a global costume.
3. **Host-shaped.** It must genuinely want to be a host process — persistent and stateful (a
   language server), a harness integration point (a plugin, an MCP server, a hook), or needed while
   the container is down. A one-shot batch tool is not host-shaped; `exec` expresses it fine.
4. **Reliquary-ownable.** Restorable on a fresh machine from a tracked manifest, with nothing
   secret and no absolute home path in any trackable file.
5. **Carries no new runtime.** Per `BEDROCK.md`: when a tool needs more than bedrock, it dockerizes
   rather than growing bedrock. A candidate that drags a language runtime onto the host must
   justify that runtime on its own terms, not on the tool's.

## Announce, do not prescribe

Every admitted tool gets a skill under `~/.claude/skills/<name>/`, and that skill's entire job is
**discoverability**. Harness tools such as `LSP` are *deferred*: their schemas do not load unless
the model goes looking, so an unannounced tool is not merely underused — it never starts at all.
That was measured: a silently-installed language server saw **0/10 adoption**, never launching once,
including in a run that made 31 tool calls. A single line naming it flipped that reliably.

So the rule is: **one line stating the tool exists; never a directive to use it.**
A relic the author writes is held to the same rule and one more — it hosts its own doctrine in a
`guide` namespace in the binary, so the skill stays a stub. See `GRADUATION.md`.
`~/.claude/skills/php-lsp/SKILL.md` is the reference length — frontmatter `description` plus one
sentence. Resist the urge to explain when to reach for it; that is the agent's call.

## Agent-aware output

A tool agents invoke should detect that and change shape. Claude Code exports **`CLAUDECODE`** into
every subprocess it spawns, which is the signal — a tool run through the `Bash` tool sees it, and a
tool run by a person at a terminal does not. Non-TTY stdout is the backstop for agentic callers that
set nothing.

The resolution order, in the order a caller expects to win: an explicit flag, then the tool's own
`<NAME>_UI` environment variable, then `CLAUDECODE`, then whether stdout is a terminal.

Agent shape means no colour, no alignment padding, no box drawing, and a stable field order.
Alignment is paid for twice — once written, once read — and buys a model nothing. Human shape may
spend freely on tables and colour. Keep `--json` a separate, explicit opt-in: it is for scripts, and
it costs more tokens than a terse line format.

`docket` is the reference implementation.

## Registration protocol

The language-server admission is the reference implementation. Six steps:

1. **Manifest entry**, so a fresh machine restores it — `~/.config/brew/Brewfile*`,
   `~/.config/npm/globals.txt`, `~/.config/cargo/crates.txt`, or a relic under `~/.config/relics/`.
   Reach for a non-brew lane only when no formula exists. Note that **bootstrap is the only thing
   that installs** from the non-brew manifests; `up` merely upgrades what is already present, so a
   package added on a running machine must also be installed by hand once.
2. **A `SKILL.md`, in the lane the tool belongs to** — terse, per the rule above. `~/.claude/skills/`
   is a plugin auto-load root: a directory under it becomes a user-level plugin
   (`<name>@skills-dir`) with no install command and no `enabledPlugins` entry. Public tools go at
   the top level, `~/.claude/skills/<name>/SKILL.md`. A tool whose *existence* is sensitive goes to
   the private lane instead, `~/.claude/skills/attic/skills/<name>/SKILL.md`, where the encrypt
   pattern already covers it — see "Claude Code lanes" in `~/.config/CLAUDE.md`. Decide this on
   first landing; a file in neither lane is the failure mode the lanes exist to prevent.
3. **`.claude-plugin/plugin.json`** — only when the harness needs wiring (a language server, a
   hook), and only for a public top-level tool. The private lane is one plugin already, so a tool
   inside it declares nothing of its own.
4. **An ownership entry in `~/.config/CLAUDE.md`**, and **never** one in a project's `CLAUDE.md`.
   Reliquary records what it owns; projects stay unaware. That is also what keeps a global tool out
   of repositories whose public edge would leak it.
5. **Put the paths in a lane.** Public: `yadm add` every new path explicitly — yadm is
   whitelist-based, a new file is untracked until named, and there is no usable blanket add; verify
   with `yadm ls-files <path>`. Private: nothing to add, but run `yadm encrypt` so the archive
   catches up.
6. **Grant permissions in the form the harness actually matches.** A tool that needs the agent to
   reach a path unprompted gets a rule in `~/.claude/settings.json`, and **a path grant is
   `Edit(<glob>)` or `Read(<glob>)` only.** File permission checks consult exactly one gate per
   operation: `Edit(path)` for *every* file-writing tool, `Read(path)` for *every* file-reading
   tool. `Write(…)`, `MultiEdit(…)`, `NotebookEdit(…)` and `Glob(…)` are accepted and never match
   — they warn once, at session start, to the human, in whichever project happens to load the file
   — and **`Grep(…)` is dead and never warns at all**. A tool invocation is a separate grant,
   `Bash(<name>:*)`; never express a path through it.

## Ledger

One entry per judged candidate, so the next session does not re-derive the verdict.

### Intelephense (PHP language server) — admitted

Clears all five: absent from project images, indexes source with no project state, must be held
open as a stdio server by the harness, restorable from `npm/globals.txt`, and adds no runtime
(Node is already present). Registration is the reference implementation — manifest entry plus
`~/.claude/skills/php-lsp/` carrying both `SKILL.md` and `plugin.json`.

Note the deliberate restraint in its manifest: it omits `licenceKey` and `storagePath` so the tool
resolves its own defaults, which is what lets both files stay publicly trackable.

### PHPStan — rejected for now

Fails gate 1 outright, and gates 2 and 5 besides.

- **Gate 1.** A PHP project that runs static analysis already ships it. Measured against a running
  container: `docker compose exec -T app php -v` round-trips in **0.09–0.13 s**. That is the entire
  prize.
- **Gate 2.** It needs the project's `vendor/` autoloader, its own config, and — with the framework
  extension — a compiled container XML that only the container generates. Its result cache is
  repo-relative and bind-mounted, so host and container runs see different absolute paths for the
  same files and invalidate each other on every alternation.
- **Gate 5.** It would put a full PHP runtime on the host to save the exec hop.

There is also direct evidence against the loop it would serve: a benchmark of static analysis on
every edit measured **106% wall clock and 104% cost** versus baseline, and placed 3rd of 5 on the
one task seeded specifically to favour it. Earlier feedback did not repay the per-edit round trip.

**This is a verdict on present evidence, not a closed door.** Circumstances move, and any of these
would reopen it:

- projects stop keeping a container up during normal work, or exec latency stops being negligible;
- PHPStan gains a genuinely project-agnostic mode needing neither installed dependencies nor a
  compiled container;
- a host PHP earns its place for an unrelated reason, so the runtime cost is already sunk;
- the benchmark is re-run under *interactive* rather than headless conditions — the one follow-up
  its own report recommends — and measures a real win.

### caveman — package rejected, ruleset adopted

Not a tool at all. `JuliusBrussee/caveman` is a prose ruleset wrapped in a `curl | bash` installer,
two Node hooks, a statusline, slash commands, subagents, an MCP server, and classical-Chinese
flavors. The ruleset is genuinely good — it forbids invented abbreviations and arrows because they
measure zero saving under the tokenizer, which is the mark of an honest one. The packaging exists
for a single reason: its `SessionStart` hook force-injects the rules so the model cannot drift back
to verbose. That enforcement is machine-wide and always-on, which is exactly the posture this
document forbids.

Its own `docs/HONEST-NUMBERS.md` argues the rest: output falls 65% on average, input falls 0%, the
skill *adds* ~1–1.5k input tokens, and whole-session savings land at 14–21% on output-heavy work and
go negative on terse work. Paying that on every repo to buy a register that fits in thirty lines is
a bad trade.

So the ruleset was distilled into the `terse` mode and reaches sessions through the `+token` mode
framework, which is opt-in per session and costs nothing when unused — a mode is inert until a
`+token` names it. Nothing from the upstream repository is installed.

Two harness facts that constrain any future register work, both verified against the current
reference: an output style reaches the main loop only — a subagent runs its own system prompt, and
only a `fork` inherits the parent's. `CLAUDE.md`, by contrast, loads into every non-fork subagent,
with the built-in `Explore` and `Plan` agents the sole exceptions and no setting to change that. A
register that must reach delegated work therefore belongs in `CLAUDE.md`, not in a style or a mode.
