use thiserror::Error;

use hardwarepool_core::CoreError;

/// Wire decoding and semantic conversion errors.
#[derive(Debug, Error)]
pub enum ProtocolError {
    #[error("protobuf decode failed: {0}")]
    Decode(#[from] prost::DecodeError),

    #[error("invalid UUID in {field}: {value}")]
    InvalidId { field: &'static str, value: String },

    #[error("missing required semantic field: {0}")]
    MissingField(&'static str),

    #[error("invalid enum value {value} for {field}")]
    InvalidEnum { field: &'static str, value: i32 },

    #[error("protocol major {actual} is not supported; expected {expected}")]
    UnsupportedProtocolMajor { expected: u32, actual: u32 },

    #[error("control envelope is {actual} bytes; maximum is {limit}")]
    MessageTooLarge { limit: usize, actual: usize },

    #[error("numeric value in {field} is out of range: {value}")]
    NumericRange { field: &'static str, value: u64 },

    #[error(transparent)]
    Core(#[from] CoreError),
}
