use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum DisplayContractError {
    #[error("invalid display stream specification: {0}")]
    InvalidStreamSpec(String),
    #[error("display endpoint must advertise at least one stream candidate")]
    EmptyStreamCandidates,
    #[error("display endpoint advertises {actual} candidates; limit is {limit}")]
    TooManyStreamCandidates { actual: usize, limit: usize },
    #[error("display endpoint advertises the same stream candidate more than once")]
    DuplicateStreamCandidate,
    #[error("display Source must advertise at least one workflow")]
    EmptySourceWorkflows,
    #[error("display Source does not support the requested workflow")]
    UnsupportedWorkflow,
    #[error("display Source and Sink have no identical compatible stream candidate")]
    NoCompatibleDisplayStream,
    #[error("invalid display frame: {0}")]
    InvalidFrame(String),
    #[error("new epoch {new_epoch} must be greater than current epoch {current_epoch}")]
    NonAdvancingEpoch { current_epoch: u64, new_epoch: u64 },
    #[error("deterministic display fixture supports BGRA8 streams only")]
    FixtureRequiresBgra8,
    #[error("deterministic display fixture exhausted its bounded sequence or timestamp range")]
    FixtureExhausted,
}
