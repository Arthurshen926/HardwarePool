#![forbid(unsafe_code)]

//! Adapter manifests and the sidecar control-plane contract.

mod manifest;
mod rpc;

pub use manifest::{
    AdapterKind, AdapterManifest, CapabilityTemplate, ControlProtocolVersion, DeploymentMode,
    LicenseMetadata, ManifestError, PortTemplate,
};
pub use rpc::{
    AdapterCatalog, InitializeParams, InitializeResult, MAX_NDJSON_LINE_BYTES,
    MAX_PENDING_REQUESTS, ProbeResult, ResponseCorrelator, RouteParams, RouteStatusResult,
    RpcError, RpcFailure, RpcRequest, RpcResponse, SmokeSample, decode_request_line,
    decode_response_line, encode_request_line, encode_response_line,
};

pub const ADAPTER_MANIFEST_SCHEMA_VERSION: u16 = 1;
pub const ADAPTER_CONTROL_PROTOCOL_MAJOR: u16 = 1;
pub const ADAPTER_CONTROL_PROTOCOL_MINOR: u16 = 0;
