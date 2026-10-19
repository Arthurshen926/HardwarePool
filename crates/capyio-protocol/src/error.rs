use thiserror::Error;

use capyio_core::CoreError;

/// Wire decoding and semantic conversion errors.
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("protobuf decode failed: {0}")]
    Decode(#[from] prost::DecodeError),

    #[error("invalid UUID in {field}: {value}")]
    InvalidId { field: &'static str, value: String },

    #[error("missing required semantic field: {0}")]
    MissingField(&'static str),

    #[error("unexpected control payload; expected {expected}")]
    UnexpectedPayload { expected: &'static str },

    #[error("invalid enum value {value} for {field}")]
    InvalidEnum { field: &'static str, value: i32 },

    #[error("protocol major {actual} is not supported; expected {expected}")]
    UnsupportedProtocolMajor { expected: u32, actual: u32 },

    #[error("control envelope is {actual} bytes; maximum is {limit}")]
    MessageTooLarge { limit: usize, actual: usize },

    #[error("{field} has {actual} entries; maximum is {limit}")]
    CatalogCountLimit {
        field: &'static str,
        limit: usize,
        actual: usize,
    },

    #[error("{field} is {actual} bytes; maximum is {limit}")]
    CatalogStringLimit {
        field: &'static str,
        limit: usize,
        actual: usize,
    },

    #[error("numeric value in {field} is out of range: {value}")]
    NumericRange { field: &'static str, value: u64 },

    #[error("invalid Port lease request: {0}")]
    InvalidPortLease(&'static str),
    #[error("invalid Route control message: {0}")]
    InvalidRouteControl(&'static str),
    #[error("control heartbeat sequence must be non-zero")]
    InvalidControlHeartbeat,

    #[error("wire catalog ownership for Adapter {adapter_id} does not match its Capabilities")]
    CatalogOwnershipMismatch { adapter_id: String },

    #[error(transparent)]
    Core(#[from] CoreError),
}
