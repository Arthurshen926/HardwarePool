use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum EncodedDisplayError {
    #[error("invalid encoded display stream: {0}")]
    InvalidStream(String),
    #[error("invalid AVC decoder configuration: {0}")]
    InvalidDecoderConfig(String),
    #[error("invalid encoded access unit: {0}")]
    InvalidAccessUnit(String),
    #[error("invalid private display record: {0}")]
    InvalidRecord(String),
}
