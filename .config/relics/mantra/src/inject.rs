//! The text a hook hands back, and the frame around it.
//!
//! A block is `===== MODE: name =====` and a payload, unchanged from the
//! expander this replaced, because that shape is already what the corpus was
//! written against.
//!
//! What is new is that a boundary can carry more than one **occasion**, and the
//! frame has to say which. A first delivery and a re-statement are different
//! claims: one introduces a directive, the other insists on one the model has
//! already been told and may have drifted off. Collapsing them into a single
//! preamble would make every refresh read like a new instruction, which is
//! exactly the reading that lets a model treat it as boilerplate.

use std::fmt::Write;

use crate::schedule::Fire;

/// Why something is being said.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Occasion {
    /// A `+token`, or the first time a deferred mode comes due.
    Activate,
    /// A mode that has been said before, said again.
    Refresh,
    /// Everything active, after a compaction took the originals away.
    Restate,
}

impl Occasion {
    fn preamble(self) -> &'static str {
        match self {
            Self::Activate => {
                "The user activated session modes via `+<name>` lines in the prompt. Those `+` \
                 tokens are mode selectors — treat them as markers, not as task content. The mode \
                 directives below are ACTIVE and BINDING for this session; apply each unless the \
                 user explicitly overrides it."
            }
            Self::Refresh => {
                "Session modes still active. Their standing directives are re-stated below so they \
                 do not fade with distance — unchanged, and still BINDING."
            }
            Self::Restate => {
                "The context was just compacted, which summarized away the session modes activated \
                 earlier. They are still ACTIVE and BINDING; their directives are re-stated below \
                 in full."
            }
        }
    }

    /// Which occasion a fire belongs to, outside a compaction.
    pub fn of(fire: &Fire) -> Self {
        if fire.full {
            Self::Activate
        } else {
            Self::Refresh
        }
    }
}

/// One mode's payload.
pub struct Block<'a> {
    /// The `+token`.
    pub name: &'a str,
    /// Body or refrain, already chosen.
    pub text: &'a str,
}

/// The whole injection, or nothing when there is nothing to say.
///
/// Occasions keep their own preamble and their blocks stay together, in the
/// order given, so an injection reads the same way twice.
pub fn render(sections: &[(Occasion, Vec<Block<'_>>)]) -> Option<String> {
    let mut out = String::new();
    for (occasion, blocks) in sections {
        if blocks.is_empty() {
            continue;
        }
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(occasion.preamble());
        for block in blocks {
            // `write!` into a String cannot fail; the result is discarded rather
            // than unwrapped so no error arm exists to be mistaken for one.
            let _ = write!(out, "\n\n===== MODE: {} =====\n{}", block.name, block.text);
        }
        out.push('\n');
    }
    if out.is_empty() { None } else { Some(out) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_to_say_is_no_injection() {
        assert_eq!(render(&[]), None);
        assert_eq!(render(&[(Occasion::Refresh, Vec::new())]), None);
    }

    #[test]
    fn a_block_keeps_the_shape_the_corpus_was_written_against() {
        let out = render(&[(
            Occasion::Activate,
            vec![Block {
                name: "terse",
                text: "Say few words.",
            }],
        )])
        .expect("one block");
        assert!(out.contains("===== MODE: terse =====\nSay few words."));
        assert!(out.starts_with("The user activated"));
    }

    #[test]
    fn two_occasions_keep_their_own_frame() {
        let out = render(&[
            (
                Occasion::Activate,
                vec![Block {
                    name: "slash",
                    text: "Expunge.",
                }],
            ),
            (
                Occasion::Refresh,
                vec![Block {
                    name: "terse",
                    text: "Cut filler.",
                }],
            ),
        ])
        .expect("two sections");
        let activate = out.find("The user activated").expect("activation frame");
        let refresh = out.find("still active").expect("refresh frame");
        assert!(activate < refresh, "{out}");
        assert!(out.find("slash").expect("slash") < refresh);
    }

    #[test]
    fn a_full_payload_reads_as_an_activation() {
        assert_eq!(
            Occasion::of(&Fire {
                name: "terse".to_owned(),
                full: true
            }),
            Occasion::Activate
        );
        assert_eq!(
            Occasion::of(&Fire {
                name: "terse".to_owned(),
                full: false
            }),
            Occasion::Refresh
        );
    }
}
