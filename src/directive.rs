//! The lightweight direction primitives.
//!
//! Beyond the rich [`Decision`](crate::decision::Decision), the Decisions
//! layer fixes six more kinds of direction (vision.md §10 Decisions Core):
//!
//! - **goal** — an outcome we are aiming for;
//! - **non_goal** — something explicitly out of scope (as load-bearing as a
//!   goal: it stops scope creep);
//! - **constraint** — a hard boundary the solution must respect;
//! - **assumption** — something taken as true without proof, to be revisited
//!   if it breaks;
//! - **principle** — a durable value that guides future choices;
//! - **success_criteria** — how we will know the goal was met.
//!
//! These share one shape — a statement, optional rationale, author/date, and
//! cross-layer [`Link`]s — so they are modelled as a single [`Directive`]
//! discriminated by [`DirectiveKind`], rather than six near-identical
//! structs. Only `decision`, which carries alternatives/consequences/revisit
//! conditions, warrants its own type.

use serde::{Deserialize, Serialize};

use crate::actor::Actor;
use crate::id::DirectiveId;
use crate::link::Link;
use crate::time::Timestamp;

/// Which kind of direction a [`Directive`] expresses. The wire strings are
/// the charter's primitive names, so a stored directive reads as
/// `"kind":"non_goal"` etc.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DirectiveKind {
    Goal,
    NonGoal,
    Constraint,
    Assumption,
    Principle,
    SuccessCriteria,
}

impl DirectiveKind {
    /// Stable discriminant stored in the `kind` column.
    pub fn as_str(&self) -> &'static str {
        match self {
            DirectiveKind::Goal => "goal",
            DirectiveKind::NonGoal => "non_goal",
            DirectiveKind::Constraint => "constraint",
            DirectiveKind::Assumption => "assumption",
            DirectiveKind::Principle => "principle",
            DirectiveKind::SuccessCriteria => "success_criteria",
        }
    }

    /// Parse a stored discriminant. `None` for an unknown string
    /// (forward-compatible — a newer producer's kind is simply unmatched,
    /// never a panic).
    pub fn parse_str(s: &str) -> Option<Self> {
        Some(match s {
            "goal" => DirectiveKind::Goal,
            "non_goal" => DirectiveKind::NonGoal,
            "constraint" => DirectiveKind::Constraint,
            "assumption" => DirectiveKind::Assumption,
            "principle" => DirectiveKind::Principle,
            "success_criteria" => DirectiveKind::SuccessCriteria,
            _ => return None,
        })
    }
}

/// A lightweight direction primitive (goal / non_goal / constraint /
/// assumption / principle / success_criteria). Like a decision it only
/// *fixes* direction — it never executes.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Directive {
    pub id: DirectiveId,
    pub kind: DirectiveKind,
    /// The direction, stated plainly ("Ship without a mobile client" for a
    /// non_goal; "p99 latency under 200ms" for a success_criteria).
    pub statement: String,
    /// Who set it.
    pub set_by: Actor,
    /// Optional reasoning / context.
    #[serde(default)]
    pub rationale: String,
    /// What this rests on (Sensemaking) and what it informs (Planning).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<Link>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
}

/// Input for recording a directive. `id` and timestamps are server-assigned.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct NewDirective {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<DirectiveId>,
    pub kind: DirectiveKind,
    pub statement: String,
    pub set_by: Actor,
    #[serde(default)]
    pub rationale: String,
    #[serde(default)]
    pub links: Vec<Link>,
}

impl NewDirective {
    /// Validate and materialise a stored [`Directive`]. A directive with no
    /// statement fixes nothing, so an empty statement is rejected — reusing
    /// [`DecisionError`](crate::decision::DecisionError) for one error type
    /// across the layer.
    pub fn into_directive(
        self,
        now: Timestamp,
    ) -> Result<Directive, crate::decision::DecisionError> {
        if self.statement.trim().is_empty() {
            return Err(crate::decision::DecisionError::EmptyStatement);
        }
        for link in &self.links {
            link.validate()?;
        }
        Ok(Directive {
            id: self.id.unwrap_or_default(),
            kind: self.kind,
            statement: self.statement,
            set_by: self.set_by,
            rationale: self.rationale,
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

    #[test]
    fn kind_round_trips_via_string() {
        for k in [
            DirectiveKind::Goal,
            DirectiveKind::NonGoal,
            DirectiveKind::Constraint,
            DirectiveKind::Assumption,
            DirectiveKind::Principle,
            DirectiveKind::SuccessCriteria,
        ] {
            assert_eq!(DirectiveKind::parse_str(k.as_str()), Some(k));
        }
        assert_eq!(DirectiveKind::parse_str("nope"), None);
    }

    #[test]
    fn non_goal_serialises_snake_case() {
        let json = serde_json::to_string(&DirectiveKind::NonGoal).unwrap();
        assert_eq!(json, "\"non_goal\"");
    }

    #[test]
    fn materialises_and_rejects_empty_statement() {
        let now = time::now();
        let ok = NewDirective {
            id: None,
            kind: DirectiveKind::Goal,
            statement: "Cut onboarding to under 5 minutes".into(),
            set_by: Actor::user(),
            rationale: String::new(),
            links: vec![],
        }
        .into_directive(now)
        .expect("valid");
        assert!(ok.id.to_string().starts_with("dir_"));
        assert_eq!(ok.kind, DirectiveKind::Goal);

        let bad = NewDirective {
            id: None,
            kind: DirectiveKind::NonGoal,
            statement: "  ".into(),
            set_by: Actor::user(),
            rationale: String::new(),
            links: vec![],
        }
        .into_directive(now);
        assert!(bad.is_err());
    }

    #[test]
    fn blank_sensemaking_link_reference_is_rejected() {
        let n = NewDirective {
            id: None,
            kind: DirectiveKind::Constraint,
            statement: "Must run on-prem".into(),
            set_by: Actor::user(),
            rationale: String::new(),
            links: vec![Link::Sensemaking {
                reference: "  ".into(),
            }],
        };
        assert_eq!(
            n.into_directive(time::now()).unwrap_err(),
            crate::decision::DecisionError::EmptyLinkReference
        );
    }
}
