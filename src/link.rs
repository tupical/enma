//! Cross-layer links.
//!
//! Decisions sits between Sensemaking and Planning in the the host pipeline
//! (vision.md §6.3): a decision is *grounded in* what sensemaking surfaced
//! and *informs* the plans that follow. Those neighbours are not modelled
//! here — Decisions only stores a typed reference to them, so the link is a
//! `(kind, target)` pair, never an embedded copy of the other layer's
//! entity.
//!
//! Decisions does not own Sensemaking artifacts or Plans, so the targets are
//! the ids those layers already mint: a free-form Sensemaking reference and
//! local newtypes for `PlanId` / `ProjectId` / `TaskId` that carry the same
//! prefix convention as the daruma originals.  the host casts between them
//! when wiring — the underlying `Uuid` is identical.

use serde::{Deserialize, Serialize};

use crate::decision::DecisionError;
use crate::shared_ids::{PlanId, ProjectId, TaskId};

/// Where a decision (or directive) points, in either direction along the
/// pipeline. The direction is implied by the variant: sensemaking is what a
/// decision is *grounded in*; plans/tasks are what it *informs*.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Link {
    /// Grounded in a Sensemaking output (an insight, a research note, a
    /// framing). Sensemaking ids are not in the shared crate, so the
    /// reference is carried as an opaque string the Sensemaking layer
    /// resolves.
    Sensemaking { reference: String },
    /// Informs / is realised by a plan.
    Plan { id: PlanId },
    /// Scopes / informs a whole project.
    Project { id: ProjectId },
    /// Informs a specific task.
    Task { id: TaskId },
}

impl Link {
    /// Stable discriminant for the `link_kind` column / wire tag.
    pub fn kind(&self) -> &'static str {
        match self {
            Link::Sensemaking { .. } => "sensemaking",
            Link::Plan { .. } => "plan",
            Link::Project { .. } => "project",
            Link::Task { .. } => "task",
        }
    }

    /// Reject a blank reference on the string-carrying variants. `Sensemaking`
    /// is the only one today, but the match is exhaustive so a future
    /// string-like variant is caught here too instead of silently skipped.
    /// A blank reference points at nothing, which breaks lineage on the
    /// other end of the link.
    pub fn validate(&self) -> Result<(), DecisionError> {
        match self {
            Link::Sensemaking { reference } if reference.trim().is_empty() => {
                Err(DecisionError::EmptyLinkReference)
            }
            Link::Sensemaking { .. }
            | Link::Plan { .. }
            | Link::Project { .. }
            | Link::Task { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn link_round_trips_with_kind_tag() {
        let l = Link::Sensemaking {
            reference: "insight-42".into(),
        };
        let json = serde_json::to_string(&l).unwrap();
        assert!(json.contains("\"kind\":\"sensemaking\""), "got {json}");
        let back: Link = serde_json::from_str(&json).unwrap();
        assert_eq!(back, l);
        assert_eq!(l.kind(), "sensemaking");
    }

    #[test]
    fn plan_link_round_trips() {
        let id = PlanId::new();
        let l = Link::Plan { id };
        let json = serde_json::to_string(&l).unwrap();
        let back: Link = serde_json::from_str(&json).unwrap();
        assert_eq!(back, l);
    }

    #[test]
    fn blank_sensemaking_reference_is_rejected() {
        let l = Link::Sensemaking {
            reference: "   ".into(),
        };
        assert_eq!(l.validate().unwrap_err(), DecisionError::EmptyLinkReference);

        let empty = Link::Sensemaking {
            reference: "".into(),
        };
        assert_eq!(
            empty.validate().unwrap_err(),
            DecisionError::EmptyLinkReference
        );
    }

    #[test]
    fn non_blank_sensemaking_reference_is_accepted() {
        let l = Link::Sensemaking {
            reference: "insight-42".into(),
        };
        assert!(l.validate().is_ok());
    }

    #[test]
    fn id_carrying_links_are_always_valid() {
        assert!(Link::Plan { id: PlanId::new() }.validate().is_ok());
        assert!(Link::Project {
            id: ProjectId::new()
        }
        .validate()
        .is_ok());
        assert!(Link::Task { id: TaskId::new() }.validate().is_ok());
    }
}
