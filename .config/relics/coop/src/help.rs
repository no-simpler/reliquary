//! Reference: the contracts, the files, the exit codes.

/// Every reference topic, in the order a reader meets them.
pub const TOPICS: &[(&str, &str)] = &[("wiring", WIRING), ("tiers", TIERS), ("keys", KEYS)];

const WIRING: &str = "\
wiring

  Every interactive shell runs coop tick before every prompt, and reads the
  badge file with a shell builtin so one exec serves both surfaces.

  The hook is registered in shell/interactive.d/050-prompt.zsh, its bash twin,
  and fish/conf.d/050-prompt.fish, beside the ske one and before oh-my-posh is
  initialised. That order matters in zsh and bash: oh-my-posh reads the exit
  status at the top of its own hook, so a hook registered after it would set
  the badge one prompt late.

  Every hook saves and restores the exit status. A precmd function that returns
  its own status makes the shell forget what the last command did, which shows
  up as a prompt that is always the colour of success.

  The prompt segment is a text segment over .Env.COOP_BADGE in
  oh-my-posh/dreamsofautonomy.toml. The command segment type was removed
  upstream in v29 and renders nothing without erroring, so an environment
  variable is the supported route.

files

  ~/.config/coop/config.toml        style and width
  ~/.config/coop/sources.d/*.toml   one declaration per file, tracked
  ~/.local/state/coop/cache/        what each asked source last said
  ~/.local/state/coop/seen/         what each shell has been shown
  ~/.local/state/coop/badge         the prompt segment

environment

  COOP_ROOT      the state tree
  COOP_CONFIG    the config tree
  COOP_UI        the default output shape
  COOP_SESSION   this shell's identity, exported once by the hook
  COOP_DISABLE   set to anything and the tick draws nothing

exit codes

  0  what was asked for
  1  doctor found something soft, or a refresh failed
  2  doctor found something broken
";

const TIERS: &str = "\
tiers

  A declaration is either a condition coop evaluates itself, or a command coop
  runs for an answer. Nothing else.

  when — stat and arithmetic, inline, before every prompt. No subprocess, so
  this is the tier to reach for. A path that has not been touched in a day, a
  path that exists, an instant that has passed, and all-of, any-of and not over
  those.

    [source]
    id = \"up\"
    summary = \"{age} since the last system update\"
    summary-missing = \"no record of a system update\"
    fix = \"up\"

    [source.when]
    path = \"~/.local/state/up/last_upped_at\"
    older-than = \"1d\"
    missing = \"fire\"

  ask — a command, memoized. Its answer is cached and reused while a refresh
  runs behind the prompt, so a producer is never on the path a prompt waits on.
  kind is findings for anything answering doctor --format json, and text for
  anything else, whose non-blank lines become notices verbatim.

    [source]
    id = \"rote\"

    [source.ask]
    run = [\"rote\", \"banner\", \"--format\", \"json\"]
    kind = \"findings\"
    refresh = \"10m\"
    keys = [\"~/.local/state/rote/cache.json\", \"day:04:00\"]

  A when source states its own summary and fix, because a predicate has no
  other way to say anything. summary-missing is for the case where a path that
  is not there at all reads differently from one that is merely stale. An ask source takes both from its producer.

  A source whose program is not on this machine goes dormant. Declarations are
  tracked and travel; producers do not.
";

const KEYS: &str = "\
keys

  refresh is a guess about how fast the world moves. keys is an answer.

  Each entry is either a path, whose modification time and size are read, or a
  day, whose calendar date is read under a rollover that need not be midnight.
  An answer is reused until the clock passes refresh or any key moves,
  whichever comes first.

    keys = [\"~/.local/state/rote/cache.json\", \"day:04:00\"]

  That pair says: ask again when the file rote derives its count from changes,
  and ask again when the drill day turns over at four in the morning. Between
  those, the answer cannot have changed, so nothing runs.

  A path that is absent fingerprints as absent, which is itself a fact that can
  change. A declaration with no keys falls back to the clock alone.
";

/// One topic, by name.
pub fn topic(name: &str) -> Option<&'static str> {
    TOPICS
        .iter()
        .find(|(topic, _)| *topic == name)
        .map(|(_, body)| *body)
}

/// Every topic name.
pub fn topic_names() -> Vec<&'static str> {
    TOPICS.iter().map(|(name, _)| *name).collect()
}

/// The asked topics, in canonical order, or all of them.
pub fn render(asked: &[String]) -> String {
    let wanted: Vec<&'static str> = if asked.is_empty() {
        topic_names()
    } else {
        TOPICS
            .iter()
            .filter(|(name, _)| asked.iter().any(|topic| topic == name))
            .map(|(name, _)| *name)
            .collect()
    };
    if wanted.is_empty() {
        return format!(
            "coop: no such topic. Try one of: {}\n",
            topic_names().join(", ")
        );
    }
    let mut out = String::new();
    for name in wanted {
        if let Some(body) = topic(name) {
            out.push_str(body);
            out.push('\n');
        }
    }
    out
}
