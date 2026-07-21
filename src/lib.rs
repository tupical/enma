//! `enma` — the Decisions layer of the the host family.
//!
//! In the the host pipeline (Intake → Sensemaking → **Decisions** → Planning →
//! Actions → execution; vision.md §6.3, §10) the Decisions layer turns
//! understanding into *direction*. It owns the goal-and-decision primitives:
//!
//! - [`Decision`] — a recorded choice, with its statement, author, date,
//!   rationale, the [`Alternative`]s weighed, the consequences accepted, and
//!   the conditions under which it should be revisited;
//! - [`Directive`] — the six lighter direction primitives discriminated by
//!   [`DirectiveKind`]: `goal`, `non_goal`, `constraint`, `assumption`,
//!   `principle`, `success_criteria`.
//!
//! Both [`Link`] back to the Sensemaking they rest on and forward to the
//! Planning they inform.
//!
//! # Contract
//! - Decisions **never executes**. It only *fixes the choice* — nothing here
//!   schedules, runs, or mutates a plan. Realising a decision is Planning's
//!   and Actions' job.
//! - Domain primitives stay pure; [`decide_ai`] is a provider-neutral async
//!   operation. This crate has no dependency on daruma or sibling layers.
//! - All JSON is serde-derived; ids, `Timestamp`, and `Actor` are local
//!   primitives — the host maps them to/from daruma types when wiring.
//! - Wire-level sensemaking adapters live here without depending on sibling crates.

pub mod actor;
pub mod ai;
pub mod decide;
pub mod decision;
pub mod directive;
pub mod error;
pub mod id;
pub mod link;
pub mod time;

pub use actor::{Actor, ActorKind};
pub use ai::{AiError, AiOutput, AiProvider, AiRequest, ToolCall};
pub use decide::decide_ai;
pub use decision::{Alternative, Decision, DecisionError, NewDecision};
pub use directive::{Directive, DirectiveKind, NewDirective};
pub use error::DecidingError;
pub use id::{DecisionId, DirectiveId, PlanId, ProjectId, TaskId};
pub use link::Link;
pub use time::Timestamp;

/// Only insights and hypotheses are useful promotion candidates.
pub fn is_decision_worthy(sensing_kind: &str) -> bool {
    matches!(sensing_kind, "insight" | "hypothesis")
}

/// Build a decision from the wire fields of an upstream sensing item.
pub fn decision_from_sensing(
    statement: String,
    source_ref: Option<String>,
    decided_by: Actor,
    now: Timestamp,
) -> Result<Decision, DecisionError> {
    let links = source_ref
        .iter()
        .map(|reference| Link::Sensemaking {
            reference: reference.clone(),
        })
        .collect();
    NewDecision {
        id: None,
        statement,
        decided_by,
        decided_at: None,
        rationale: source_ref
            .as_ref()
            .map(|reference| format!("Promoted from sensing item {reference}"))
            .unwrap_or_default(),
        alternatives: Vec::new(),
        consequences: Vec::new(),
        revisit_when: String::new(),
        links,
    }
    .into_decision(now)
}

#[cfg(test)]
mod adapter_tests {
    use super::*;

    #[test]
    fn sensing_fields_are_preserved_in_decision() {
        let decision = decision_from_sensing(
            "Choose SQLite".into(),
            Some("sense_1".into()),
            Actor::user(),
            Timestamp::from_timestamp_secs(1).unwrap(),
        )
        .unwrap();
        assert_eq!(decision.statement, "Choose SQLite");
        assert_eq!(decision.rationale, "Promoted from sensing item sense_1");
        assert_eq!(
            decision.links,
            vec![Link::Sensemaking {
                reference: "sense_1".into()
            }]
        );
        assert!(is_decision_worthy("insight"));
        assert!(is_decision_worthy("hypothesis"));
        assert!(!is_decision_worthy("knowledge"));
    }
}
