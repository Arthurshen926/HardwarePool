use std::collections::BTreeSet;

use capyio_core::{CapabilityDescriptor, RouteId};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::Value;
use thiserror::Error;

use crate::ControlProtocolVersion;

pub const MAX_NDJSON_LINE_BYTES: usize = 64 * 1024;
pub const MAX_PENDING_REQUESTS: usize = 64;

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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteParams {
    pub route_id: RouteId,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RouteStatusResult {
    pub route_id: RouteId,
    pub state: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SmokeSample {
    pub test_only: bool,
    pub sequence: u64,
    pub payload: String,
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
}
