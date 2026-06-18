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
//! - These are pure domain primitives: no I/O, no async. This crate has no
//!   dependency on taskagent or sibling `*_oss` layers.
//! - All JSON is serde-derived; ids, `Timestamp`, and `Actor` are local
//!   primitives — mcpbox maps them to/from taskagent types when wiring.
//! - Adapters from sensemaking (`draft_from_sensing`) live in mcpbox, not
//!   here, to keep this crate free of sibling-layer dependencies.

pub mod actor;
pub mod decision;
pub mod directive;
pub mod id;
pub mod link;
pub mod shared_ids;
pub mod time;

pub use actor::{Actor, ActorKind};
pub use decision::{Alternative, Decision, DecisionError, NewDecision};
pub use directive::{Directive, DirectiveKind, NewDirective};
pub use id::{DecisionId, DirectiveId};
pub use link::Link;
pub use shared_ids::{PlanId, ProjectId, TaskId};
pub use time::Timestamp;
