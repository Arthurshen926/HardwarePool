use std::{error::Error, fmt};

use capyio_core::StreamId;

use crate::{
    BoundedFrameQueue, CameraFixtureError, FrameQueueMetrics, FrameQueueOverflowPolicy,
    FrameQueuePushOutcome, GeneratedVideoFrame,
};

/// A worker-thread ingress for already-decoded canonical camera frames.
///
/// It deliberately knows nothing about codecs, network protocols, shared
/// memory or Media Foundation. Those process boundaries must validate and own
/// their payload before handing an owned frame to this queue.
#[derive(Clone, Debug)]
pub struct ExternalNv12FrameIngress {
    stream_id: StreamId,
    stream_epoch: u64,
    frames: BoundedFrameQueue,
    last_accepted_sequence: Option<u64>,
    last_accepted_timestamp_nanos: Option<u64>,
}

impl ExternalNv12FrameIngress {
    pub fn new(
        stream_id: StreamId,
        stream_epoch: u64,
        capacity: usize,
    ) -> Result<Self, ExternalNv12FrameIngressError> {
        if stream_epoch == 0 {
            return Err(ExternalNv12FrameIngressError::InvalidStreamEpoch);
        }
        Ok(Self {
            stream_id,
            stream_epoch,
            frames: BoundedFrameQueue::new(capacity, FrameQueueOverflowPolicy::DropOldest)?,
            last_accepted_sequence: None,
            last_accepted_timestamp_nanos: None,
        })
    }

    pub fn push(
        &mut self,
        frame: GeneratedVideoFrame,
    ) -> Result<FrameQueuePushOutcome, ExternalNv12FrameIngressError> {
        frame.validate(&crate::fixture_stream_spec())?;
        if frame.descriptor.flags.end_of_stream {
            return Err(ExternalNv12FrameIngressError::EndOfStreamUnsupported);
        }
        if frame.descriptor.stream_id != self.stream_id {
            return Err(ExternalNv12FrameIngressError::WrongStream {
                expected: self.stream_id,
                actual: frame.descriptor.stream_id,
            });
        }
        if frame.descriptor.stream_epoch != self.stream_epoch {
            return Err(ExternalNv12FrameIngressError::WrongEpoch {
                expected: self.stream_epoch,
                actual: frame.descriptor.stream_epoch,
            });
        }
        if self
            .last_accepted_sequence
            .is_some_and(|previous| frame.descriptor.sequence <= previous)
        {
            return Err(ExternalNv12FrameIngressError::NonAdvancingSequence {
                previous: self
                    .last_accepted_sequence
                    .expect("checked as present above"),
                actual: frame.descriptor.sequence,
            });
        }
        if self
            .last_accepted_timestamp_nanos
            .is_some_and(|previous| frame.descriptor.source_timestamp_nanos <= previous)
        {
            return Err(ExternalNv12FrameIngressError::NonAdvancingSourceTimestamp {
                previous: self
                    .last_accepted_timestamp_nanos
                    .expect("checked as present above"),
                actual: frame.descriptor.source_timestamp_nanos,
            });
        }

        let sequence = frame.descriptor.sequence;
        let source_timestamp_nanos = frame.descriptor.source_timestamp_nanos;
        let outcome = self.frames.push(frame)?;
        self.last_accepted_sequence = Some(sequence);
        self.last_accepted_timestamp_nanos = Some(source_timestamp_nanos);
        Ok(outcome)
    }

    pub fn pop(&mut self) -> Option<GeneratedVideoFrame> {
        self.frames.pop()
    }

    #[must_use]
    pub const fn stream_id(&self) -> StreamId {
        self.stream_id
    }

    #[must_use]
    pub const fn stream_epoch(&self) -> u64 {
        self.stream_epoch
    }

    #[must_use]
    pub fn pending_frames(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.frames.capacity()
    }

    #[must_use]
    pub const fn metrics(&self) -> FrameQueueMetrics {
        self.frames.metrics()
    }

    #[must_use]
    pub const fn last_accepted_sequence(&self) -> Option<u64> {
        self.last_accepted_sequence
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ExternalNv12FrameIngressError {
    InvalidStreamEpoch,
    WrongStream {
        expected: StreamId,
        actual: StreamId,
    },
    WrongEpoch {
        expected: u64,
        actual: u64,
    },
    NonAdvancingSequence {
        previous: u64,
        actual: u64,
    },
    NonAdvancingSourceTimestamp {
        previous: u64,
        actual: u64,
    },
    EndOfStreamUnsupported,
    Frame(CameraFixtureError),
}

impl fmt::Display for ExternalNv12FrameIngressError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidStreamEpoch => formatter.write_str("stream epoch must be positive"),
            Self::WrongStream { expected, actual } => {
                write!(formatter, "expected stream {expected}, received {actual}")
            }
            Self::WrongEpoch { expected, actual } => {
                write!(
                    formatter,
                    "expected stream epoch {expected}, received {actual}"
                )
            }
            Self::NonAdvancingSequence { previous, actual } => write!(
                formatter,
                "frame sequence {actual} does not advance beyond {previous}"
            ),
            Self::NonAdvancingSourceTimestamp { previous, actual } => write!(
                formatter,
                "source timestamp {actual} does not advance beyond {previous}"
            ),
            Self::EndOfStreamUnsupported => {
                formatter.write_str("the live Windows camera ingress does not accept end-of-stream")
            }
            Self::Frame(error) => error.fmt(formatter),
        }
    }
}

impl Error for ExternalNv12FrameIngressError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Frame(error) => Some(error),
            _ => None,
        }
    }
}

impl From<CameraFixtureError> for ExternalNv12FrameIngressError {
    fn from(value: CameraFixtureError) -> Self {
        Self::Frame(value)
    }
}
