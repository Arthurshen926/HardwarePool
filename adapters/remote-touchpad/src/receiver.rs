use std::{error::Error, fmt};

use capyio_input::{
    InputContractError, InputSequenceOutcome, InputSequenceTracker, InputStreamDescriptor,
    TouchpadDescriptor, TouchpadFrame,
};

use crate::{PrivateTouchpadPacketCodecV1, PrivateTouchpadPacketError};

const RATE_WINDOW_NANOS: u64 = 1_000_000_000;

pub const DEFAULT_PRIVATE_TOUCHPAD_PACKETS_PER_SECOND: u16 = 240;
pub const MAX_PRIVATE_TOUCHPAD_PACKETS_PER_SECOND: u16 = 1_000;
pub const MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS: u64 = 10_000_000;
pub const DEFAULT_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS: u64 = 250_000_000;
pub const MAX_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS: u64 = 30_000_000_000;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadReceiverLimits {
    pub max_packets_per_second: u16,
    pub active_idle_timeout_nanos: u64,
}

impl Default for PrivateTouchpadReceiverLimits {
    fn default() -> Self {
        Self {
            max_packets_per_second: DEFAULT_PRIVATE_TOUCHPAD_PACKETS_PER_SECOND,
            active_idle_timeout_nanos: DEFAULT_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
        }
    }
}

impl PrivateTouchpadReceiverLimits {
    pub(crate) fn validate(self) -> Result<(), PrivateTouchpadReceiverBuildError> {
        if self.max_packets_per_second == 0
            || self.max_packets_per_second > MAX_PRIVATE_TOUCHPAD_PACKETS_PER_SECOND
        {
            return Err(PrivateTouchpadReceiverBuildError::InvalidPacketsPerSecond {
                actual: self.max_packets_per_second,
                maximum: MAX_PRIVATE_TOUCHPAD_PACKETS_PER_SECOND,
            });
        }
        if !(MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS..=MAX_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS)
            .contains(&self.active_idle_timeout_nanos)
        {
            return Err(PrivateTouchpadReceiverBuildError::InvalidIdleTimeout {
                actual: self.active_idle_timeout_nanos,
                minimum: MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
                maximum: MAX_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadReceiverBuildError {
    Packet(PrivateTouchpadPacketError),
    Contract(InputContractError),
    InvalidPacketsPerSecond {
        actual: u16,
        maximum: u16,
    },
    InvalidIdleTimeout {
        actual: u64,
        minimum: u64,
        maximum: u64,
    },
}

impl fmt::Display for PrivateTouchpadReceiverBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Packet(error) => write!(formatter, "invalid touchpad packet binding: {error}"),
            Self::Contract(error) => {
                write!(formatter, "invalid touchpad sequence binding: {error}")
            }
            Self::InvalidPacketsPerSecond { actual, maximum } => write!(
                formatter,
                "private touchpad packet limit must be 1..={maximum} packets/s; received {actual}"
            ),
            Self::InvalidIdleTimeout {
                actual,
                minimum,
                maximum,
            } => write!(
                formatter,
                "private touchpad active idle timeout must be {minimum}..={maximum} ns; received {actual}"
            ),
        }
    }
}

impl Error for PrivateTouchpadReceiverBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Packet(error) => Some(error),
            Self::Contract(error) => Some(error),
            Self::InvalidPacketsPerSecond { .. } | Self::InvalidIdleTimeout { .. } => None,
        }
    }
}

impl From<PrivateTouchpadPacketError> for PrivateTouchpadReceiverBuildError {
    fn from(error: PrivateTouchpadPacketError) -> Self {
        Self::Packet(error)
    }
}

impl From<InputContractError> for PrivateTouchpadReceiverBuildError {
    fn from(error: InputContractError) -> Self {
        Self::Contract(error)
    }
}

/// Lifecycle boundary implemented by an authorized touchpad Sink.
///
/// The Sink must be bound to the same Stream, epoch and first sequence as the
/// receiver. It remains responsible for semantic gap cancellation/suppression;
/// the receiver deliberately submits a forward-gap frame so that policy is not
/// hidden at the byte boundary.
pub trait PrivateTouchpadSink {
    type Error;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error>;

    /// Advances the Sink before the receiver commits the same fresh epoch.
    fn advance_epoch(&mut self, new_epoch: u64, first_sequence: u64) -> Result<(), Self::Error>;

    /// Cancels retained contacts and closes the Sink lifecycle.
    fn close(&mut self) -> Result<(), Self::Error>;
}

#[cfg(windows)]
impl PrivateTouchpadSink for capyio_windows_input::SyntheticTouchpadSession {
    type Error = capyio_windows_input::SyntheticTouchpadSessionError;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        Self::submit_frame(self, frame).map(|_| ())
    }

    fn advance_epoch(&mut self, new_epoch: u64, first_sequence: u64) -> Result<(), Self::Error> {
        Self::advance_epoch(self, new_epoch, first_sequence).map(|_| ())
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Self::close(self).map(|_| ())
    }
}

#[cfg(windows)]
impl<T> PrivateTouchpadSink for capyio_windows_input::VhfTouchpadSession<T>
where
    T: capyio_windows_input::VhfBrokerRecordTransport,
{
    type Error = capyio_windows_input::VhfTouchpadSessionError;

    fn submit_frame(&mut self, frame: &TouchpadFrame) -> Result<(), Self::Error> {
        Self::submit_frame(self, frame)
    }

    fn advance_epoch(&mut self, new_epoch: u64, _first_sequence: u64) -> Result<(), Self::Error> {
        Self::advance_epoch(self, new_epoch)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Self::close(self)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadReceiverState {
    Active,
    Failed,
    TimedOut,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadReceiveOutcome {
    pub sequence: u64,
    pub sequence_outcome: InputSequenceOutcome,
    pub active_contacts: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadPollOutcome {
    Pending,
    Idle,
    TimedOut,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadReceiverFault {
    Packet(PrivateTouchpadPacketError),
    Sequence(InputContractError),
    ArrivalClockRegression {
        previous: u64,
        actual: u64,
    },
    RateLimitExceeded {
        limit: u16,
        window_started_nanos: u64,
    },
}

impl fmt::Display for PrivateTouchpadReceiverFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Packet(error) => write!(formatter, "private touchpad packet rejected: {error}"),
            Self::Sequence(error) => {
                write!(formatter, "private touchpad sequence rejected: {error}")
            }
            Self::ArrivalClockRegression { previous, actual } => write!(
                formatter,
                "private touchpad receive clock regressed from {previous} to {actual} ns"
            ),
            Self::RateLimitExceeded {
                limit,
                window_started_nanos,
            } => write!(
                formatter,
                "private touchpad exceeded {limit} packets/s in window starting at {window_started_nanos} ns"
            ),
        }
    }
}

impl Error for PrivateTouchpadReceiverFault {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Packet(error) => Some(error),
            Self::Sequence(error) => Some(error),
            Self::ArrivalClockRegression { .. } | Self::RateLimitExceeded { .. } => None,
        }
    }
}

#[derive(Debug)]
pub enum PrivateTouchpadReceiverError<E> {
    Inactive(PrivateTouchpadReceiverState),
    Fault {
        fault: PrivateTouchpadReceiverFault,
        cleanup: Option<E>,
    },
    Sink {
        primary: E,
        cleanup: Option<E>,
    },
    Close(E),
}

impl<E: fmt::Display> fmt::Display for PrivateTouchpadReceiverError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inactive(state) => {
                write!(
                    formatter,
                    "private touchpad receiver is not active: {state:?}"
                )
            }
            Self::Fault {
                fault,
                cleanup: None,
            } => fault.fmt(formatter),
            Self::Fault {
                fault,
                cleanup: Some(cleanup),
            } => write!(formatter, "{fault}; Sink cleanup also failed: {cleanup}"),
            Self::Sink {
                primary,
                cleanup: None,
            } => write!(formatter, "private touchpad Sink failed: {primary}"),
            Self::Sink {
                primary,
                cleanup: Some(cleanup),
            } => write!(
                formatter,
                "private touchpad Sink failed: {primary}; cleanup also failed: {cleanup}"
            ),
            Self::Close(error) => write!(formatter, "private touchpad Sink close failed: {error}"),
        }
    }
}

impl<E: Error + 'static> Error for PrivateTouchpadReceiverError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Inactive(_) => None,
            Self::Fault { fault, .. } => Some(fault),
            Self::Sink { primary, .. } | Self::Close(primary) => Some(primary),
        }
    }
}

pub struct PrivateTouchpadReceiver<S: PrivateTouchpadSink> {
    codec: PrivateTouchpadPacketCodecV1,
    sequence: InputSequenceTracker,
    limits: PrivateTouchpadReceiverLimits,
    sink: S,
    state: PrivateTouchpadReceiverState,
    last_arrival_nanos: Option<u64>,
    rate_window_started_nanos: Option<u64>,
    packets_in_window: u16,
    contacts_active: bool,
}

impl<S: PrivateTouchpadSink> PrivateTouchpadReceiver<S> {
    pub fn new(
        stream: InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
        limits: PrivateTouchpadReceiverLimits,
        sink: S,
    ) -> Result<Self, PrivateTouchpadReceiverBuildError> {
        validate_private_touchpad_receiver_binding(&stream, descriptor, first_sequence, limits)?;
        let codec = PrivateTouchpadPacketCodecV1::new(stream.clone(), descriptor)?;
        let sequence =
            InputSequenceTracker::new(stream.stream_id, stream.stream_epoch, first_sequence)?;
        Ok(Self {
            codec,
            sequence,
            limits,
            sink,
            state: PrivateTouchpadReceiverState::Active,
            last_arrival_nanos: None,
            rate_window_started_nanos: None,
            packets_in_window: 0,
            contacts_active: false,
        })
    }

    #[must_use]
    pub const fn state(&self) -> PrivateTouchpadReceiverState {
        self.state
    }

    pub fn receive(
        &mut self,
        packet: &[u8],
        arrival_nanos: u64,
    ) -> Result<PrivateTouchpadReceiveOutcome, PrivateTouchpadReceiverError<S::Error>> {
        self.require_active()?;
        if let Some(previous) = self.last_arrival_nanos
            && arrival_nanos < previous
        {
            return self.fail_fault(PrivateTouchpadReceiverFault::ArrivalClockRegression {
                previous,
                actual: arrival_nanos,
            });
        }

        let (window_started_nanos, packets_in_window) = match self.rate_window_started_nanos {
            Some(started) if arrival_nanos.saturating_sub(started) < RATE_WINDOW_NANOS => {
                if self.packets_in_window >= self.limits.max_packets_per_second {
                    return self.fail_fault(PrivateTouchpadReceiverFault::RateLimitExceeded {
                        limit: self.limits.max_packets_per_second,
                        window_started_nanos: started,
                    });
                }
                (started, self.packets_in_window + 1)
            }
            _ => (arrival_nanos, 1),
        };

        let frame = match self.codec.decode(packet) {
            Ok(frame) => frame,
            Err(error) => {
                return self.fail_fault(PrivateTouchpadReceiverFault::Packet(error));
            }
        };
        let mut sequence = self.sequence;
        let sequence_outcome = match sequence.observe(frame.header) {
            Ok(outcome) => outcome,
            Err(error) => {
                return self.fail_fault(PrivateTouchpadReceiverFault::Sequence(error));
            }
        };

        if let Err(primary) = self.sink.submit_frame(&frame) {
            let cleanup = self.sink.close().err();
            self.state = PrivateTouchpadReceiverState::Failed;
            return Err(PrivateTouchpadReceiverError::Sink { primary, cleanup });
        }

        self.sequence = sequence;
        self.last_arrival_nanos = Some(arrival_nanos);
        self.rate_window_started_nanos = Some(window_started_nanos);
        self.packets_in_window = packets_in_window;
        self.contacts_active =
            matches!(sequence_outcome, InputSequenceOutcome::InOrder) && !frame.is_released();

        Ok(PrivateTouchpadReceiveOutcome {
            sequence: frame.header.sequence,
            sequence_outcome,
            active_contacts: if self.contacts_active {
                frame.contacts.len() as u8
            } else {
                0
            },
        })
    }

    pub fn poll_timeout(
        &mut self,
        now_nanos: u64,
    ) -> Result<PrivateTouchpadPollOutcome, PrivateTouchpadReceiverError<S::Error>> {
        self.require_active()?;
        if let Some(previous) = self.last_arrival_nanos
            && now_nanos < previous
        {
            return self.fail_fault(PrivateTouchpadReceiverFault::ArrivalClockRegression {
                previous,
                actual: now_nanos,
            });
        }
        if !self.contacts_active {
            return Ok(PrivateTouchpadPollOutcome::Idle);
        }
        let Some(last_arrival_nanos) = self.last_arrival_nanos else {
            return Ok(PrivateTouchpadPollOutcome::Pending);
        };
        if now_nanos - last_arrival_nanos < self.limits.active_idle_timeout_nanos {
            return Ok(PrivateTouchpadPollOutcome::Pending);
        }
        match self.sink.close() {
            Ok(()) => {
                self.state = PrivateTouchpadReceiverState::TimedOut;
                self.contacts_active = false;
                Ok(PrivateTouchpadPollOutcome::TimedOut)
            }
            Err(error) => {
                self.state = PrivateTouchpadReceiverState::Failed;
                Err(PrivateTouchpadReceiverError::Close(error))
            }
        }
    }

    pub fn advance_epoch(
        &mut self,
        new_epoch: u64,
        first_sequence: u64,
    ) -> Result<(), PrivateTouchpadReceiverError<S::Error>> {
        self.require_active()?;
        let mut codec = self.codec.clone();
        if let Err(error) = codec.advance_epoch(new_epoch) {
            return self.fail_fault(PrivateTouchpadReceiverFault::Packet(error));
        }
        let mut sequence = self.sequence;
        if let Err(error) = sequence.advance_epoch(new_epoch, first_sequence) {
            return self.fail_fault(PrivateTouchpadReceiverFault::Sequence(error));
        }
        if let Err(primary) = self.sink.advance_epoch(new_epoch, first_sequence) {
            let cleanup = self.sink.close().err();
            self.state = PrivateTouchpadReceiverState::Failed;
            return Err(PrivateTouchpadReceiverError::Sink { primary, cleanup });
        }

        self.codec = codec;
        self.sequence = sequence;
        self.last_arrival_nanos = None;
        self.rate_window_started_nanos = None;
        self.packets_in_window = 0;
        self.contacts_active = false;
        Ok(())
    }

    pub fn disconnect(&mut self) -> Result<(), PrivateTouchpadReceiverError<S::Error>> {
        self.require_active()?;
        match self.sink.close() {
            Ok(()) => {
                self.state = PrivateTouchpadReceiverState::Closed;
                self.contacts_active = false;
                Ok(())
            }
            Err(error) => {
                self.state = PrivateTouchpadReceiverState::Failed;
                Err(PrivateTouchpadReceiverError::Close(error))
            }
        }
    }

    fn require_active(&self) -> Result<(), PrivateTouchpadReceiverError<S::Error>> {
        if self.state == PrivateTouchpadReceiverState::Active {
            Ok(())
        } else {
            Err(PrivateTouchpadReceiverError::Inactive(self.state))
        }
    }

    fn fail_fault<T>(
        &mut self,
        fault: PrivateTouchpadReceiverFault,
    ) -> Result<T, PrivateTouchpadReceiverError<S::Error>> {
        let cleanup = self.sink.close().err();
        self.state = PrivateTouchpadReceiverState::Failed;
        self.contacts_active = false;
        Err(PrivateTouchpadReceiverError::Fault { fault, cleanup })
    }
}

pub(crate) fn validate_private_touchpad_receiver_binding(
    stream: &InputStreamDescriptor,
    descriptor: TouchpadDescriptor,
    first_sequence: u64,
    limits: PrivateTouchpadReceiverLimits,
) -> Result<(), PrivateTouchpadReceiverBuildError> {
    limits.validate()?;
    PrivateTouchpadPacketCodecV1::new(stream.clone(), descriptor)?;
    InputSequenceTracker::new(stream.stream_id, stream.stream_epoch, first_sequence)?;
    Ok(())
}

impl<S: PrivateTouchpadSink> Drop for PrivateTouchpadReceiver<S> {
    fn drop(&mut self) {
        if self.state == PrivateTouchpadReceiverState::Active {
            let _ = self.sink.close();
            self.state = PrivateTouchpadReceiverState::Closed;
            self.contacts_active = false;
        }
    }
}
