//! `decisions-oss` — the Decisions layer of the MCPBox family.
//!
//! In the MCPBox pipeline (Intake → Sensemaking → **Decisions** → Planning →
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
//! - These are pure domain primitives: no I/O, no async. Unlike its
//!   AI-operation siblings (`intake-oss`, `planning-oss`, `sensemaking-oss`)
//!   this crate has no dependency on `taskagent-ai-infra`.
//! - All JSON is serde-derived; ids and `Timestamp` come from the shared
//!   crate (consumed read-only via the `vendor/oss` symlink), and the
//!   [`Actor`](taskagent_domain::Actor) type is reused rather than redefined.

pub mod decision;
pub mod directive;
pub mod id;
pub mod link;

pub use decision::{Alternative, Decision, DecisionError, NewDecision};
pub use directive::{Directive, DirectiveKind, NewDirective};
pub use id::{DecisionId, DirectiveId};
pub use link::Link;

// Re-export the shared/domain types that appear in this layer's public
// surface, so callers depend on `decisions_oss::*` without also naming the
// vendored crates.
pub use taskagent_domain::Actor;
pub use taskagent_shared::{time, PlanId, ProjectId, TaskId, Timestamp};
