//! Decision-layer id newtypes.
//!
//! The shared crate (`daruma-shared::ids`) only mints ids for core
//! entities, and `vendor/oss` is **read-only** — so the ids the Decisions
//! layer owns are defined here, in its own zone. The shape (UUIDv7 +
//! human-readable prefix, `serde(transparent)`, `pln_<uuid>`-style Display)
//! is copied from the shared `newtype_id!` macro so a Decisions id reads and
//! parses exactly like a core one. The macro itself is not re-exported by
//! the shared crate, hence the small local re-derivation rather than a
//! `use`.

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! decision_id {
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

            #[inline]
            pub const fn prefix() -> &'static str {
                $prefix
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

decision_id!(DecisionId, "dec");
decision_id!(DirectiveId, "dir");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_round_trips_through_from_str() {
        let id = DecisionId::new();
        let shown = id.to_string();
        assert!(shown.starts_with("dec_"), "got {shown}");
        let back: DecisionId = shown.parse().unwrap();
        assert_eq!(id, back);
    }
}
