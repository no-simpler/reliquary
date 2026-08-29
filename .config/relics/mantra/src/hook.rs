//! What each hook boundary carries, and the rule that a mode file never names
//! one.
//!
//! A mode declares **intent** — "every twenty-five thousand tokens". The runtime
//! owns **mechanism** — which of Claude Code's hook events actually carries that.
//! Keeping hook names out of mode files is what stops the corpus from rotting
//! when the harness changes its event surface.
//!
//! | Boundary | Carries |
//! |---|---|
//! | `SessionStart` `startup`/`clear` | nothing; the context is empty, so state for this id goes |
//! | `SessionStart` `resume`/`fork` | a rebuild, and no injection: the restored context still holds what it held |
//! | `SessionStart` `compact` | every active mode, in full |
//! | `UserPromptSubmit` | activation, and refreshes due at a turn boundary |
//! | `PostToolBatch` | refreshes due inside a turn |
//!
//! Intra-turn refresh is the reason `PostToolBatch` is wired at all. Salience
//! decays with what fills the window, and a single agentic turn can fill a
//! hundred thousand tokens of it without ever reaching a prompt submission —
//! which is precisely the session in which a directive from turn one has gone
//! quietest.
//!
//! **A payload carrying `agent_id` is a subagent**, whose window is its own and
//! whose modes are not this session's. Every path checks it first.
//!
//! **Nothing here may fail loudly.** `UserPromptSubmit` promotes any exit-zero
//! stdout straight into the model's context, so an error path that prints is an
//! error path that speaks to the model. Every fallible step here answers `None`.

use camino::{Utf8Path, Utf8PathBuf};
use serde::Deserialize;

use crate::inject::{self, Block, Occasion};
use crate::mode::{self, Mode};
use crate::resolve;
use crate::schedule::{self, Fire};
use crate::state::{self, Active, Session};
use crate::token;
use crate::transcript;

/// The events this answers to. `PostToolUse` is accepted beside `PostToolBatch`
/// so the wiring can move between them without a rebuild — mechanism is the
/// runtime's to choose.
const HANDLED: &[&str] = &[
    "SessionStart",
    "UserPromptSubmit",
    "PostToolBatch",
    "PostToolUse",
];

/// The fields of a hook payload this reads. Unknown keys are ignored on purpose:
/// the harness adds fields between versions, and a strict read would turn an
/// upgrade into a silently dead hook.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct Payload {
    /// Which event fired.
    pub hook_event_name: String,
    /// The session this belongs to, and the state key.
    pub session_id: String,
    /// Where the conversation is recorded.
    pub transcript_path: String,
    /// The session's working directory.
    pub cwd: String,
    /// Present only inside a subagent.
    pub agent_id: Option<String>,
    /// `SessionStart`: `startup`, `resume`, `clear`, `compact` or `fork`.
    pub source: String,
    /// `UserPromptSubmit`: what the user sent.
    pub prompt: String,
    /// `SessionStart` volunteers this on a resumed or forked session.
    pub context_tokens: Option<u64>,
}

/// Where this machine keeps modes and state.
pub struct Env {
    /// The home directory, whose `.claude` tree is searched first.
    pub home: Utf8PathBuf,
    /// The state directory.
    pub root: Utf8PathBuf,
}

/// What to hand back to the harness.
pub struct Injection {
    /// The event to echo, which the envelope must match.
    pub event: String,
    /// The text.
    pub context: String,
}

/// Reads a hook payload and answers with what the model should be told, if
/// anything. Every failure is `None`.
pub fn run(input: &str, env: &Env) -> Option<Injection> {
    let payload: Payload = serde_json::from_str(input).ok()?;
    dispatch(&payload, env)
}

fn dispatch(payload: &Payload, env: &Env) -> Option<Injection> {
    if payload.agent_id.is_some() {
        return None;
    }
    if !HANDLED.contains(&payload.hook_event_name.as_str()) {
        return None;
    }
    if !state::is_id(&payload.session_id) {
        return None;
    }
    let context = match payload.hook_event_name.as_str() {
        "SessionStart" => session_start(payload, env),
        "UserPromptSubmit" => prompt(payload, env),
        _ => refresh(payload, env),
    }?;
    Some(Injection {
        event: payload.hook_event_name.clone(),
        context,
    })
}

/// A session beginning, resuming, forking, clearing, or coming out of a
/// compaction.
fn session_start(payload: &Payload, env: &Env) -> Option<String> {
    match payload.source.as_str() {
        // The context is empty. Anything this id remembers was spoken into a
        // conversation that no longer exists.
        "startup" | "clear" => {
            state::remove(&env.root, &payload.session_id);
            None
        }
        // The originals were summarised away, so every active mode says itself
        // again, in full.
        "compact" => {
            let mut session = recover(payload, env);
            if session.modes.is_empty() {
                return None;
            }
            let tokens = tokens(payload, session.tokens);
            let modes = load(&session, payload, env);
            session.generation = session.generation.saturating_add(1);
            let fires = schedule::advance(&mut session, &modes, tokens, &[], true);
            let _ = state::save(&env.root, &payload.session_id, &session);
            inject::render(&[(Occasion::Restate, blocks(&fires, &modes))])
        }
        // The transcript comes back with whatever it held, so there is nothing
        // to re-say. What is missing is the state beside it — a fork gets a new
        // session id and therefore none at all.
        "resume" | "fork" => {
            let session = recover(payload, env);
            if !session.modes.is_empty() {
                let _ = state::save(&env.root, &payload.session_id, &session);
            }
            None
        }
        _ => None,
    }
}

/// A turn boundary: the one place a `+token` can switch a mode on.
fn prompt(payload: &Payload, env: &Env) -> Option<String> {
    let mut session = state::load(&env.root, &payload.session_id).unwrap_or_default();
    let roots = roots(payload, env);

    let mut activated = Vec::new();
    for name in token::activations(&payload.prompt) {
        if session.holds(&name) {
            continue;
        }
        if resolve::find(&name, &roots).is_none() {
            continue;
        }
        activated.push(name);
    }
    if session.modes.is_empty() && activated.is_empty() {
        return None;
    }

    let tokens = tokens(payload, session.tokens);
    for name in &activated {
        session.modes.push(Active::new(name.clone(), tokens));
    }
    session.turns = session.turns.saturating_add(1);

    let modes = load_from(&session, &roots);
    let fires = schedule::advance(&mut session, &modes, tokens, &activated, false);
    let _ = state::save(&env.root, &payload.session_id, &session);
    render(&fires, &modes)
}

/// Inside a turn. Nothing can be switched on here, so a session with no state
/// costs one failed open and nothing else.
fn refresh(payload: &Payload, env: &Env) -> Option<String> {
    let mut session = state::load(&env.root, &payload.session_id)?;
    if session.modes.is_empty() {
        return None;
    }
    let roots = roots(payload, env);
    let modes = load_from(&session, &roots);
    // Reading the window means reading the transcript, so a session whose modes
    // are all activation-only never touches it.
    if !modes.iter().any(Mode::reads_tokens) {
        return None;
    }
    let tokens = tokens(payload, session.tokens);
    let fires = schedule::advance(&mut session, &modes, tokens, &[], false);
    let _ = state::save(&env.root, &payload.session_id, &session);
    render(&fires, &modes)
}

/// Whatever state this session has, or one rebuilt from the transcript.
///
/// A rebuild leaves every mark at zero, which is deliberate: a session recovered
/// from a fork or from a lost file has definitionally not been refreshed
/// recently, so the next boundary should be the one that refreshes it.
fn recover(payload: &Payload, env: &Env) -> Session {
    if let Some(session) = state::load(&env.root, &payload.session_id) {
        return session;
    }
    let roots = roots(payload, env);
    let modes = transcript_activations(payload)
        .into_iter()
        .filter(|name| resolve::find(name, &roots).is_some())
        .map(|name| Active::new(name, 0))
        .collect();
    Session {
        modes,
        ..Session::default()
    }
}

/// Every `+token` the user ever sent in this conversation, in order.
///
/// The transcript is where activation is written down permanently, in the user's
/// own words. Compaction takes it out of the model's context; it never takes it
/// out of the file.
fn transcript_activations(payload: &Payload) -> Vec<String> {
    #[derive(Deserialize)]
    struct Line {
        #[serde(rename = "type", default)]
        kind: String,
        #[serde(rename = "isSidechain", default)]
        is_sidechain: bool,
        #[serde(default)]
        message: Option<Message>,
    }
    #[derive(Deserialize)]
    struct Message {
        #[serde(default)]
        content: Content,
    }
    #[derive(Default, Deserialize)]
    #[serde(untagged)]
    enum Content {
        Text(String),
        Parts(Vec<Part>),
        #[default]
        Other,
    }
    #[derive(Deserialize)]
    struct Part {
        #[serde(default)]
        text: String,
    }

    if payload.transcript_path.is_empty() {
        return Vec::new();
    }
    let Ok(text) = fs_err::read_to_string(&payload.transcript_path) else {
        return Vec::new();
    };
    let mut found: Vec<String> = Vec::new();
    for line in text.lines() {
        // A tool result is a user line by structure and never by authorship, and
        // it is where the bytes are.
        if !line.contains(r#""type":"user""#) || line.contains(r#""tool_use_id""#) {
            continue;
        }
        let Ok(parsed) = serde_json::from_str::<Line>(line) else {
            continue;
        };
        if parsed.kind != "user" || parsed.is_sidechain {
            continue;
        }
        let Some(message) = parsed.message else {
            continue;
        };
        let prompt = match message.content {
            Content::Text(text) => text,
            Content::Parts(parts) => parts
                .into_iter()
                .map(|p| p.text)
                .collect::<Vec<_>>()
                .join("\n"),
            Content::Other => continue,
        };
        for name in token::activations(&prompt) {
            if !found.contains(&name) {
                found.push(name);
            }
        }
    }
    found
}

/// How full the window is: the transcript first, because it is current; then
/// whatever the harness volunteered; then the last thing this saw.
fn tokens(payload: &Payload, last: u64) -> u64 {
    if !payload.transcript_path.is_empty() {
        let path = Utf8Path::new(&payload.transcript_path);
        if let Some(found) = transcript::context_tokens(path) {
            return found;
        }
    }
    payload.context_tokens.unwrap_or(last)
}

fn roots(payload: &Payload, env: &Env) -> Vec<Utf8PathBuf> {
    let project = std::env::var("CLAUDE_PROJECT_DIR")
        .ok()
        .filter(|p| !p.is_empty())
        .or_else(|| Some(payload.cwd.clone()).filter(|p| !p.is_empty()))
        .map(Utf8PathBuf::from);
    resolve::roots(&env.home, project.as_deref())
}

fn load(session: &Session, payload: &Payload, env: &Env) -> Vec<Mode> {
    load_from(session, &roots(payload, env))
}

/// The mode file behind each active entry. A file that has gone away or stopped
/// parsing simply is not here, which is what makes every caller skip it.
fn load_from(session: &Session, roots: &[Utf8PathBuf]) -> Vec<Mode> {
    session
        .modes
        .iter()
        .filter_map(|active| resolve::find(&active.name, roots))
        .filter_map(|path| {
            let name = path.file_stem()?;
            mode::read(name, &path).ok()
        })
        .collect()
}

fn blocks<'a>(fires: &[Fire], modes: &'a [Mode]) -> Vec<Block<'a>> {
    fires
        .iter()
        .filter_map(|fire| {
            let mode = modes.iter().find(|m| m.name == fire.name)?;
            Some(Block {
                name: &mode.name,
                text: if fire.full { mode.full() } else { mode.short() },
            })
        })
        .collect()
}

/// Activations and refreshes in one injection, each under its own frame.
fn render(fires: &[Fire], modes: &[Mode]) -> Option<String> {
    let split = |want: Occasion| -> Vec<Fire> {
        fires
            .iter()
            .filter(|f| Occasion::of(f) == want)
            .cloned()
            .collect()
    };
    let first = split(Occasion::Activate);
    let again = split(Occasion::Refresh);
    inject::render(&[
        (Occasion::Activate, blocks(&first, modes)),
        (Occasion::Refresh, blocks(&again, modes)),
    ])
}
