#![forbid(unsafe_code)]

//! Adapter manifests and the sidecar control-plane contract.

mod manifest;
mod rpc;

pub use manifest::{
    AdapterKind, AdapterManifest, CapabilityTemplate, ControlProtocolVersion, DeploymentBindings,
    DeploymentMode, DriverBackedDeployment, DriverBackedPlatformBinding, DriverDependencyMetadata,
    ExternalServiceConnection, ExternalServiceConnectionKind, ExternalServiceDeployment,
    ExternalServicePlatformBinding, ExternalServiceProbe, ExternalServiceProbeKind,
    InProcessDeployment, InProcessPlatformBinding, LicenseMetadata, ManifestError, PortTemplate,
    SidecarDeployment, UserModeControllerBinding,
};
pub use rpc::{
    AdapterCatalog, AdapterProblemDescriptor, BoundedJsonObject, DataEndpointDescriptor,
    InitializeParams, InitializeResult, MAX_ADAPTER_CONFIG_BYTES, MAX_ADAPTER_CONFIG_ENTRIES,
    MAX_DATA_ENDPOINT_METADATA_ENTRIES, MAX_NDJSON_LINE_BYTES, MAX_PENDING_REQUESTS,
    MAX_ROUTE_WARNINGS, ProbeResult, ResponseCorrelator, RoutePrepareRequest, RoutePrepareResult,
    RouteStartRequest, RouteStartResult, RouteStatusRequest, RouteStatusResult, RouteStopRequest,
    RouteStopResult, RpcError, RpcFailure, RpcRequest, RpcResponse, decode_request_line,
    decode_response_line, encode_request_line, encode_response_line,
};

pub const ADAPTER_MANIFEST_SCHEMA_VERSION: u16 = 2;
pub const ADAPTER_CONTROL_PROTOCOL_MAJOR: u16 = 1;
pub const ADAPTER_CONTROL_PROTOCOL_MINOR: u16 = 0;
