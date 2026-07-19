//! Decision-layer id newtypes.
//!
//! The shared crate (`daruma-shared::ids`) only mints ids for core
//! entities, and `vendor/oss` is **read-only** — so every id this layer
//! needs is minted here, in its own zone: its own [`DecisionId`] /
//! [`DirectiveId`], plus the cross-layer reference ids [`PlanId`] /
//! [`ProjectId`] / [`TaskId`] it links against (formerly a separate
//! `shared_ids` module — merged in here since both were the same shape).
//! The shape (UUIDv7 + human-readable prefix, `serde(transparent)`,
//! `<prefix>_<uuid>`-style `Display`) comes from layer-kit's shared
//! `newtype_id!` macro, so a Decisions id reads and parses exactly like a
//! core one. the host casts between them and the real daruma ids when
//! wiring — the underlying `Uuid` is identical.

layer_kit::newtype_id! {
    /// Strongly-typed id for a [`crate::Decision`].
    pub struct DecisionId("dec");
}

layer_kit::newtype_id! {
    /// Strongly-typed id for a [`crate::Directive`].
    pub struct DirectiveId("dir");
}

// ── Cross-layer reference ids ───────────────────────────────────────────
//
// `Link` variants point to entities owned by sibling layers (Planning,
// Actions). Rather than pulling in `daruma-shared`, we mint minimal
// UUIDv7 newtypes here that carry the same prefix convention.

layer_kit::newtype_id! {
    /// Strongly-typed id for a plan owned by the Planning layer.
    pub struct PlanId("pln");
}

layer_kit::newtype_id! {
    /// Strongly-typed id for a project.
    pub struct ProjectId("prj");
}

layer_kit::newtype_id! {
    /// Strongly-typed id for a task owned by the Actions layer.
    pub struct TaskId("tsk");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decision_id_display_round_trips_through_from_str() {
        let id = DecisionId::new();
        let shown = id.to_string();
        assert!(shown.starts_with("dec_"), "got {shown}");
        let back: DecisionId = shown.parse().unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn directive_id_display_round_trips_through_from_str() {
        let id = DirectiveId::new();
        let shown = id.to_string();
        assert!(shown.starts_with("dir_"), "got {shown}");
        let back: DirectiveId = shown.parse().unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn plan_id_display_and_parse_roundtrip() {
        let id = PlanId::new();
        let s = id.to_string();
        assert!(s.starts_with("pln_"), "got {s}");
        let back: PlanId = s.parse().unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn project_id_display_and_parse_roundtrip() {
        let id = ProjectId::new();
        let s = id.to_string();
        assert!(s.starts_with("prj_"), "got {s}");
        let back: ProjectId = s.parse().unwrap();
        assert_eq!(id, back);
    }

    #[test]
    fn task_id_display_and_parse_roundtrip() {
        let id = TaskId::new();
        let s = id.to_string();
        assert!(s.starts_with("tsk_"), "got {s}");
        let back: TaskId = s.parse().unwrap();
        assert_eq!(id, back);
    }
}
