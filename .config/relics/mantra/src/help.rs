pub const TOPICS: &[(&str, &str)] = &[
    ("triggers", TRIGGERS),
    ("metadata", METADATA),
    ("hooks", HOOKS),
];

pub const TRIGGERS: &str = "\
TRIGGERS

A list, not an enum: combinations are list membership.

  - on: activate                      said when +token switches it on
  - every: { tokens: 25000 }          said again every so many context tokens
  - when: { context_tokens: 500000 }  said once, on crossing the mark

No triggers key means [ on: activate ] — what an expander did.

Unknown keys are refused rather than ignored, so a typo is a doctor
finding and never a mode that quietly stops being said.

Compaction is not a trigger. Every active mode is restated in full after
one, because the context that held it is gone.";

pub const METADATA: &str = "\
METADATA

Optional. A mode file with none is a mode with the default schedule.

  triggers   list; see mantra help triggers
  refrain    one line, said instead of the body on a refresh

A mode's first delivery is always its body, whichever clause calls for
it. The refrain is for re-delivery.

Absent refrain falls back to the body, which is right for a mode with no
exceptions to leave out of the repeat loop.";

pub const HOOKS: &str = "\
HOOKS

One command answers every boundary; the payload names its own event.

  SessionStart startup|clear   state for the id is forgotten
  SessionStart resume|fork     state rebuilt from the transcript, nothing said
  SessionStart compact         every active mode restated in full
  UserPromptSubmit             activation, and refreshes due at a turn
  PostToolBatch                refreshes due inside a turn

A payload carrying agent_id is a subagent and is ignored.

Wiring lives in ~/.claude/settings.json. mantra doctor checks it.";

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
