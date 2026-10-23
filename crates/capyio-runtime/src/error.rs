use capyio_core::{AdapterInstanceId, CoreError, NodeId, PortId, PortRef, RouteId, SessionId};
use thiserror::Error;

use crate::OperationId;
use crate::PortLeaseError;

#[derive(Debug, Error)]
pub enum RuntimeError {
    #[error("replacement local catalog Node identity does not match this Runtime")]
    LocalCatalogNodeMismatch,
    #[error("peer {0} is unknown")]
    UnknownPeer(NodeId),
    #[error("peer {0} is offline")]
    PeerOffline(NodeId),
    #[error("session {0} is unknown")]
    UnknownSession(SessionId),
    #[error("session {0} already exists")]
    DuplicateSession(SessionId),
    #[error(
        "catalog Node {catalog_node_id} does not match Session {session_id} peer {expected_peer_id}"
    )]
    PeerCatalogNodeMismatch {
        session_id: SessionId,
        expected_peer_id: NodeId,
        catalog_node_id: NodeId,
    },
    #[error("Route {0} is unknown")]
    UnknownRoute(RouteId),
    #[error("Route {0} authorization was terminally retired; create a new Route")]
    RouteAuthorizationRetired(RouteId),
    #[error("Route {0} has no local endpoint whose authorization can be retired")]
    RouteAuthorizationNotLocal(RouteId),
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
    #[error("Route {route_id} does not use local lease Port {port:?}")]
    LeasePortNotOnRoute { route_id: RouteId, port: PortRef },
    #[error("Route {0} retains a lease and cannot restrict its Draft authorization")]
    DraftAuthorizationLeasePresent(RouteId),
    #[error("Route {0} must be authorized before a lease is acquired")]
    LeaseAuthorizationRequired(RouteId),
    #[error("Route {route_id} lease expiry exceeds its authorization")]
    LeaseExceedsAuthorization { route_id: RouteId },
    #[error("Route {route_id} lease Session mismatch: expected {expected}, got {actual}")]
    LeaseSessionMismatch {
        route_id: RouteId,
        expected: SessionId,
        actual: SessionId,
    },
    #[error("Route {route_id} has no current claim for Port {port:?}")]
    LeaseClaimRequired { route_id: RouteId, port: PortRef },
    #[error(transparent)]
    Lease(#[from] PortLeaseError),
    #[error(transparent)]
    Core(#[from] CoreError),
}
