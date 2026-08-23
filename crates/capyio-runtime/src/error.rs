use capyio_core::{AdapterInstanceId, CoreError, NodeId, PortId, RouteId, SessionId};
use thiserror::Error;

use crate::OperationId;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("peer {0} is unknown")]
    UnknownPeer(NodeId),
    #[error("peer {0} is offline")]
    PeerOffline(NodeId),
    #[error("session {0} is unknown")]
    UnknownSession(SessionId),
    #[error("session {0} already exists")]
    DuplicateSession(SessionId),
    #[error("Route {0} is unknown")]
    UnknownRoute(RouteId),
    #[error("Route {0} already exists")]
    DuplicateRoute(RouteId),
    #[error("Adapter {adapter_id} is unknown on Node {node_id}")]
    UnknownAdapter {
        node_id: NodeId,
        adapter_id: AdapterInstanceId,
    },
    #[error("Port {port_id} is not available on Node {node_id}")]
    PortNotAdvertised { node_id: NodeId, port_id: PortId },
    #[error("operation limits must be greater than zero")]
    InvalidOperationLimits,
    #[error("pending operation limit {limit} reached")]
    PendingOperationLimitReached { limit: usize },
    #[error("operation identifier space is exhausted")]
    OperationIdExhausted,
    #[error("operation {0} is unknown or no longer retained")]
    UnknownOperation(OperationId),
    #[error("invalid operation completion: {0}")]
    InvalidOperationCompletion(&'static str),
    #[error(transparent)]
    Core(#[from] CoreError),
}
