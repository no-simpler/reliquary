pub const TOPICS: &[(&str, &str)] = &[
    ("ladder", LADDER),
    ("metadata", METADATA),
    ("keys", KEYS),
    ("agent", AGENT),
];

pub const LADDER: &str = "\
THE LADDER

    handoff ──▶ relay ──▶ spec:design ──▶ spec:implementation
       └────────────────▶

handoff   One session writes it, one session reads it, and it closes. Nothing is
          owed afterwards. Reach for this when the work is a single pickup, even
          a large one.

relay     Read, acted on, and then succeeded: the session that consumes a relay
          owes the next one. Reach for this when the next step genuinely cannot
          be planned until this one lands. `docket relay <id>` mints the
          successor and archives the predecessor in a single step, carrying the
          chain id forward and incrementing the hop.

spec      A multi-session initiative that designs first and implements second,
          tracked in one file with a checklist. Stage `design` iterates on the
          plan; stage `implementation` works it. A spec is a directory, so
          anything the initiative needs can sit beside it.

Promotion runs forward only, and never rewrites what an item already carries —
each rung's fields are a superset of the rung below, so nothing is lost on the
way up. A handoff may skip the relay rung with `--to spec` when it turns out to
be a whole initiative rather than one more step.

There is no demotion. An item that was promoted in error is closed, and the
replacement is opened at the rung that fits.";

pub const METADATA: &str = "\
FRONTMATTER

Every item is one Markdown file whose frontmatter the CLI owns and whose body
you write. Keys appear in this order, and every rung adds to the one below it.

Every item:

  id            four characters, unique across every project on this machine
  kind          handoff, relay or spec
  title         the item's name, one line, at most 72 characters
  tagline       one line under the title, at most 80 characters
  project       absolute path this item belongs to
  created       RFC 3339, UTC
  updated       RFC 3339, UTC
  order         sparse; `docket reorder` owns it
  blocked       optional, one line at most 80: present means blocked, and why
  origin        optional: the project this was written from, when they differ
  tags          optional, single tokens of at most 32 characters

relay adds:

  chain         stable across every hop of one chain
  hop           1 for the first
  supersedes    the item this one was minted from, absent on the first hop

spec adds:

  stage         design or implementation

Every one-line field is normalised on the way in: trimmed, and any run of
whitespace collapsed to a single space. The limits bind what is written, never
what is read — an item already on disk with an over-long field still lists and
still opens, and `docket doctor` reports it with the command that fixes it.

An item whose frontmatter stops parsing is never hidden. It stays in every
listing marked INVALID with the line and column at fault, `docket doctor`
reports it, and `docket set` rewrites the whole block in canonical order, which
repairs it.";

pub const KEYS: &str = "\
PROJECTS

A docket belongs to a directory, and that directory is resolved before anything
else happens:

  in a git repository   the main checkout root — so every subdirectory, and
                        every linked worktree, shares one docket
  in a submodule        the submodule's own root
  anywhere else         the working directory itself

Paths are resolved through symlinks. A path that does not exist yet resolves as
far as it can and keeps the rest, so an item written for a project you are about
to create keys to the same docket once it exists:

  docket create handoff --to ~/Developer/new-thing --allow-missing \\
      --title '...' --tagline '...'

Items live under ~/.claude/docket, one directory per project, beside the
transcript directory Claude Code keeps for the same path. Set DOCKET_ROOT to
point somewhere else — that is how tests and trial runs stay clear of the real
one.

Nothing here is version controlled, and nothing here belongs in a repository.
An item is a note between sessions, not a record.";

pub const AGENT: &str = "\
FOR AGENTS

Writing an item:

  1. `docket create <kind> --title '...' --tagline '...'` prints an id and a
     path.
  2. Write the body at that path with ordinary file tools. It is Markdown, and
     the frontmatter above it belongs to the CLI — leave it alone.
  3. Say the id back to the user. Four characters, selectable with one
     double-click, usable from any directory.

Reading one: `docket show <id>` prints the body. `docket path <id>` prints the
file, for editing in place.

Finishing one: `docket close <id>` archives it. A relay is finished with
`docket relay <id>`, which opens the successor and archives the predecessor
together.

Output modes: docket prints agent-shaped output — unaligned, uncoloured, stable
field order — whenever CLAUDECODE is set or stdout is not a terminal. A person
at a terminal gets an aligned, coloured table instead. Force either with
--format, and use --json when parsing.

A good body says what a session arriving cold needs: what the work is, what was
already settled and must not be re-derived, what to do first, and how to know it
worked. What is obvious now is gone in a week.

The title and the tagline are not a summary of that body. They are the two lines
a reader skims to decide whether to open it at all — 72 and 80 characters, hard
limits — so write them as a name and a single claim, and put everything else
below the frontmatter.";

pub fn topic(name: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(key, _)| *key == name)
        .map(|(_, body)| *body)
}

pub fn topic_names() -> String {
    TOPICS
        .iter()
        .map(|(key, _)| *key)
        .collect::<Vec<_>>()
        .join(", ")
}
