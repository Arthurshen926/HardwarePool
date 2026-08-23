use capyio_core::{
    AdapterHealth, AdapterInstanceId, AdapterState, NodeId, ProblemId, RouteId, RouteState,
    SessionId, SessionState,
};
use serde::{Deserialize, Serialize};

use crate::{OperationId, OperationStatus};

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
    SessionStateChanged {
        session_id: SessionId,
        state: SessionState,
    },
    CatalogChanged {
        node_id: NodeId,
        adapter_id: AdapterInstanceId,
    },
    AdapterChanged {
        node_id: NodeId,
        adapter_id: AdapterInstanceId,
        state: AdapterState,
        health: AdapterHealth,
    },
    RouteChanged {
        route_id: RouteId,
        state: RouteState,
    },
    ProblemReported {
        problem_id: ProblemId,
    },
    OperationChanged {
        operation_id: OperationId,
        status: OperationStatus,
    },
}
