use capyio_core::StreamId;
use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum InputContractError {
    #[error("invalid input stream descriptor: {0}")]
    InvalidStream(String),
    #[error("invalid input frame header: {0}")]
    InvalidHeader(String),
    #[error("input frame belongs to stream {actual}; expected {expected}")]
    WrongStream {
        expected: StreamId,
        actual: StreamId,
    },
    #[error("input frame epoch {actual} is stale; current epoch is {current}")]
    StaleEpoch { current: u64, actual: u64 },
    #[error("input frame epoch {actual} is from the future; current epoch is {current}")]
    FutureEpoch { current: u64, actual: u64 },
    #[error("input sequence {actual} is duplicate or late; expected at least {expected}")]
    DuplicateOrLate { expected: u64, actual: u64 },
    #[error("input sequence is exhausted for the current epoch")]
    SequenceExhausted,
    #[error("new input epoch {new_epoch} does not advance current epoch {current_epoch}")]
    NonAdvancingEpoch { current_epoch: u64, new_epoch: u64 },
    #[error("invalid pointer frame: {0}")]
    InvalidPointerFrame(String),
    #[error("invalid touch frame: {0}")]
    InvalidTouchFrame(String),
    #[error("invalid touchpad descriptor: {0}")]
    InvalidTouchpadDescriptor(String),
    #[error("invalid touchpad frame: {0}")]
    InvalidTouchpadFrame(String),
    #[error("invalid touchpad fixture: {0}")]
    InvalidTouchpadFixture(String),
    #[error("touchpad timestamp {actual} regressed below {previous}")]
    TouchpadTimestampRegression { previous: u64, actual: u64 },
    #[error("invalid keyboard frame: {0}")]
    InvalidKeyboardFrame(String),
    #[error("invalid gamepad state: {0}")]
    InvalidGamepadState(String),
    #[error("invalid haptics command: {0}")]
    InvalidHapticsCommand(String),
}
