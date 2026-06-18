//! Local `Actor` primitive — who authored a decision or directive.
//!
//! Self-contained replacement for `taskagent_domain::Actor`.  The shape is
//! intentionally minimal: a discriminated kind (`user` / `agent`) and an
//! opaque string id.  mcpbox maps to/from taskagent's own `Actor` when
//! wiring the layer — the wire strings are identical, so round-trips are
//! lossless.

use serde::{Deserialize, Serialize};

/// Who authored a [`Decision`](crate::decision::Decision) or
/// [`Directive`](crate::directive::Directive).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Actor {
    pub kind: ActorKind,
    /// Opaque identifier — a user-id, agent-slug, or service name.
    pub id: String,
}

/// Discriminates between a human and an automated agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
    User,
    Agent,
}

impl Actor {
    /// Convenience constructor: anonymous human actor (`"user"`).
    pub fn user() -> Self {
        Self {
            kind: ActorKind::User,
            id: "user".into(),
        }
    }

    /// Convenience constructor: named agent actor.
    pub fn agent(id: impl Into<String>) -> Self {
        Self {
            kind: ActorKind::Agent,
            id: id.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_actor_serialises_snake_case() {
        let a = Actor::user();
        let json = serde_json::to_string(&a).unwrap();
        assert!(json.contains("\"user\""), "got {json}");
    }

    #[test]
    fn agent_actor_round_trips() {
        let a = Actor::agent("decisions-ai");
        let json = serde_json::to_string(&a).unwrap();
        let back: Actor = serde_json::from_str(&json).unwrap();
        assert_eq!(back, a);
    }
}
