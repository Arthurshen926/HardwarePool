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
            #[must_use]
            pub fn new() -> Self {
                Self(Uuid::new_v4())
            }

            #[must_use]
            pub const fn from_uuid(value: Uuid) -> Self {
                Self(value)
            }

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

define_id!(NodeId, "Stable identity of one logical CapyIO Node.");
define_id!(
    AdapterInstanceId,
    "Identity of one deployed Adapter instance."
);
define_id!(
    CapabilityId,
    "Identity of one user-understandable capability."
);
define_id!(
    PortId,
    "Identity of one typed endpoint owned by a Capability."
);
define_id!(RouteId, "Identity of one directed Source-to-Sink Route.");
define_id!(
    SessionId,
    "Identity of one peer trust/control relationship."
);
define_id!(ProblemId, "Identity of one structured diagnostic.");
define_id!(StreamId, "Identity of one data-plane stream.");
define_id!(MessageId, "Identity used to correlate protocol messages.");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typed_ids_round_trip_without_becoming_interchangeable() {
        let original = PortId::new();
        let parsed: PortId = original.to_string().parse().expect("valid UUID");
        assert_eq!(original, parsed);
    }
}
