use thiserror::Error;

/// Validation and bounded-buffer errors in the shared audio data plane.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AudioDataError {
    #[error("audio frame sample count must be greater than zero")]
    EmptyFrame,

    #[error("audio frame payload is {actual} bytes; expected {expected}")]
    PayloadLength { expected: usize, actual: usize },

    #[error("audio frame size arithmetic overflowed")]
    SizeOverflow,

    #[error("audio frame exceeds bootstrap payload limit of {limit} bytes")]
    PayloadTooLarge { limit: usize },

    #[error("reorder-buffer capacity must be greater than zero")]
    ZeroCapacity,
}
