use std::collections::{BTreeMap, BTreeSet};

use capyio_core::{
    CapabilityDescriptor, FormatDescriptor, PortRef, ProfileId, QosMode, RouteBackend, RouteId,
    RouteState,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;

use crate::ControlProtocolVersion;

pub const MAX_NDJSON_LINE_BYTES: usize = 64 * 1024;
pub const MAX_PENDING_REQUESTS: usize = 64;
pub const MAX_ADAPTER_CONFIG_BYTES: usize = 8 * 1024;
pub const MAX_ADAPTER_CONFIG_ENTRIES: usize = 32;
pub const MAX_DATA_ENDPOINT_METADATA_ENTRIES: usize = 32;
pub const MAX_ROUTE_WARNINGS: usize = 16;

const MAX_CONTRACT_STRING_BYTES: usize = 2 * 1024;
const MAX_JSON_DEPTH: usize = 8;
const MAX_JSON_NODES: usize = 256;
const MAX_FORMAT_PARAMETERS: usize = 32;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl RpcRequest {
    pub fn new<T: Serialize>(
        id: u64,
        method: impl Into<String>,
        params: &T,
    ) -> Result<Self, RpcError> {
        if id == 0 {
            return Err(RpcError::InvalidCorrelationId);
        }
        let method = method.into();
        if method.trim().is_empty() {
            return Err(RpcError::EmptyMethod);
        }
        Ok(Self {
            jsonrpc: "2.0".to_owned(),
            id,
            method,
            params: serde_json::to_value(params)?,
        })
    }

    pub fn decode_params<T: DeserializeOwned>(&self) -> Result<T, RpcError> {
        Ok(serde_json::from_value(self.params.clone())?)
    }

    fn validate(&self) -> Result<(), RpcError> {
        if self.jsonrpc != "2.0" {
            return Err(RpcError::UnsupportedJsonRpcVersion(self.jsonrpc.clone()));
        }
        if self.id == 0 {
            return Err(RpcError::InvalidCorrelationId);
        }
        if self.method.trim().is_empty() {
            return Err(RpcError::EmptyMethod);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RpcFailure {
    pub code: String,
    pub message: String,
    pub retryable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcFailure>,
}

impl RpcResponse {
    pub fn success<T: Serialize>(id: u64, result: &T) -> Result<Self, RpcError> {
        let response = Self {
            jsonrpc: "2.0".to_owned(),
            id,
            result: Some(serde_json::to_value(result)?),
            error: None,
        };
        response.validate()?;
        Ok(response)
    }

    pub fn failure(id: u64, error: RpcFailure) -> Result<Self, RpcError> {
        let response = Self {
            jsonrpc: "2.0".to_owned(),
            id,
            result: None,
            error: Some(error),
        };
        response.validate()?;
        Ok(response)
    }

    pub fn decode_result<T: DeserializeOwned>(&self) -> Result<T, RpcError> {
        if let Some(error) = &self.error {
            return Err(RpcError::Remote {
                code: error.code.clone(),
                message: error.message.clone(),
                retryable: error.retryable,
            });
        }
        let result = self.result.clone().ok_or(RpcError::InvalidResponseShape)?;
        Ok(serde_json::from_value(result)?)
    }

    fn validate(&self) -> Result<(), RpcError> {
        if self.jsonrpc != "2.0" {
            return Err(RpcError::UnsupportedJsonRpcVersion(self.jsonrpc.clone()));
        }
        if self.id == 0 {
            return Err(RpcError::InvalidCorrelationId);
        }
        if self.result.is_some() == self.error.is_some() {
            return Err(RpcError::InvalidResponseShape);
        }
        Ok(())
    }
}

pub fn encode_request_line(request: &RpcRequest) -> Result<Vec<u8>, RpcError> {
    request.validate()?;
    encode_line(request)
}

pub fn decode_request_line(line: &[u8]) -> Result<RpcRequest, RpcError> {
    let request: RpcRequest = decode_line(line)?;
    request.validate()?;
    Ok(request)
}

pub fn encode_response_line(response: &RpcResponse) -> Result<Vec<u8>, RpcError> {
    response.validate()?;
    encode_line(response)
}

pub fn decode_response_line(line: &[u8]) -> Result<RpcResponse, RpcError> {
    let response: RpcResponse = decode_line(line)?;
    response.validate()?;
    Ok(response)
}

fn encode_line<T: Serialize>(value: &T) -> Result<Vec<u8>, RpcError> {
    let mut bytes = serde_json::to_vec(value)?;
    if bytes.len() + 1 > MAX_NDJSON_LINE_BYTES {
        return Err(RpcError::LineTooLarge {
            actual: bytes.len() + 1,
            limit: MAX_NDJSON_LINE_BYTES,
        });
    }
    bytes.push(b'\n');
    Ok(bytes)
}

fn decode_line<T: DeserializeOwned>(line: &[u8]) -> Result<T, RpcError> {
    if line.is_empty() || line == b"\n" || line == b"\r\n" {
        return Err(RpcError::EmptyLine);
    }
    if line.len() > MAX_NDJSON_LINE_BYTES {
        return Err(RpcError::LineTooLarge {
            actual: line.len(),
            limit: MAX_NDJSON_LINE_BYTES,
        });
    }
    let body = line.strip_suffix(b"\n").unwrap_or(line);
    let body = body.strip_suffix(b"\r").unwrap_or(body);
    Ok(serde_json::from_slice(body)?)
}

#[derive(Clone, Debug, Default)]
pub struct ResponseCorrelator {
    pending: BTreeSet<u64>,
}

impl ResponseCorrelator {
    pub fn register(&mut self, id: u64) -> Result<(), RpcError> {
        if id == 0 {
            return Err(RpcError::InvalidCorrelationId);
        }
        if self.pending.len() >= MAX_PENDING_REQUESTS {
            return Err(RpcError::PendingLimitReached(MAX_PENDING_REQUESTS));
        }
        if !self.pending.insert(id) {
            return Err(RpcError::DuplicateCorrelationId(id));
        }
        Ok(())
    }

    pub fn resolve(&mut self, response: &RpcResponse) -> Result<(), RpcError> {
        if self.pending.remove(&response.id) {
            Ok(())
        } else {
            Err(RpcError::UnexpectedResponseId(response.id))
        }
    }

    pub fn abandon(&mut self, id: u64) {
        self.pending.remove(&id);
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InitializeParams {
    pub adapter_instance_id: String,
    pub control_protocol: ControlProtocolVersion,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct InitializeResult {
    pub adapter_id: String,
    pub adapter_name: String,
    pub control_protocol: ControlProtocolVersion,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProbeResult {
    pub ready: bool,
    pub detail: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterCatalog {
    pub adapter_id: String,
    pub capabilities: Vec<CapabilityDescriptor>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub struct BoundedJsonObject(pub BTreeMap<String, Value>);

impl BoundedJsonObject {
    pub fn validate(&self) -> Result<(), RpcError> {
        if self.0.len() > MAX_ADAPTER_CONFIG_ENTRIES {
            return Err(RpcError::ContractLimit("Adapter configuration entries"));
        }
        let encoded_bytes = serde_json::to_vec(&self.0)?.len();
        if encoded_bytes > MAX_ADAPTER_CONFIG_BYTES {
            return Err(RpcError::ContractLimit("Adapter configuration bytes"));
        }
        let mut nodes = 0;
        validate_json_object(&self.0, 0, &mut nodes)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DataEndpointDescriptor {
    pub transport: String,
    pub address: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

impl DataEndpointDescriptor {
    pub fn validate(&self) -> Result<(), RpcError> {
        validate_contract_string("data endpoint transport", &self.transport)?;
        validate_contract_string("data endpoint address", &self.address)?;
        if self.metadata.len() > MAX_DATA_ENDPOINT_METADATA_ENTRIES {
            return Err(RpcError::ContractLimit("data endpoint metadata entries"));
        }
        for (key, value) in &self.metadata {
            validate_contract_string("data endpoint metadata key", key)?;
            validate_contract_string("data endpoint metadata value", value)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AdapterProblemDescriptor {
    pub code: String,
    pub message: String,
    pub retryable: bool,
}

impl AdapterProblemDescriptor {
    fn validate(&self) -> Result<(), RpcError> {
        validate_contract_string("Adapter warning code", &self.code)?;
        validate_contract_string("Adapter warning message", &self.message)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutePrepareRequest {
    pub route_id: RouteId,
    pub source: PortRef,
    pub sink: PortRef,
    pub profile: ProfileId,
    pub selected_format: Option<FormatDescriptor>,
    pub selected_qos: QosMode,
    pub backend: RouteBackend,
    pub epoch: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_endpoint: Option<DataEndpointDescriptor>,
    #[serde(default)]
    pub adapter_config: BoundedJsonObject,
}

impl RoutePrepareRequest {
    pub fn validate(&self) -> Result<(), RpcError> {
        if self.epoch == 0 {
            return Err(RpcError::InvalidContract("Route epoch"));
        }
        self.profile
            .validate()
            .map_err(|_| RpcError::InvalidContract("Route Profile"))?;
        validate_contract_string("Route Profile name", &self.profile.name)?;
        if let Some(format) = &self.selected_format {
            format
                .validate()
                .map_err(|_| RpcError::InvalidContract("selected Route format"))?;
            validate_contract_string("selected Route format ID", &format.id)?;
            if format.parameters.len() > MAX_FORMAT_PARAMETERS {
                return Err(RpcError::ContractLimit("selected Route format parameters"));
            }
            for (key, value) in &format.parameters {
                validate_contract_string("selected Route format parameter key", key)?;
                validate_contract_string("selected Route format parameter value", value)?;
            }
        }
        if let QosMode::Custom(value) = &self.selected_qos {
            validate_contract_string("selected custom Route QoS", value)?;
        }
        if let Some(endpoint) = &self.data_endpoint {
            endpoint.validate()?;
        }
        self.adapter_config.validate()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RoutePrepareResult {
    pub accepted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_endpoint: Option<DataEndpointDescriptor>,
    #[serde(default)]
    pub warnings: Vec<AdapterProblemDescriptor>,
}

impl RoutePrepareResult {
    pub fn validate(&self) -> Result<(), RpcError> {
        validate_route_result(self.data_endpoint.as_ref(), &self.warnings)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteStartRequest {
    pub route_id: RouteId,
    pub epoch: u64,
}

impl RouteStartRequest {
    pub fn validate(&self) -> Result<(), RpcError> {
        validate_route_epoch(self.epoch)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteStartResult {
    pub accepted: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data_endpoint: Option<DataEndpointDescriptor>,
    #[serde(default)]
    pub warnings: Vec<AdapterProblemDescriptor>,
}

impl RouteStartResult {
    pub fn validate(&self) -> Result<(), RpcError> {
        validate_route_result(self.data_endpoint.as_ref(), &self.warnings)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteStopRequest {
    pub route_id: RouteId,
    pub epoch: u64,
}

impl RouteStopRequest {
    pub fn validate(&self) -> Result<(), RpcError> {
        validate_route_epoch(self.epoch)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteStopResult {
    pub accepted: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteStatusRequest {
    pub route_id: RouteId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteStatusResult {
    pub route_id: RouteId,
    pub epoch: u64,
    pub state: RouteState,
    #[serde(default)]
    pub warnings: Vec<AdapterProblemDescriptor>,
}

impl RouteStatusResult {
    pub fn validate(&self) -> Result<(), RpcError> {
        validate_route_result(None, &self.warnings)
    }
}

fn validate_route_result(
    endpoint: Option<&DataEndpointDescriptor>,
    warnings: &[AdapterProblemDescriptor],
) -> Result<(), RpcError> {
    if let Some(endpoint) = endpoint {
        endpoint.validate()?;
    }
    if warnings.len() > MAX_ROUTE_WARNINGS {
        return Err(RpcError::ContractLimit("Route warnings"));
    }
    for warning in warnings {
        warning.validate()?;
    }
    Ok(())
}

fn validate_json_object(
    object: &BTreeMap<String, Value>,
    depth: usize,
    nodes: &mut usize,
) -> Result<(), RpcError> {
    if depth > MAX_JSON_DEPTH {
        return Err(RpcError::ContractLimit("Adapter configuration JSON depth"));
    }
    if object.len() > MAX_ADAPTER_CONFIG_ENTRIES {
        return Err(RpcError::ContractLimit(
            "Adapter configuration object entries",
        ));
    }
    for (key, value) in object {
        validate_contract_string("Adapter configuration key", key)?;
        validate_json_value(value, depth + 1, nodes)?;
    }
    Ok(())
}

fn validate_json_value(value: &Value, depth: usize, nodes: &mut usize) -> Result<(), RpcError> {
    *nodes = nodes.saturating_add(1);
    if *nodes > MAX_JSON_NODES || depth > MAX_JSON_DEPTH {
        return Err(RpcError::ContractLimit(
            "Adapter configuration JSON nodes/depth",
        ));
    }
    match value {
        Value::String(value) => validate_contract_string("Adapter configuration string", value),
        Value::Array(values) => {
            if values.len() > MAX_ADAPTER_CONFIG_ENTRIES {
                return Err(RpcError::ContractLimit(
                    "Adapter configuration array entries",
                ));
            }
            for value in values {
                validate_json_value(value, depth + 1, nodes)?;
            }
            Ok(())
        }
        Value::Object(values) => {
            let values = values
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            validate_json_object(&values, depth, nodes)
        }
        Value::Null | Value::Bool(_) | Value::Number(_) => Ok(()),
    }
}

fn validate_contract_string(field: &'static str, value: &str) -> Result<(), RpcError> {
    if value.trim().is_empty() {
        return Err(RpcError::InvalidContract(field));
    }
    if value.len() > MAX_CONTRACT_STRING_BYTES {
        return Err(RpcError::ContractLimit(field));
    }
    Ok(())
}

fn validate_route_epoch(epoch: u64) -> Result<(), RpcError> {
    if epoch == 0 {
        Err(RpcError::InvalidContract("Route epoch"))
    } else {
        Ok(())
    }
}

#[derive(Debug, Error)]
pub enum RpcError {
    #[error("JSON encoding/decoding failed: {0}")]
    Json(#[from] serde_json::Error),
    #[error("NDJSON line is empty")]
    EmptyLine,
    #[error("NDJSON line is {actual} bytes; maximum is {limit}")]
    LineTooLarge { actual: usize, limit: usize },
    #[error("JSON-RPC version {0} is unsupported")]
    UnsupportedJsonRpcVersion(String),
    #[error("JSON-RPC correlation ID must be non-zero")]
    InvalidCorrelationId,
    #[error("JSON-RPC method cannot be empty")]
    EmptyMethod,
    #[error("Adapter control contract field is invalid: {0}")]
    InvalidContract(&'static str),
    #[error("Adapter control contract limit exceeded: {0}")]
    ContractLimit(&'static str),
    #[error("JSON-RPC response must contain exactly one of result or error")]
    InvalidResponseShape,
    #[error("pending JSON-RPC request limit {0} reached")]
    PendingLimitReached(usize),
    #[error("correlation ID {0} is already pending")]
    DuplicateCorrelationId(u64),
    #[error("response correlation ID {0} is not pending")]
    UnexpectedResponseId(u64),
    #[error("remote error {code}: {message} (retryable={retryable})")]
    Remote {
        code: String,
        message: String,
        retryable: bool,
    },
}

#[cfg(test)]
mod tests {
    use capyio_core::{CapabilityId, NodeId, PortId};

    use super::*;

    #[test]
    fn request_and_response_round_trip_as_one_line() {
        let request = RpcRequest::new(1, "adapter.probe", &()).expect("request");
        let bytes = encode_request_line(&request).expect("encode");
        assert_eq!(bytes.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert_eq!(decode_request_line(&bytes).expect("decode"), request);

        let response = RpcResponse::success(
            1,
            &ProbeResult {
                ready: true,
                detail: "mock".to_owned(),
            },
        )
        .expect("response");
        assert_eq!(
            decode_response_line(&encode_response_line(&response).expect("encode"))
                .expect("decode"),
            response
        );
    }

    #[test]
    fn malformed_and_oversized_lines_are_rejected() {
        assert!(matches!(
            decode_request_line(b"not json\n"),
            Err(RpcError::Json(_))
        ));
        assert!(matches!(
            decode_request_line(&vec![b'x'; MAX_NDJSON_LINE_BYTES + 1]),
            Err(RpcError::LineTooLarge { .. })
        ));
    }

    #[test]
    fn response_correlation_is_bounded_and_exact() {
        let mut correlator = ResponseCorrelator::default();
        correlator.register(7).expect("register");
        let unexpected = RpcResponse::success(8, &()).expect("response");
        assert!(matches!(
            correlator.resolve(&unexpected),
            Err(RpcError::UnexpectedResponseId(8))
        ));
        let expected = RpcResponse::success(7, &()).expect("response");
        correlator.resolve(&expected).expect("resolve");
        assert!(matches!(
            correlator.resolve(&expected),
            Err(RpcError::UnexpectedResponseId(7))
        ));
    }

    #[test]
    fn generic_route_contract_round_trips_without_payload_data() {
        let request = RoutePrepareRequest {
            route_id: RouteId::new(),
            source: PortRef {
                node_id: NodeId::new(),
                capability_id: CapabilityId::new(),
                port_id: PortId::new(),
            },
            sink: PortRef {
                node_id: NodeId::new(),
                capability_id: CapabilityId::new(),
                port_id: PortId::new(),
            },
            profile: ProfileId::new("capyio.test.samples", 1),
            selected_format: Some(FormatDescriptor::new("application/test")),
            selected_qos: QosMode::Basic,
            backend: RouteBackend::CapyDataPlane,
            epoch: 7,
            data_endpoint: Some(DataEndpointDescriptor {
                transport: "local_ipc".to_owned(),
                address: "capyio-test-endpoint".to_owned(),
                metadata: BTreeMap::from([("direction".to_owned(), "source".to_owned())]),
            }),
            adapter_config: BoundedJsonObject(BTreeMap::from([(
                "fixture".to_owned(),
                Value::Bool(true),
            )])),
        };
        request.validate().expect("bounded request");
        let rpc = RpcRequest::new(11, "route.prepare", &request).expect("request");
        let decoded = decode_request_line(&encode_request_line(&rpc).expect("encode"))
            .expect("decode")
            .decode_params::<RoutePrepareRequest>()
            .expect("Route contract");
        decoded.validate().expect("decoded bounds");
        assert_eq!(decoded, request);

        let result = RouteStartResult {
            accepted: true,
            data_endpoint: request.data_endpoint,
            warnings: vec![AdapterProblemDescriptor {
                code: "mock_notice".to_owned(),
                message: "finite test acknowledgement".to_owned(),
                retryable: false,
            }],
        };
        result.validate().expect("bounded result");
        let response = RpcResponse::success(11, &result).expect("response");
        let decoded = decode_response_line(&encode_response_line(&response).expect("encode"))
            .expect("decode")
            .decode_result::<RouteStartResult>()
            .expect("Route result");
        decoded.validate().expect("decoded result bounds");
        assert_eq!(decoded, result);
    }

    #[test]
    fn adapter_configuration_limits_are_explicit() {
        let config = BoundedJsonObject(BTreeMap::from([(
            "oversized".to_owned(),
            Value::String("x".repeat(MAX_CONTRACT_STRING_BYTES + 1)),
        )]));
        assert!(matches!(
            config.validate(),
            Err(RpcError::ContractLimit("Adapter configuration string"))
        ));
    }
}
