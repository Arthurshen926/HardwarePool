use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum VideoContractError {
    #[error("invalid video stream specification: {0}")]
    InvalidStreamSpec(String),
    #[error("video endpoint must advertise at least one stream candidate")]
    EmptyStreamCandidates,
    #[error("video endpoint advertises {actual} candidates; limit is {limit}")]
    TooManyStreamCandidates { actual: usize, limit: usize },
    #[error("video endpoint advertises the same stream candidate more than once")]
    DuplicateStreamCandidate,
    #[error("source or sink does not support the requested video use case")]
    UnsupportedVideoUseCase,
    #[error("source and sink have no identical compatible video stream candidate")]
    NoCompatibleVideoStream,
    #[error("invalid camera descriptor: {0}")]
    InvalidCameraDescriptor(String),
    #[error("invalid camera control descriptor: {0}")]
    InvalidCameraControl(String),
    #[error("invalid video frame descriptor: {0}")]
    InvalidFrameDescriptor(String),
}
