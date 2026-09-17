use thiserror::Error;

#[derive(Clone, Debug, Eq, Error, PartialEq)]
pub enum WindowsDisplayError {
    #[error("invalid Windows mirror stream: {0}")]
    InvalidStream(String),
    #[error("invalid captured BGRA frame: {0}")]
    InvalidFrame(String),
    #[error("cannot {operation} while mirror Source is {state}")]
    InvalidState {
        operation: &'static str,
        state: &'static str,
    },
    #[error("capture target size changed to {width}x{height}; exact renegotiation is required")]
    ReconfigureRequired { width: u32, height: u32 },
    #[error("new stream mode does not match the pending capture size")]
    ReconfigureSizeMismatch,
    #[error("capture sequence or timestamp arithmetic exhausted its bounded range")]
    ArithmeticExhausted,
    #[error("the Windows Graphics Capture system picker was cancelled")]
    PickerCancelled,
    #[error("Windows Graphics Capture is not supported on this system")]
    CaptureUnsupported,
    #[error("Windows capture backend failed: {0}")]
    Backend(String),
    #[error(transparent)]
    DisplayContract(#[from] capyio_display::DisplayContractError),
}
