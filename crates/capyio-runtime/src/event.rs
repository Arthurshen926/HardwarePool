use capyio_core::{
    BindingState, CapabilityId, NodeId, ProjectionKind, SessionId, SessionPhase,
};
use serde::{Deserialize, Serialize};

use crate::{OperationId, OperationStatus};

/// Structured, sanitized Runtime event suitable for UI and diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeEvent {
    pub sequence: u64,
    pub kind: RuntimeEventKind,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RuntimeEventKind {
    PeerRegistered {
        peer_id: NodeId,
    },
    PeerOnlineChanged {
        peer_id: NodeId,
        online: bool,
    },
    SessionOpened {
        session_id: SessionId,
        peer_id: NodeId,
    },
    SessionPhaseChanged {
        session_id: SessionId,
        phase: SessionPhase,
    },
    BindingChanged {
        session_id: SessionId,
        capability_id: CapabilityId,
        projection_kind: ProjectionKind,
        state: BindingState,
    },
    OperationChanged {
        operation_id: OperationId,
        status: OperationStatus,
    },
}
