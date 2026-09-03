#![forbid(unsafe_code)]

//! Reserved Adapter boundary. No upstream source, process, network, or codec is present.

/// Makes the intentionally empty foundation state machine-readable to tests and inventories.
pub const IMPLEMENTATION_STATUS: &str = "planned-no-upstream-source";
