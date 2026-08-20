#![forbid(unsafe_code)]

//! Deterministic orchestration on top of `hardwarepool-core`.
//!
//! The bootstrap Runtime is intentionally synchronous. Future async hosts submit
//! completion events back into this deterministic state owner instead of mutating
//! sessions directly from arbitrary callback threads.

mod error;
mod event;
mod operation;
mod runtime;
mod snapshot;

pub use error::RuntimeError;
pub use event::{RuntimeEvent, RuntimeEventKind};
pub use operation::{
    ActualAudioStreamParameters, DEFAULT_MAX_PENDING_OPERATIONS,
    DEFAULT_MAX_RETAINED_TERMINAL_OPERATIONS, HostOperation, HostOperationCompletion,
    HostOperationFailure, HostOperationFailureCode, HostOperationOutput, OperationId,
    OperationRecord, OperationRegistry, OperationStatus, OperationUpdate,
};
pub use runtime::{NodeRuntime, PeerRecord};
pub use snapshot::{PeerSnapshot, RuntimeSnapshot};
