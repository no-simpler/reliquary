use std::fmt;
use std::path::PathBuf;

use anyhow::{Result, bail};
use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::id::Id;

/// Whole seconds. Nothing on a docket turns on milliseconds, and a shorter
/// stamp is a shorter line to read.
pub fn now() -> Timestamp {
    Timestamp::now()
        .round(jiff::Unit::Second)
        .unwrap_or_else(|_| Timestamp::now())
}

#[derive(
    Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize, clap::ValueEnum,
)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    Handoff,
    Relay,
    Spec,
}

impl Kind {
    pub fn dir(self) -> &'static str {
        match self {
            Kind::Handoff => "handoffs",
            Kind::Relay => "relays",
            Kind::Spec => "specs",
        }
    }

    pub const ALL: [Kind; 3] = [Kind::Handoff, Kind::Relay, Kind::Spec];
}

impl fmt::Display for Kind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Kind::Handoff => "handoff",
            Kind::Relay => "relay",
            Kind::Spec => "spec",
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Stage {
    Design,
    Implementation,
}

impl fmt::Display for Stage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Stage::Design => "design",
            Stage::Implementation => "implementation",
        })
    }
}

/// Provenance of a relay chain, minted when a handoff becomes a relay and
/// carried unchanged through every later hop and promotion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chain {
    pub chain: Id,
    pub hop: u32,
    pub supersedes: Option<Id>,
}

/// The rung an item occupies. Constructing one is the only way to express a
/// kind, so `stage` cannot exist on a handoff and a relay cannot lack its chain.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Rung {
    Handoff,
    Relay(Chain),
    Spec { stage: Stage, chain: Option<Chain> },
}

impl Rung {
    pub fn kind(&self) -> Kind {
        match self {
            Rung::Handoff => Kind::Handoff,
            Rung::Relay(_) => Kind::Relay,
            Rung::Spec { .. } => Kind::Spec,
        }
    }

    pub fn chain(&self) -> Option<&Chain> {
        match self {
            Rung::Handoff => None,
            Rung::Relay(chain) => Some(chain),
            Rung::Spec { chain, .. } => chain.as_ref(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Item {
    pub id: Id,
    pub name: String,
    pub tagline: String,
    pub project: PathBuf,
    pub created: Timestamp,
    pub updated: Timestamp,
    pub order: i64,
    pub rung: Rung,
    pub blocked: Option<String>,
    pub origin: Option<PathBuf>,
    pub tags: Vec<String>,
}

impl Item {
    pub fn kind(&self) -> Kind {
        self.rung.kind()
    }

    pub fn is_blocked(&self) -> bool {
        self.blocked.as_ref().is_some_and(|b| !b.trim().is_empty())
    }

    /// What `promote` would do next, or `None` at the top of the ladder.
    pub fn next_step(&self) -> Option<Step> {
        match &self.rung {
            Rung::Handoff => Some(Step::ToRelay),
            Rung::Relay(_) => Some(Step::ToSpec),
            Rung::Spec {
                stage: Stage::Design,
                ..
            } => Some(Step::ToImplementation),
            Rung::Spec {
                stage: Stage::Implementation,
                ..
            } => None,
        }
    }

    /// Advance one rung, or jump to `target` when one is named. Forward only:
    /// every rejected transition names the command that would have worked.
    pub fn promote(&mut self, target: Option<Kind>) -> Result<Step> {
        let step = match (target, &self.rung) {
            (None, _) => match self.next_step() {
                Some(step) => step,
                None => bail!(
                    "{} is already a spec in implementation, the top of the ladder. \
                     Close it when its work lands: docket close {}",
                    self.id,
                    self.id
                ),
            },
            (Some(Kind::Relay), Rung::Handoff) => Step::ToRelay,
            (Some(Kind::Spec), Rung::Handoff) => Step::SkipToSpec,
            (Some(Kind::Spec), Rung::Relay(_)) => Step::ToSpec,
            (Some(Kind::Handoff), _) => bail!(
                "the ladder runs forward only, and handoff is its first rung. \
                 {} is a {}",
                self.id,
                self.kind()
            ),
            (Some(Kind::Relay), rung) => bail!(
                "{} is a {} — the ladder runs forward only, so it cannot become a relay",
                self.id,
                rung.kind()
            ),
            (Some(Kind::Spec), Rung::Spec { .. }) => bail!(
                "{} is already a spec. To advance its stage, drop the flag: docket promote {}",
                self.id,
                self.id
            ),
        };

        self.rung = match (step, std::mem::replace(&mut self.rung, Rung::Handoff)) {
            (Step::ToRelay, Rung::Handoff) => Rung::Relay(Chain {
                chain: self.id,
                hop: 1,
                supersedes: None,
            }),
            (Step::SkipToSpec, Rung::Handoff) => Rung::Spec {
                stage: Stage::Design,
                chain: None,
            },
            (Step::ToSpec, Rung::Relay(chain)) => Rung::Spec {
                stage: Stage::Design,
                chain: Some(chain),
            },
            (Step::ToImplementation, Rung::Spec { chain, .. }) => Rung::Spec {
                stage: Stage::Implementation,
                chain,
            },
            (_, rung) => rung,
        };
        Ok(step)
    }

    /// The successor a consumed relay owes: same chain, next hop, superseding
    /// the item it was minted from.
    pub fn successor(&self, id: Id, name: String, tagline: String) -> Result<Item> {
        let Rung::Relay(chain) = &self.rung else {
            bail!(
                "{} is a {}; only a relay owes a successor. Promote it first: docket promote {}",
                self.id,
                self.kind(),
                self.id
            );
        };
        let now = now();
        Ok(Item {
            id,
            name,
            tagline,
            project: self.project.clone(),
            created: now,
            updated: now,
            order: self.order,
            rung: Rung::Relay(Chain {
                chain: chain.chain,
                hop: chain.hop + 1,
                supersedes: Some(self.id),
            }),
            blocked: None,
            origin: self.origin.clone(),
            tags: self.tags.clone(),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Step {
    ToRelay,
    ToSpec,
    SkipToSpec,
    ToImplementation,
}

impl fmt::Display for Step {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Step::ToRelay => "handoff -> relay",
            Step::ToSpec => "relay -> spec:design",
            Step::SkipToSpec => "handoff -> spec:design",
            Step::ToImplementation => "spec:design -> spec:implementation",
        })
    }
}

/// The on-disk projection. Field order here is the canonical frontmatter key
/// order, and every rung's fields are a superset of the rung below it, so a
/// promotion only ever adds keys.
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Wire {
    pub id: Id,
    pub kind: Kind,
    /// Each alias is what lets every item written before that field was renamed
    /// load unchanged; the next write puts it under the current key.
    #[serde(alias = "title")]
    pub name: String,
    #[serde(alias = "description")]
    pub tagline: String,
    pub project: PathBuf,
    pub created: Timestamp,
    pub updated: Timestamp,
    pub order: i64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chain: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hop: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supersedes: Option<Id>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<Stage>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub origin: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
}

impl TryFrom<Wire> for Item {
    type Error = anyhow::Error;

    fn try_from(w: Wire) -> Result<Item> {
        let chain = match (w.chain, w.hop) {
            (Some(chain), Some(hop)) => Some(Chain {
                chain,
                hop,
                supersedes: w.supersedes,
            }),
            (None, None) if w.supersedes.is_none() => None,
            _ => bail!(
                "chain, hop and supersedes describe one relay chain: give chain and hop together, or neither"
            ),
        };

        let rung = match w.kind {
            Kind::Handoff => {
                if chain.is_some() {
                    bail!("kind is handoff, but chain fields are present — a handoff has no chain");
                }
                if w.stage.is_some() {
                    bail!("kind is handoff, but stage is present — only a spec carries a stage");
                }
                Rung::Handoff
            }
            Kind::Relay => {
                if w.stage.is_some() {
                    bail!("kind is relay, but stage is present — only a spec carries a stage");
                }
                match chain {
                    Some(chain) => Rung::Relay(chain),
                    None => bail!("kind is relay, so chain and hop are required"),
                }
            }
            Kind::Spec => match w.stage {
                Some(stage) => Rung::Spec { stage, chain },
                None => bail!("kind is spec, so stage is required (design or implementation)"),
            },
        };

        Ok(Item {
            id: w.id,
            name: w.name,
            tagline: w.tagline,
            project: w.project,
            created: w.created,
            updated: w.updated,
            order: w.order,
            rung,
            blocked: w.blocked,
            origin: w.origin,
            tags: w.tags,
        })
    }
}

impl From<&Item> for Wire {
    fn from(item: &Item) -> Wire {
        let chain = item.rung.chain();
        Wire {
            id: item.id,
            kind: item.kind(),
            name: item.name.clone(),
            tagline: item.tagline.clone(),
            project: item.project.clone(),
            created: item.created,
            updated: item.updated,
            order: item.order,
            chain: chain.map(|c| c.chain),
            hop: chain.map(|c| c.hop),
            supersedes: chain.and_then(|c| c.supersedes),
            stage: match &item.rung {
                Rung::Spec { stage, .. } => Some(*stage),
                _ => None,
            },
            blocked: item.blocked.clone(),
            origin: item.origin.clone(),
            tags: item.tags.clone(),
        }
    }
}
