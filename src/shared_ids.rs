//! Local newtypes for cross-layer reference ids.
//!
//! `Link` variants point to entities owned by sibling layers (Planning,
//! Actions).  Rather than pulling in `daruma-shared`, we mint minimal
//! UUIDv7 newtypes here that carry the same prefix convention.  the host maps
//! to/from the real daruma ids when wiring — the underlying `Uuid` is
//! identical, so the conversion is a no-op cast.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use uuid::Uuid;

macro_rules! shared_id {
    ($name:ident, $prefix:literal) => {
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub Uuid);

        impl $name {
            #[inline]
            pub fn new() -> Self {
                Self(Uuid::now_v7())
            }

            #[inline]
            pub fn from_uuid(uuid: Uuid) -> Self {
                Self(uuid)
            }

            #[inline]
            pub fn as_uuid(&self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}_{}", $prefix, self.0)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let trimmed = s.strip_prefix(concat!($prefix, "_")).unwrap_or(s);
                Ok(Self(Uuid::parse_str(trimmed)?))
            }
        }
    };
}

shared_id!(PlanId, "pln");
shared_id!(ProjectId, "prj");
shared_id!(TaskId, "tsk");

#[cfg(test)]
mod tests {
    use super::*;

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
