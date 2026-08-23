#![forbid(unsafe_code)]

//! Portable deterministic domain model for CapyIO.
//!
//! Core knows Nodes, Adapter instances, Capabilities, typed Ports, directed
//! Routes, Sessions and Problems. It intentionally performs no I/O and contains
//! no platform, transport, codec, UI or generated protocol dependency.

pub mod capability;
pub mod error;
pub mod ids;
pub mod problem;
pub mod route;
pub mod session;

pub use capability::{
    AdapterDeploymentMode, AdapterHealth, AdapterInstanceDescriptor, AdapterState, Availability,
    CapabilityClass, CapabilityDescriptor, FormatDescriptor, InteroperabilityMode, NodeDescriptor,
    OnlineState, PermissionRequirement, Platform, PortDescriptor, PortDirection, ProfileId,
    ProtocolVersion, QosMode,
};
pub use error::CoreError;
pub use ids::{
    AdapterInstanceId, CapabilityId, MessageId, NodeId, PortId, ProblemId, RouteId, SessionId,
    StreamId,
};
pub use problem::{Problem, ProblemCategory, ProblemSeverity};
pub use route::{AuthorizationState, PortRef, Route, RouteBackend, RouteState};
pub use session::{Session, SessionState};
