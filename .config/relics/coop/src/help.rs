//! Reference: the contracts, the files, the exit codes.

/// Every reference topic, in the order a reader meets them.
pub const TOPICS: &[(&str, &str)] = &[("wiring", WIRING), ("tiers", TIERS)];

const WIRING: &str = "\
wiring

  Every interactive shell runs coop tick before every prompt, then reads the
  badge file with a shell builtin, so one exec serves both surfaces.

  The hook is registered in shell/interactive.d/050-prompt.zsh, its bash twin,
  and fish/conf.d/050-prompt.fish, beside the ske one and before oh-my-posh is
  initialised. That order matters in zsh and bash: oh-my-posh reads the exit
  status at the top of its own hook, so a hook registered after it would set
  the badge one prompt late.

  Every hook saves and restores the exit status. A precmd function that returns
  its own status makes the shell forget what the last command did, which shows
  up as a prompt that is always the colour of success.

  The badge file holds two fields, the count and the digest of the outstanding
  set, or nothing. The hook exports them as COOP_BADGE and COOP_SEEN. The
  prompt segment is a text segment over .Env.COOP_BADGE in
  oh-my-posh/dreamsofautonomy.toml; the next tick draws the card only when
  COOP_SEEN differs from the digest it computes. A shell keeps that in its own
  environment, for exactly as long as it lives.

files

  ~/.config/coop/config.toml        style and width
  ~/.config/coop/sources.d/*.toml   one declaration per file, tracked
  ~/.local/state/coop/badge         count and digest, for the hook
  ~/.local/state/coop/first-seen.json  when each notice arrived, for doctor

environment

  COOP_ROOT      the state tree
  COOP_CONFIG    the config tree
  COOP_UI        the default output shape
  COOP_SEEN      the digest this shell was last shown, exported by the hook
  COOP_DISABLE   set to anything and the tick draws nothing

exit codes

  0  what was asked for
  1  doctor found something soft
  2  doctor found something broken
";

const TIERS: &str = "\
tiers

  coop holds no answer of its own. Every source is evaluated afresh before
  every prompt, and a declaration picks one of three tiers by how expensive
  its answer is.

  when   coop stats a path. The producer writes a stamp when it runs.
  ask    coop runs the producer, every prompt, killed at 50ms. The producer
         answers read-only from its own state.
  read   coop reads a report file. The producer rewrites it on its own
         cadence: launchd, up, a hook, its own last run.

  A producer that cannot answer within the budget belongs in read or when,
  never in ask; doctor says so when one is over. Whatever caching a producer
  needs is the producer's, because only it knows when its truth changes.

  A producer does not keep state for coop's benefit, does not retract
  anything, and does not colour its output. Something it gets wrong lands in
  doctor, never on the card.

  when — stat and arithmetic. A path that has not been touched in a span, a
  path that exists, an instant that has passed, and all-of, any-of and not
  over those. A when source states its own summary and fix, because a
  predicate has no other way to say anything; summary-missing is for a path
  that is not there at all, when that reads differently from stale.

    [source]
    id = \"up\"
    summary = \"{age} since the last system update\"
    summary-missing = \"no record of a system update\"
    fix = \"up\"

    [source.when]
    path = \"~/.local/state/up/last_upped_at\"
    older-than = \"1d\"
    missing = \"fire\"

  ask — a command. kind is findings for anything answering doctor --format
  json, and text for anything else, whose non-blank lines become notices
  verbatim. A findings producer reports its grade through its exit status, so
  a non-zero status is the answer; a text producer has no such channel, so
  there a non-zero status is a failure.

    [source]
    id = \"rote\"

    [source.ask]
    run = [\"rote\", \"banner\", \"--format\", \"json\"]
    kind = \"findings\"

  read — a file, in the same two kinds. expect-every is what the producer
  promises; past it, doctor reports the producer has stopped, and the card
  keeps showing what is there.

    [source]
    id = \"slow\"

    [source.read]
    path = \"~/.local/state/slow/report.json\"
    kind = \"findings\"
    expect-every = \"1d\"

  A source whose program or report is not on this machine is dormant.
  Declarations are tracked and travel; producers do not.
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
