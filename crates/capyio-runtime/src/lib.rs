#![forbid(unsafe_code)]

//! Deterministic CapyIO catalog, Route and operation orchestration.

mod error;
mod event;
mod lease;
mod operation;
mod runtime;
mod snapshot;

pub use error::RuntimeError;
pub use event::{RuntimeEvent, RuntimeEventKind};
pub use lease::{
    LeaseClaimOutcome, MAX_PORT_LEASE_CLAIMS, MAX_PORT_LEASE_POLICIES, PortLeaseClaim,
    PortLeaseError, PortLeasePolicy, PortLeaseRegistry,
};
pub use operation::{
    DEFAULT_MAX_PENDING_OPERATIONS, DEFAULT_MAX_RETAINED_TERMINAL_OPERATIONS, HostOperation,
    HostOperationCompletion, HostOperationFailure, HostOperationFailureCode, HostOperationOutput,
    OperationId, OperationRecord, OperationRegistry, OperationStatus, OperationUpdate,
};
pub use runtime::NodeRuntime;
pub use snapshot::RuntimeSnapshot;
