use thiserror::Error;

use crate::{
    AdapterInstanceId, CapabilityId, InteroperabilityMode, NodeId, PortDirection, PortId, PortRef,
    ProfileId, RouteBackend, RouteId, RouteState, SessionId, SessionState,
};

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum CoreError {
    #[error("invalid Node: {0}")]
    InvalidNode(String),
    #[error("invalid Adapter {adapter_id}: {reason}")]
    InvalidAdapter {
        adapter_id: AdapterInstanceId,
        reason: String,
    },
    #[error("invalid Capability {capability_id}: {reason}")]
    InvalidCapability {
        capability_id: CapabilityId,
        reason: String,
    },
    #[error("invalid Port {port_id}: {reason}")]
    InvalidPort { port_id: PortId, reason: String },
    #[error("invalid Profile: {0}")]
    InvalidProfile(String),
    #[error("invalid format: {0}")]
    InvalidFormat(String),
    #[error("invalid Problem: {0}")]
    InvalidProblem(String),
    #[error("duplicate Adapter {0}")]
    DuplicateAdapter(AdapterInstanceId),
    #[error("duplicate Capability {0}")]
    DuplicateCapability(CapabilityId),
    #[error("duplicate Port {0}")]
    DuplicatePort(PortId),
    #[error("unknown Adapter {0}")]
    UnknownAdapter(AdapterInstanceId),
    #[error("unknown Capability {0}")]
    UnknownCapability(CapabilityId),
    #[error("unknown Port {0}")]
    UnknownPort(PortId),
    #[error("unknown Route {0}")]
    UnknownRoute(RouteId),
    #[error("unknown Session {0}")]
    UnknownSession(SessionId),
    #[error("Port reference does not match its descriptor: {0:?}")]
    InvalidPortRef(PortRef),
    #[error("Route endpoint {port_id} must be {expected:?}, got {actual:?}")]
    InvalidRouteEndpoint {
        port_id: PortId,
        expected: PortDirection,
        actual: PortDirection,
    },
    #[error("incompatible Profiles: {source_profile:?} -> {sink_profile:?}")]
    IncompatibleProfiles {
        source_profile: ProfileId,
        sink_profile: ProfileId,
    },
    #[error("Source and Sink interoperability modes differ")]
    IncompatibleInteroperabilityModes,
    #[error("Route backend {backend:?} is incompatible with Port interoperability mode {mode:?}")]
    BackendInteroperabilityMismatch {
        backend: RouteBackend,
        mode: InteroperabilityMode,
    },
    #[error("Adapter {adapter_id} does not support Route backend {backend:?}")]
    UnsupportedRouteBackend {
        adapter_id: AdapterInstanceId,
        backend: RouteBackend,
    },
    #[error("Adapter {adapter_id} does not own Route endpoint Capability {capability_id}")]
    AdapterDoesNotOwnRouteEndpoint {
        adapter_id: AdapterInstanceId,
        capability_id: CapabilityId,
    },
    #[error("LocalPipeline requires both endpoints on one Node, got {source_node} -> {sink_node}")]
    LocalPipelineRequiresSameNode {
        source_node: NodeId,
        sink_node: NodeId,
    },
    #[error("Route Profile changed from {route_profile:?} to {endpoint_profile:?}")]
    RouteProfileChanged {
        route_profile: ProfileId,
        endpoint_profile: ProfileId,
    },
    #[error("Source and Sink have no compatible format")]
    NoCompatibleFormat,
    #[error("Source and Sink have no compatible QoS mode")]
    NoCompatibleQos,
    #[error("selected Route format was not mutually advertised")]
    UnsupportedRouteFormat,
    #[error("selected Route QoS was not mutually advertised")]
    UnsupportedRouteQos,
    #[error("Route authorization transition is invalid")]
    InvalidAuthorizationTransition,
    #[error("Route is not authorized")]
    RouteNotAuthorized,
    #[error("Route authorization has expired")]
    AuthorizationExpired,
    #[error("Route transition '{action}' is not valid from {from:?}")]
    InvalidRouteTransition {
        from: RouteState,
        action: &'static str,
    },
    #[error("Session transition '{action}' is not valid from {from:?}")]
    InvalidSessionTransition {
        from: SessionState,
        action: &'static str,
    },
}
