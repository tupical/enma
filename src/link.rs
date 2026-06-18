//! Cross-layer links.
//!
//! Decisions sits between Sensemaking and Planning in the MCPBox pipeline
//! (vision.md §6.3): a decision is *grounded in* what sensemaking surfaced
//! and *informs* the plans that follow. Those neighbours are not modelled
//! here — Decisions only stores a typed reference to them, so the link is a
//! `(kind, target)` pair, never an embedded copy of the other layer's
//! entity.
//!
//! Decisions does not own Sensemaking artifacts or Plans, so the targets are
//! the ids those layers already mint: a free-form Sensemaking reference and
//! local newtypes for `PlanId` / `ProjectId` / `TaskId` that carry the same
//! prefix convention as the taskagent originals.  mcpbox casts between them
//! when wiring — the underlying `Uuid` is identical.

use serde::{Deserialize, Serialize};

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
}
