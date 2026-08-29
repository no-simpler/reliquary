//! Deciding what gets said, and whether in full.
//!
//! One function advances a session against the window and reports what to
//! deliver; another reports the same reasoning without changing anything, which
//! is what `mantra explain` is for. A rules engine you cannot interrogate is a
//! haunted house, so the two are written together and share their vocabulary.
//!
//! **The first delivery is always the body.** A refrain is for *re*-delivery, so
//! a mode that has never been said says itself in full whichever clause finally
//! calls for it. That is what makes a deferred mode work: `paced` carries no
//! `on: activate`, so it says nothing until the session is large enough, and
//! when it does it says the whole thing rather than an abbreviation of something
//! nobody has read.
//!
//! **A `when` edge is consumed by crossing it, not by being the reason
//! something was said.** Otherwise a mode that happened to be refreshing on the
//! same boundary would leave the edge armed, and it would fire again next turn.

use crate::mode::Mode;
use crate::state::{Active, Session};
use crate::trigger::{Moment, Trigger};

/// One mode, due now.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fire {
    /// Which mode.
    pub name: String,
    /// Whether it says its body rather than its refrain.
    pub full: bool,
}

/// Advances `state` to `tokens` and returns what is due, in activation order.
///
/// `activated` names the modes switched on at this very boundary — an `on:
/// activate` clause is due then and at no other time. `restate` is compaction:
/// every mode says its body again, because the context that held it is gone.
pub fn advance(
    state: &mut Session,
    modes: &[Mode],
    tokens: u64,
    activated: &[String],
    restate: bool,
) -> Vec<Fire> {
    state.tokens = tokens;
    let mut due = Vec::new();
    for active in &mut state.modes {
        let Some(mode) = modes.iter().find(|m| m.name == active.name) else {
            // The file went away mid-session. Nothing to say, and nothing to
            // forget either: putting it back should resume it.
            continue;
        };
        // A mark ahead of the window is stale: compaction shrinks the window,
        // and the transcript that reports it is written asynchronously, so a
        // mark set from a lagging read can otherwise sit permanently out of
        // reach and the clause never fires again.
        active.last_fired_at = active.last_fired_at.min(tokens);
        let crossed = crossed(mode, active, tokens);
        let fires = restate
            || !crossed.is_empty()
            || (activated.contains(&active.name) && wants_activation(mode))
            || periodic_due(mode, active, tokens);
        active.latched.extend(crossed);
        if !fires {
            continue;
        }
        let full = restate || active.fires == 0;
        active.fires = active.fires.saturating_add(1);
        active.last_fired_at = tokens;
        due.push(Fire {
            name: active.name.clone(),
            full,
        });
    }
    due
}

/// The `when` marks this mode crosses at `tokens` and has not crossed before.
fn crossed(mode: &Mode, active: &Active, tokens: u64) -> Vec<u64> {
    mode.triggers
        .iter()
        .filter_map(|trigger| match trigger {
            Trigger::When(when) => Some(when.context_tokens),
            Trigger::On(_) | Trigger::Every(_) => None,
        })
        .filter(|mark| tokens >= *mark && !active.latched.contains(mark))
        .collect()
}

fn wants_activation(mode: &Mode) -> bool {
    mode.triggers
        .iter()
        .any(|t| matches!(t, Trigger::On(Moment::Activate)))
}

fn periodic_due(mode: &Mode, active: &Active, tokens: u64) -> bool {
    mode.triggers.iter().any(|trigger| match trigger {
        Trigger::Every(every) => tokens >= active.last_fired_at.saturating_add(every.tokens),
        Trigger::On(_) | Trigger::When(_) => false,
    })
}

/// One clause of one mode's schedule, and where it stands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Standing {
    /// The clause, as a mode file spells it.
    pub trigger: Trigger,
    /// Where it stands right now, in one phrase.
    pub note: String,
}

/// Why each clause of `mode` did or did not fire, changing nothing.
pub fn standing(mode: &Mode, active: &Active, tokens: u64) -> Vec<Standing> {
    mode.triggers
        .iter()
        .map(|trigger| {
            let note = match trigger {
                Trigger::On(Moment::Activate) => {
                    if active.fires == 0 {
                        "spent — activation has passed".to_owned()
                    } else {
                        format!("said at {}", active.activated_at)
                    }
                }
                Trigger::Every(every) => {
                    let next = active.last_fired_at.saturating_add(every.tokens);
                    if tokens >= next {
                        "due now".to_owned()
                    } else {
                        format!("due in {}", next - tokens)
                    }
                }
                Trigger::When(when) => {
                    if active.latched.contains(&when.context_tokens) {
                        "spent — the mark was crossed".to_owned()
                    } else if tokens >= when.context_tokens {
                        "due now".to_owned()
                    } else {
                        format!("waiting — {} away", when.context_tokens - tokens)
                    }
                }
            };
            Standing {
                trigger: *trigger,
                note,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::trigger::{Every, When};
    use camino::Utf8PathBuf;

    fn mode(name: &str, triggers: Vec<Trigger>) -> Mode {
        Mode {
            name: name.to_owned(),
            path: Utf8PathBuf::from(format!("/modes/{name}.md")),
            triggers,
            refrain: Some("short".to_owned()),
            body: "long".to_owned(),
        }
    }

    fn session(names: &[&str], tokens: u64) -> Session {
        Session {
            modes: names
                .iter()
                .map(|n| Active::new((*n).to_owned(), tokens))
                .collect(),
            tokens,
            ..Session::default()
        }
    }

    fn names(fires: &[Fire]) -> Vec<(&str, bool)> {
        fires
            .iter()
            .map(|f| (f.name.as_str(), f.full))
            .collect::<Vec<_>>()
    }

    #[test]
    fn activation_says_the_body_once() {
        let modes = [mode("terse", vec![Trigger::On(Moment::Activate)])];
        let mut state = session(&["terse"], 100);
        let first = advance(&mut state, &modes, 100, &["terse".to_owned()], false);
        assert_eq!(names(&first), [("terse", true)]);
        // The same boundary cannot come round again, and no other clause exists.
        let next = advance(&mut state, &modes, 200, &[], false);
        assert!(next.is_empty());
    }

    #[test]
    fn a_mode_without_an_activation_clause_says_nothing_when_switched_on() {
        let modes = [mode(
            "paced",
            vec![Trigger::When(When {
                context_tokens: 500_000,
            })],
        )];
        let mut state = session(&["paced"], 100);
        assert!(advance(&mut state, &modes, 100, &["paced".to_owned()], false).is_empty());
    }

    #[test]
    fn a_periodic_clause_fires_on_the_mark_and_not_between() {
        let modes = [mode(
            "terse",
            vec![
                Trigger::On(Moment::Activate),
                Trigger::Every(Every { tokens: 25_000 }),
            ],
        )];
        let mut state = session(&["terse"], 1_000);
        advance(&mut state, &modes, 1_000, &["terse".to_owned()], false);
        assert!(advance(&mut state, &modes, 20_000, &[], false).is_empty());
        assert!(advance(&mut state, &modes, 25_999, &[], false).is_empty());
        let due = advance(&mut state, &modes, 26_000, &[], false);
        assert_eq!(names(&due), [("terse", false)], "a refresh, not the body");
        assert!(advance(&mut state, &modes, 30_000, &[], false).is_empty());
    }

    #[test]
    fn a_deferred_clause_says_the_body_because_nothing_said_it_yet() {
        let modes = [mode(
            "paced",
            vec![Trigger::When(When {
                context_tokens: 500_000,
            })],
        )];
        let mut state = session(&["paced"], 1_000);
        assert!(advance(&mut state, &modes, 499_999, &[], false).is_empty());
        assert_eq!(
            names(&advance(&mut state, &modes, 500_000, &[], false)),
            [("paced", true)]
        );
    }

    #[test]
    fn a_deferred_clause_fires_once_per_edge() {
        let modes = [mode(
            "paced",
            vec![Trigger::When(When {
                context_tokens: 500_000,
            })],
        )];
        let mut state = session(&["paced"], 1_000);
        advance(&mut state, &modes, 600_000, &[], false);
        assert!(advance(&mut state, &modes, 700_000, &[], false).is_empty());
    }

    #[test]
    fn switching_on_past_the_mark_fires_immediately() {
        let modes = [mode(
            "paced",
            vec![Trigger::When(When {
                context_tokens: 500_000,
            })],
        )];
        let mut state = session(&["paced"], 600_000);
        assert_eq!(
            names(&advance(
                &mut state,
                &modes,
                600_000,
                &["paced".to_owned()],
                false
            )),
            [("paced", true)]
        );
    }

    #[test]
    fn an_edge_crossed_on_a_boundary_something_else_owned_is_still_spent() {
        let modes = [mode(
            "both",
            vec![
                Trigger::Every(Every { tokens: 10 }),
                Trigger::When(When {
                    context_tokens: 100,
                }),
            ],
        )];
        let mut state = session(&["both"], 0);
        // The periodic clause is what makes this due; the mark is crossed on the
        // same boundary and must not stay armed for the next one.
        advance(&mut state, &modes, 100, &[], false);
        assert_eq!(state.modes[0].latched, [100]);
    }

    #[test]
    fn compaction_restates_every_mode_in_full() {
        let modes = [
            mode("terse", vec![Trigger::On(Moment::Activate)]),
            mode("robust", vec![Trigger::On(Moment::Activate)]),
        ];
        let mut state = session(&["terse", "robust"], 1_000);
        advance(
            &mut state,
            &modes,
            1_000,
            &["terse".to_owned(), "robust".to_owned()],
            false,
        );
        let after = advance(&mut state, &modes, 9_958, &[], true);
        assert_eq!(names(&after), [("terse", true), ("robust", true)]);
        assert_eq!(state.modes[0].last_fired_at, 9_958);
    }

    #[test]
    fn compaction_does_not_re_arm_a_spent_edge() {
        let modes = [mode(
            "paced",
            vec![Trigger::When(When {
                context_tokens: 400_000,
            })],
        )];
        let mut state = session(&["paced"], 1_000);
        advance(&mut state, &modes, 400_000, &[], false);
        advance(&mut state, &modes, 9_958, &[], true);
        assert!(advance(&mut state, &modes, 400_000, &[], false).is_empty());
    }

    #[test]
    fn a_mark_left_ahead_of_the_window_is_clamped_back_onto_it() {
        let modes = [mode(
            "terse",
            vec![Trigger::Every(Every { tokens: 25_000 })],
        )];
        let mut state = session(&["terse"], 400_000);
        state.modes[0].last_fired_at = 400_000;
        state.modes[0].fires = 1;
        // The window shrank to a tenth of the mark. Without the clamp nothing
        // fires again until the session is bigger than it was before.
        assert!(advance(&mut state, &modes, 9_958, &[], false).is_empty());
        assert_eq!(state.modes[0].last_fired_at, 9_958);
        assert_eq!(
            names(&advance(&mut state, &modes, 34_958, &[], false)),
            [("terse", false)]
        );
    }

    #[test]
    fn a_mode_whose_file_vanished_says_nothing_and_stays_on() {
        let mut state = session(&["gone"], 100);
        assert!(advance(&mut state, &[], 200, &["gone".to_owned()], false).is_empty());
        assert!(state.holds("gone"));
    }

    #[test]
    fn standing_reports_the_reason_a_clause_is_waiting() {
        let mode = mode(
            "terse",
            vec![
                Trigger::On(Moment::Activate),
                Trigger::Every(Every { tokens: 25_000 }),
                Trigger::When(When {
                    context_tokens: 500_000,
                }),
            ],
        );
        let active = Active {
            name: "terse".to_owned(),
            activated_at: 1_000,
            last_fired_at: 10_000,
            fires: 2,
            latched: Vec::new(),
        };
        let notes: Vec<String> = standing(&mode, &active, 20_000)
            .into_iter()
            .map(|s| s.note)
            .collect();
        assert_eq!(
            notes,
            ["said at 1000", "due in 15000", "waiting — 480000 away"]
        );
    }
}
