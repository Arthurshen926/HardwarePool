use std::{error::Error, fmt};

pub const MF_CAMERA_STREAM_ID: u32 = 0;
pub const MAX_PENDING_SAMPLE_REQUESTS: usize = 4;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfMediaSourceState {
    Stopped,
    Started,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfMediaStreamState {
    Stopped,
    Started,
    Shutdown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfMediaSourceOperation {
    Start,
    Stop,
    RequestSample,
    CompleteSample,
    CancelSample,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MfPresentationSelection {
    stream_count: u32,
    selected_stream_id: Option<u32>,
}

impl MfPresentationSelection {
    #[must_use]
    pub const fn new(stream_count: u32, selected_stream_id: Option<u32>) -> Self {
        Self {
            stream_count,
            selected_stream_id,
        }
    }

    #[must_use]
    pub const fn canonical() -> Self {
        Self::new(1, Some(MF_CAMERA_STREAM_ID))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MfSampleRequestTicket {
    request_id: u64,
    stream_generation: u64,
}

impl MfSampleRequestTicket {
    #[must_use]
    pub const fn request_id(self) -> u64 {
        self.request_id
    }

    #[must_use]
    pub const fn stream_generation(self) -> u64 {
        self.stream_generation
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfMediaSourceEvent {
    NewStream {
        stream_id: u32,
        stream_generation: u64,
    },
    UpdatedStream {
        stream_id: u32,
        stream_generation: u64,
    },
    StreamStarted {
        stream_id: u32,
        stream_generation: u64,
        start_time_100ns: i64,
    },
    SourceStarted {
        stream_generation: u64,
        start_time_100ns: i64,
    },
    StreamSample {
        ticket: MfSampleRequestTicket,
        sequence: u64,
    },
    StreamStopped {
        stream_id: u32,
        stream_generation: u64,
    },
    SourceStopped {
        stream_generation: u64,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MfMediaSourceStopOutcome {
    pub events: [MfMediaSourceEvent; 2],
    pub cancelled_sample_requests: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MfMediaSourceShutdownOutcome {
    pub was_started: bool,
    pub cancelled_sample_requests: usize,
}

/// Allocation-free protocol core for one Frame Server media source and stream.
///
/// Windows COM glue translates these actions to Media Foundation event queues.
/// The core itself does not call Media Foundation or touch system registration.
#[derive(Clone, Debug)]
pub struct MfMediaSourceCore {
    source_state: MfMediaSourceState,
    stream_state: MfMediaStreamState,
    stream_was_announced: bool,
    stream_generation: u64,
    next_request_id: u64,
    pending_requests: [Option<MfSampleRequestTicket>; MAX_PENDING_SAMPLE_REQUESTS],
    pending_head: usize,
    pending_len: usize,
    last_completed_sequence: Option<u64>,
}

impl Default for MfMediaSourceCore {
    fn default() -> Self {
        Self {
            source_state: MfMediaSourceState::Stopped,
            stream_state: MfMediaStreamState::Stopped,
            stream_was_announced: false,
            stream_generation: 0,
            next_request_id: 1,
            pending_requests: [None; MAX_PENDING_SAMPLE_REQUESTS],
            pending_head: 0,
            pending_len: 0,
            last_completed_sequence: None,
        }
    }
}

impl MfMediaSourceCore {
    #[must_use]
    pub const fn source_state(&self) -> MfMediaSourceState {
        self.source_state
    }

    #[must_use]
    pub const fn stream_state(&self) -> MfMediaStreamState {
        self.stream_state
    }

    #[must_use]
    pub const fn stream_generation(&self) -> u64 {
        self.stream_generation
    }

    #[must_use]
    pub const fn pending_sample_requests(&self) -> usize {
        self.pending_len
    }

    pub fn start(
        &mut self,
        selection: MfPresentationSelection,
        start_time_100ns: i64,
    ) -> Result<[MfMediaSourceEvent; 3], MfMediaSourceCoreError> {
        if self.source_state == MfMediaSourceState::Shutdown {
            return Err(MfMediaSourceCoreError::InvalidState {
                operation: MfMediaSourceOperation::Start,
                state: self.source_state,
            });
        }
        if selection != MfPresentationSelection::canonical() {
            return Err(MfMediaSourceCoreError::InvalidPresentationSelection {
                stream_count: selection.stream_count,
                selected_stream_id: selection.selected_stream_id,
            });
        }
        if start_time_100ns < 0 {
            return Err(MfMediaSourceCoreError::InvalidStartTime(start_time_100ns));
        }

        if self.source_state == MfMediaSourceState::Started {
            let stream_generation = self.stream_generation;
            return Ok([
                MfMediaSourceEvent::UpdatedStream {
                    stream_id: MF_CAMERA_STREAM_ID,
                    stream_generation,
                },
                MfMediaSourceEvent::StreamStarted {
                    stream_id: MF_CAMERA_STREAM_ID,
                    stream_generation,
                    start_time_100ns,
                },
                MfMediaSourceEvent::SourceStarted {
                    stream_generation,
                    start_time_100ns,
                },
            ]);
        }

        let stream_generation = self
            .stream_generation
            .checked_add(1)
            .ok_or(MfMediaSourceCoreError::StreamGenerationExhausted)?;
        let announcement = if self.stream_was_announced {
            MfMediaSourceEvent::UpdatedStream {
                stream_id: MF_CAMERA_STREAM_ID,
                stream_generation,
            }
        } else {
            MfMediaSourceEvent::NewStream {
                stream_id: MF_CAMERA_STREAM_ID,
                stream_generation,
            }
        };

        self.stream_generation = stream_generation;
        self.stream_was_announced = true;
        self.source_state = MfMediaSourceState::Started;
        self.stream_state = MfMediaStreamState::Started;
        self.last_completed_sequence = None;

        Ok([
            announcement,
            MfMediaSourceEvent::StreamStarted {
                stream_id: MF_CAMERA_STREAM_ID,
                stream_generation,
                start_time_100ns,
            },
            MfMediaSourceEvent::SourceStarted {
                stream_generation,
                start_time_100ns,
            },
        ])
    }

    pub fn request_sample(&mut self) -> Result<MfSampleRequestTicket, MfMediaSourceCoreError> {
        self.require_started(MfMediaSourceOperation::RequestSample)?;
        if self.pending_len == MAX_PENDING_SAMPLE_REQUESTS {
            return Err(MfMediaSourceCoreError::PendingRequestLimitReached {
                maximum: MAX_PENDING_SAMPLE_REQUESTS,
            });
        }

        let request_id = self.next_request_id;
        let next_request_id = request_id
            .checked_add(1)
            .ok_or(MfMediaSourceCoreError::RequestIdExhausted)?;
        let ticket = MfSampleRequestTicket {
            request_id,
            stream_generation: self.stream_generation,
        };
        let insertion_index = (self.pending_head + self.pending_len) % MAX_PENDING_SAMPLE_REQUESTS;
        self.pending_requests[insertion_index] = Some(ticket);
        self.pending_len += 1;
        self.next_request_id = next_request_id;
        Ok(ticket)
    }

    pub fn complete_sample(
        &mut self,
        ticket: MfSampleRequestTicket,
        sequence: u64,
    ) -> Result<MfMediaSourceEvent, MfMediaSourceCoreError> {
        self.require_started(MfMediaSourceOperation::CompleteSample)?;
        let expected = self.pending_requests[self.pending_head]
            .ok_or(MfMediaSourceCoreError::NoPendingSampleRequest)?;
        if ticket != expected {
            return Err(MfMediaSourceCoreError::WrongSampleRequest {
                expected,
                actual: ticket,
            });
        }
        if self
            .last_completed_sequence
            .is_some_and(|previous| sequence <= previous)
        {
            return Err(MfMediaSourceCoreError::NonAdvancingFrameSequence {
                previous: self
                    .last_completed_sequence
                    .expect("the preceding predicate observed a sequence"),
                actual: sequence,
            });
        }

        self.pop_oldest_pending_request();
        self.last_completed_sequence = Some(sequence);
        Ok(MfMediaSourceEvent::StreamSample { ticket, sequence })
    }

    /// Removes the oldest request when sample construction fails before an
    /// `MEMediaSample` event can be queued. The stream remains started so a
    /// transient allocation or Media Foundation error cannot strand a ticket
    /// or silently force a source-level state transition.
    pub fn cancel_sample(
        &mut self,
        ticket: MfSampleRequestTicket,
    ) -> Result<(), MfMediaSourceCoreError> {
        self.require_started(MfMediaSourceOperation::CancelSample)?;
        let expected = self.pending_requests[self.pending_head]
            .ok_or(MfMediaSourceCoreError::NoPendingSampleRequest)?;
        if ticket != expected {
            return Err(MfMediaSourceCoreError::WrongSampleRequest {
                expected,
                actual: ticket,
            });
        }

        self.pop_oldest_pending_request();
        Ok(())
    }

    pub fn stop(&mut self) -> Result<MfMediaSourceStopOutcome, MfMediaSourceCoreError> {
        self.require_started(MfMediaSourceOperation::Stop)?;
        let cancelled_sample_requests = self.clear_pending_requests();
        let stream_generation = self.stream_generation;
        self.stream_state = MfMediaStreamState::Stopped;
        self.source_state = MfMediaSourceState::Stopped;
        self.last_completed_sequence = None;
        Ok(MfMediaSourceStopOutcome {
            events: [
                MfMediaSourceEvent::StreamStopped {
                    stream_id: MF_CAMERA_STREAM_ID,
                    stream_generation,
                },
                MfMediaSourceEvent::SourceStopped { stream_generation },
            ],
            cancelled_sample_requests,
        })
    }

    pub fn shutdown(&mut self) -> MfMediaSourceShutdownOutcome {
        let was_started = self.source_state == MfMediaSourceState::Started;
        let cancelled_sample_requests = self.clear_pending_requests();
        self.stream_state = MfMediaStreamState::Shutdown;
        self.source_state = MfMediaSourceState::Shutdown;
        self.last_completed_sequence = None;
        MfMediaSourceShutdownOutcome {
            was_started,
            cancelled_sample_requests,
        }
    }

    fn require_started(
        &self,
        operation: MfMediaSourceOperation,
    ) -> Result<(), MfMediaSourceCoreError> {
        if self.source_state == MfMediaSourceState::Started
            && self.stream_state == MfMediaStreamState::Started
        {
            Ok(())
        } else {
            Err(MfMediaSourceCoreError::InvalidState {
                operation,
                state: self.source_state,
            })
        }
    }

    fn clear_pending_requests(&mut self) -> usize {
        let cancelled = self.pending_len;
        self.pending_requests.fill(None);
        self.pending_head = 0;
        self.pending_len = 0;
        cancelled
    }

    fn pop_oldest_pending_request(&mut self) {
        debug_assert!(self.pending_len > 0);
        debug_assert!(self.pending_requests[self.pending_head].is_some());
        self.pending_requests[self.pending_head] = None;
        self.pending_head = (self.pending_head + 1) % MAX_PENDING_SAMPLE_REQUESTS;
        self.pending_len -= 1;
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MfMediaSourceCoreError {
    InvalidState {
        operation: MfMediaSourceOperation,
        state: MfMediaSourceState,
    },
    InvalidPresentationSelection {
        stream_count: u32,
        selected_stream_id: Option<u32>,
    },
    InvalidStartTime(i64),
    StreamGenerationExhausted,
    RequestIdExhausted,
    PendingRequestLimitReached {
        maximum: usize,
    },
    NoPendingSampleRequest,
    WrongSampleRequest {
        expected: MfSampleRequestTicket,
        actual: MfSampleRequestTicket,
    },
    NonAdvancingFrameSequence {
        previous: u64,
        actual: u64,
    },
}

impl fmt::Display for MfMediaSourceCoreError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidState { operation, state } => {
                write!(
                    formatter,
                    "cannot {operation:?} while media source is {state:?}"
                )
            }
            Self::InvalidPresentationSelection {
                stream_count,
                selected_stream_id,
            } => write!(
                formatter,
                "presentation selects {selected_stream_id:?} from {stream_count} streams; exactly stream 0 is required"
            ),
            Self::InvalidStartTime(value) => {
                write!(formatter, "Media Foundation start time {value} is negative")
            }
            Self::StreamGenerationExhausted => {
                formatter.write_str("media stream generation is exhausted")
            }
            Self::RequestIdExhausted => formatter.write_str("sample request ID is exhausted"),
            Self::PendingRequestLimitReached { maximum } => {
                write!(formatter, "pending sample request limit {maximum} reached")
            }
            Self::NoPendingSampleRequest => formatter.write_str("no sample request is pending"),
            Self::WrongSampleRequest { expected, actual } => write!(
                formatter,
                "sample request {} does not match oldest pending request {}",
                actual.request_id, expected.request_id
            ),
            Self::NonAdvancingFrameSequence { previous, actual } => write!(
                formatter,
                "frame sequence {actual} does not advance completed sequence {previous}"
            ),
        }
    }
}

impl Error for MfMediaSourceCoreError {}
