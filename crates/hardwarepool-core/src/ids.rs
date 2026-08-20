use std::{fmt, str::FromStr};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

macro_rules! define_id {
    ($name:ident, $description:literal) => {
        #[doc = $description]
        #[derive(
            Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            /// Creates a new random version-4 identifier.
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            /// Wraps an existing UUID without changing it.
            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

            /// Returns the underlying UUID.
            #[must_use]
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.0.fmt(formatter)
            }
        }

        impl FromStr for $name {
            type Err = uuid::Error;

            fn from_str(value: &str) -> Result<Self, Self::Err> {
                Uuid::parse_str(value).map(Self)
            }
        }

        impl From<Uuid> for $name {
            fn from(value: Uuid) -> Self {
                Self(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

define_id!(NodeId, "Stable identity of a HardwarePool node.");
define_id!(
    CapabilityId,
    "Identity of one independently authorized capability."
);
define_id!(SessionId, "Identity of one peer control session.");
define_id!(BindingId, "Identity of one per-capability session binding.");
define_id!(
    ProjectionId,
    "Identity of a local projection of a remote capability."
);
define_id!(StreamId, "Identity of one media or data stream.");
define_id!(MessageId, "Identity used to correlate protocol messages.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_ids_round_trip_as_strings() {
        let original = NodeId::new();
        let parsed: NodeId = original.to_string().parse().expect("valid UUID");
        assert_eq!(original, parsed);
    }
}
