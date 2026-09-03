use std::fmt;

use capyio_video::VideoContractError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CameraFixtureError {
    InvalidStreamEpoch,
    InvalidQueueCapacity { actual: usize, maximum: usize },
    SequenceExhausted,
    TimestampOverflow,
    QueueFull { rejected_sequence: u64 },
    VideoContract(VideoContractError),
    PayloadLengthMismatch { declared: u64, actual: usize },
}

impl fmt::Display for CameraFixtureError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStreamEpoch => formatter.write_str("stream epoch must be positive"),
            Self::InvalidQueueCapacity { actual, maximum } => write!(
                formatter,
                "fixture queue capacity {actual} is outside 1..={maximum}"
            ),
            Self::SequenceExhausted => formatter.write_str("video sequence is exhausted"),
            Self::TimestampOverflow => formatter.write_str("video timestamp overflowed"),
            Self::QueueFull { rejected_sequence } => {
                write!(
                    formatter,
                    "fixture queue rejected sequence {rejected_sequence}"
                )
            }
            Self::VideoContract(error) => write!(formatter, "{error}"),
            Self::PayloadLengthMismatch { declared, actual } => write!(
                formatter,
                "frame declares {declared} payload bytes but owns {actual}"
            ),
        }
    }
}

impl std::error::Error for CameraFixtureError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::VideoContract(error) => Some(error),
            _ => None,
        }
    }
}

impl From<VideoContractError> for CameraFixtureError {
    fn from(value: VideoContractError) -> Self {
        Self::VideoContract(value)
    }
}
