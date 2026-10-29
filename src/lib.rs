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
pub use ai::{AiError, AiOutput, AiProvider, AiRequest, AiUsage, ToolCall};
pub use decide::{decide_ai, Decided};
pub use decision::{Alternative, Decision, DecisionError, NewDecision};
pub use directive::{Directive, DirectiveKind, NewDirective};
pub use error::DecidingError;
pub use id::{DecisionId, DirectiveId, PlanId, ProjectId, TaskId};
pub use link::Link;
pub use time::Timestamp;

/// Only insights are promotion candidates (ADR-0011): a hypothesis is an
/// untested claim, not a decision, until it is confirmed as insight/knowledge
/// or explicitly rejected.
pub fn is_decision_worthy(sensing_kind: &str) -> bool {
    matches!(sensing_kind, "insight")
}

/// Kinds `enma.decide` refuses outright (ADR-0011 fail-closed gate): an
/// untested hypothesis never becomes a decision. Narrower than
/// [`is_decision_worthy`] on purpose — a hypothesis confirmed as `knowledge`
/// continues on the product path as before (ADR-0011 §4).
pub fn blocks_decision(sensing_kind: &str) -> bool {
    matches!(sensing_kind, "hypothesis")
}

/// Kinds that are themselves an unanswered question (ADR-0007 §2): deciding
/// on them would invent the answer, so `enma.decide` asks for input instead.
pub fn is_open_question(sensing_kind: &str) -> bool {
    matches!(sensing_kind, "question" | "research_gap")
}

/// Questions a human must answer before a decision can rest on this sensing
/// item (ADR-0007 §2 fail-closed gate): its non-blank `open_questions`, or
/// its body when the item is itself a question/research gap. Empty = no
/// blocker. Deterministic, so the gate works with no AI provider.
pub fn unanswered_questions(sensing_item: &serde_json::Value) -> Vec<String> {
    let mut questions: Vec<String> = Vec::new();
    for q in sensing_item["open_questions"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(q) = q.as_str().map(str::trim).filter(|q| !q.is_empty()) {
            if !questions.iter().any(|known| known == q) {
                questions.push(q.to_string());
            }
        }
    }
    let kind = sensing_item["kind"].as_str().unwrap_or_default();
    if questions.is_empty() && is_open_question(kind) {
        let body = sensing_item["body"]
            .as_str()
            .map(str::trim)
            .unwrap_or_default();
        questions.push(if body.is_empty() {
            kind.to_string()
        } else {
            body.to_string()
        });
    }
    questions
}

#[cfg(test)]
mod adapter_tests {
    use super::*;

    #[test]
    fn decision_worthy_sensing_kinds_are_selected() {
        assert!(is_decision_worthy("insight"));
        assert!(!is_decision_worthy("hypothesis"));
        assert!(!is_decision_worthy("knowledge"));
        assert!(!is_decision_worthy("rejected_idea"));
    }

    #[test]
    fn only_hypothesis_blocks_a_decision() {
        assert!(blocks_decision("hypothesis"));
        assert!(!blocks_decision("insight"));
        assert!(!blocks_decision("knowledge"));
        assert!(!blocks_decision(""));
    }

    #[test]
    fn open_questions_and_question_kinds_are_unanswered() {
        use serde_json::json;
        assert_eq!(
            unanswered_questions(
                &json!({"kind": "insight", "open_questions": ["Who pays?", " ", "Who pays?", 7]})
            ),
            ["Who pays?"]
        );
        assert_eq!(
            unanswered_questions(&json!({"kind": "question", "body": "Which DB?"})),
            ["Which DB?"]
        );
        assert_eq!(
            unanswered_questions(&json!({"kind": "research_gap", "body": " "})),
            ["research_gap"]
        );
        assert!(unanswered_questions(&json!({"kind": "insight", "body": "hot keys"})).is_empty());
        assert!(
            unanswered_questions(&json!({"kind": "knowledge", "open_questions": []})).is_empty()
        );
        assert!(!is_open_question("hypothesis"));
    }
}
