use thiserror::Error;

/// Validation and bounded-buffer errors in the shared audio data plane.
#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum AudioDataError {
    #[error("invalid audio format: {0}")]
    InvalidFormat(String),

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

    #[error("invalid audio stream specification: {0}")]
    InvalidStreamSpec(String),

    #[error("audio endpoint must advertise at least one stream candidate")]
    EmptyStreamCandidates,

    #[error("audio endpoint advertises {actual} candidates; limit is {limit}")]
    TooManyStreamCandidates { actual: usize, limit: usize },

    #[error("audio endpoint advertises the same stream candidate more than once")]
    DuplicateStreamCandidate,

    #[error("source or sink does not support the requested audio use case")]
    UnsupportedAudioUseCase,

    #[error("source and sink have no compatible audio stream candidate")]
    NoCompatibleAudioStream,
}
