//! Adapter: Sensemaking → Decisions.
//!
//! Converts a [`SensingItem`] into a draft [`NewDecision`] or [`NewDirective`],
//! recording provenance through [`Link::Sensemaking`].
//!
//! # Mapping
//! | `SensingItemKind` | Result |
//! |---|---|
//! | `Insight` | `NewDecision` (non-obvious pattern worth committing to) |
//! | `Hypothesis` | `NewDecision` (testable claim promoted to a draft choice) |
//! | `Knowledge` | `NewDirective { kind: Assumption }` (settled fact → assumption) |
//! | everything else | `None` (not yet decision-worthy) |
//!
//! The produced object always carries a `Link::Sensemaking { reference }` where
//! `reference` is the string representation of the source `SensingItemId` —
//! that is the entire point of the adapter: provenance is never optional.

use sensemaking_oss::types::{SensingItem, SensingItemKind};
use taskagent_domain::Actor;

use crate::decision::NewDecision;
use crate::directive::{DirectiveKind, NewDirective};
use crate::link::Link;

/// The two outcomes a sensing item can be promoted to.
pub enum SensingDraft {
    Decision(NewDecision),
    Directive(NewDirective),
}

/// Attempt to promote a [`SensingItem`] into a draft decision or directive.
///
/// Returns `None` for kinds that are not (yet) decision-worthy:
/// `Question`, `Risk`, `Contradiction`, `RejectedIdea`, `ResearchGap`.
///
/// The caller supplies `decided_by` / `set_by` — the agent or person who is
/// promoting the sensing item, not necessarily whoever created it.
pub fn draft_from_sensing(item: &SensingItem, actor: Actor) -> Option<SensingDraft> {
    // Provenance link — carried by every produced object.
    let provenance = Link::Sensemaking {
        reference: item.id.to_string(),
    };

    match item.kind {
        SensingItemKind::Insight | SensingItemKind::Hypothesis => {
            Some(SensingDraft::Decision(NewDecision {
                id: None,
                statement: item.body.clone(),
                decided_by: actor,
                decided_at: None,
                // Body of the sensing item becomes the rationale; the caller
                // can refine the statement before materialising.
                rationale: format!(
                    "Promoted from sensemaking {} ({})",
                    item.kind, item.id
                ),
                alternatives: Vec::new(),
                consequences: Vec::new(),
                revisit_when: String::new(),
                links: vec![provenance],
            }))
        }
        SensingItemKind::Knowledge => Some(SensingDraft::Directive(NewDirective {
            id: None,
            kind: DirectiveKind::Assumption,
            statement: item.body.clone(),
            set_by: actor,
            rationale: format!(
                "Promoted from sensemaking {} ({})",
                item.kind, item.id
            ),
            links: vec![provenance],
        })),
        // Not yet decision-worthy.
        SensingItemKind::Question
        | SensingItemKind::Risk
        | SensingItemKind::Contradiction
        | SensingItemKind::RejectedIdea
        | SensingItemKind::ResearchGap => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sensemaking_oss::types::{SensingItem, SensingItemKind};
    use taskagent_domain::Actor;

    /// The id of the source SensingItem must be recoverable from the provenance
    /// link — that is the entire contract of this adapter.
    #[test]
    fn insight_provenance_roundtrips() {
        let item = SensingItem::new(SensingItemKind::Insight, "Cache reads dominate write latency");
        let item_id = item.id.to_string();

        let draft = draft_from_sensing(&item, Actor::user()).expect("insight maps to a decision");
        let SensingDraft::Decision(nd) = draft else {
            panic!("expected Decision");
        };

        let link = nd.links.iter().find(|l| matches!(l, Link::Sensemaking { .. }));
        let Some(Link::Sensemaking { reference }) = link else {
            panic!("provenance link missing");
        };
        assert_eq!(reference, &item_id, "provenance must point back to the source SensingItem");
    }

    #[test]
    fn hypothesis_maps_to_decision() {
        let item = SensingItem::new(SensingItemKind::Hypothesis, "Batching reduces p99 by 30%");
        let draft = draft_from_sensing(&item, Actor::user()).expect("hypothesis maps");
        assert!(matches!(draft, SensingDraft::Decision(_)));
    }

    #[test]
    fn knowledge_maps_to_assumption_directive() {
        let item = SensingItem::new(SensingItemKind::Knowledge, "Users are on mobile-first");
        let item_id = item.id.to_string();

        let draft = draft_from_sensing(&item, Actor::user()).expect("knowledge maps");
        let SensingDraft::Directive(nd) = draft else {
            panic!("expected Directive");
        };
        assert_eq!(nd.kind, DirectiveKind::Assumption);

        let link = nd.links.iter().find(|l| matches!(l, Link::Sensemaking { .. }));
        let Some(Link::Sensemaking { reference }) = link else {
            panic!("provenance link missing");
        };
        assert_eq!(reference, &item_id);
    }

    #[test]
    fn non_actionable_kinds_return_none() {
        for kind in [
            SensingItemKind::Question,
            SensingItemKind::Risk,
            SensingItemKind::Contradiction,
            SensingItemKind::ResearchGap,
        ] {
            let item = SensingItem::new(kind, "some body");
            assert!(
                draft_from_sensing(&item, Actor::user()).is_none(),
                "{kind} should not produce a draft"
            );
        }
    }
}
