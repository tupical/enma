//! The `decision` primitive — the rich centre of the Decisions layer.
//!
//! A decision records a *choice that was made*: not the work to do it (that
//! is Planning/Actions), but the commitment itself and the reasoning that
//! must survive it. The MCPBox charter (vision.md §10 Decisions Core)
//! requires a decision to carry its statement, author, date, rationale, the
//! alternatives that were weighed, the consequences accepted, and the
//! conditions under which it should be revisited.
//!
//! Decisions never executes — it only *fixes the choice*. Nothing here
//! schedules, runs, or mutates a plan; a decision merely [`Link`]s to the
//! sensemaking it rests on and the plans it informs.

use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::id::DecisionId;
use crate::link::Link;
use crate::time::Timestamp;

/// An alternative that was considered but not chosen. Recording the
/// rejected options is what makes a decision auditable later: "why not X?"
/// is answered here, not reconstructed from memory.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Alternative {
    /// The option that was weighed.
    pub option: String,
    /// Why it was not chosen. May be empty when self-evident.
    #[serde(default)]
    pub rejected_because: String,
}

/// A made-and-recorded decision (vision.md §10).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    pub id: DecisionId,
    /// The choice, stated as a commitment ("We will use Postgres for the
    /// primary store").
    pub statement: String,
    /// Who made the decision.
    pub decided_by: Actor,
    /// When the decision was made.
    pub decided_at: Timestamp,
    /// Why — the reasoning behind the choice.
    #[serde(default)]
    pub rationale: String,
    /// The options that were weighed and rejected.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<Alternative>,
    /// What this choice commits us to / costs us (trade-offs accepted).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub consequences: Vec<String>,
    /// When this decision should be reconsidered — a trigger ("if write
    /// throughput exceeds X") or a date, free-form. Empty = no scheduled
    /// review.
    #[serde(default)]
    pub revisit_when: String,
    /// What this decision rests on (Sensemaking) and what it informs
    /// (Planning). See [`Link`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Reasons a [`NewDecision`] is not well-formed. A decision with no statement
/// or no author fixes nothing, so it is rejected before it is materialised.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DecisionError {
    /// The statement is empty or whitespace-only.
    EmptyStatement,
    /// An alternative was recorded with an empty `option`.
    EmptyAlternative,
}

impl std::fmt::Display for DecisionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DecisionError::EmptyStatement => f.write_str("decision statement must not be empty"),
            DecisionError::EmptyAlternative => {
                f.write_str("alternative option must not be empty")
            }
        }
    }
}

impl std::error::Error for DecisionError {}

/// Input for recording a decision. `id` and timestamps are server-assigned;
/// `decided_at` defaults to now when absent.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewDecision {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<DecisionId>,
    pub statement: String,
    pub decided_by: Actor,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub decided_at: Option<Timestamp>,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub alternatives: Vec<Alternative>,
    #[serde(default)]
    pub consequences: Vec<String>,
    #[serde(default)]
    pub revisit_when: String,
    #[serde(default)]
    pub links: Vec<Link>,
}

impl NewDecision {
    /// Validate and materialise a stored [`Decision`], assigning an id and
    /// timestamps. A decision must at least *say what was chosen* — an empty
    /// statement is rejected rather than silently stored.
    pub fn into_decision(self, now: Timestamp) -> Result<Decision, DecisionError> {
        if self.statement.trim().is_empty() {
            return Err(DecisionError::EmptyStatement);
        }
        if self.alternatives.iter().any(|a| a.option.trim().is_empty()) {
            return Err(DecisionError::EmptyAlternative);
        }
        Ok(Decision {
            id: self.id.unwrap_or_default(),
            statement: self.statement,
            decided_by: self.decided_by,
            decided_at: self.decided_at.unwrap_or(now),
            rationale: self.rationale,
            alternatives: self.alternatives,
            consequences: self.consequences,
            revisit_when: self.revisit_when,
            links: self.links,
            created_at: now,
            updated_at: now,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time;

    fn sample() -> NewDecision {
        NewDecision {
            id: None,
            statement: "Use Postgres for the primary store".into(),
            decided_by: Actor::user(),
            decided_at: None,
            rationale: "Relational integrity matters more than write latency here".into(),
            alternatives: vec![Alternative {
                option: "DynamoDB".into(),
                rejected_because: "No multi-row transactions".into(),
            }],
            consequences: vec!["Operate a Postgres instance".into()],
            revisit_when: "if write throughput exceeds 50k/s".into(),
            links: vec![Link::Sensemaking {
                reference: "insight-throughput".into(),
            }],
        }
    }

    #[test]
    fn materialises_with_all_charter_fields() {
        let now = time::now();
        let d = sample().into_decision(now).expect("valid");
        assert!(d.id.to_string().starts_with("dec_"));
        assert_eq!(d.decided_at, now, "decided_at defaults to now when absent");
        assert_eq!(d.created_at, now);
        assert_eq!(d.alternatives.len(), 1);
        assert_eq!(d.consequences.len(), 1);
        assert!(!d.revisit_when.is_empty());
        assert_eq!(d.links.len(), 1);
    }

    #[test]
    fn empty_statement_is_rejected() {
        let mut n = sample();
        n.statement = "   ".into();
        assert_eq!(
            n.into_decision(time::now()).unwrap_err(),
            DecisionError::EmptyStatement
        );
    }

    #[test]
    fn empty_alternative_option_is_rejected() {
        let mut n = sample();
        n.alternatives = vec![Alternative {
            option: "".into(),
            rejected_because: "x".into(),
        }];
        assert_eq!(
            n.into_decision(time::now()).unwrap_err(),
            DecisionError::EmptyAlternative
        );
    }

    #[test]
    fn round_trips_through_json() {
        let d = sample().into_decision(time::now()).unwrap();
        let json = serde_json::to_string(&d).unwrap();
        let back: Decision = serde_json::from_str(&json).unwrap();
        assert_eq!(back, d);
    }

    #[test]
    fn explicit_decided_at_is_preserved() {
        let earlier = time::now() - chrono::Duration::days(1);
        let mut n = sample();
        n.decided_at = Some(earlier);
        let d = n.into_decision(time::now()).unwrap();
        assert_eq!(d.decided_at, earlier);
    }
}
