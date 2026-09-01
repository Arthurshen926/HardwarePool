use std::{error::Error, fmt};

use capyio_input::{InputStreamDescriptor, TouchpadDescriptor};

use crate::{
    PrivateTouchpadPollOutcome, PrivateTouchpadReceiveOutcome, PrivateTouchpadReceiver,
    PrivateTouchpadReceiverBuildError, PrivateTouchpadReceiverError, PrivateTouchpadReceiverLimits,
    PrivateTouchpadReceiverState, PrivateTouchpadRouteBinding, PrivateTouchpadSink,
    PrivateTouchpadSinkFactory, PrivateTouchpadTransportCodecV1,
    PrivateTouchpadTransportRecordError, PrivateTouchpadTransportRecordV1,
    receiver::validate_private_touchpad_receiver_binding,
};

/// Lifecycle of one pre-authenticated private touchpad transport connection.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadTransportReceiverState {
    AwaitingHello,
    Active,
    TimedOut,
    Failed,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadTransportReceiverBuildError {
    EpochMismatch { route: u64, stream: u64 },
    Receiver(PrivateTouchpadReceiverBuildError),
}

impl fmt::Display for PrivateTouchpadTransportReceiverBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EpochMismatch { route, stream } => write!(
                formatter,
                "private touchpad transport Route epoch {route} does not match Stream epoch {stream}"
            ),
            Self::Receiver(error) => error.fmt(formatter),
        }
    }
}

impl Error for PrivateTouchpadTransportReceiverBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::EpochMismatch { .. } => None,
            Self::Receiver(error) => Some(error),
        }
    }
}

impl From<PrivateTouchpadReceiverBuildError> for PrivateTouchpadTransportReceiverBuildError {
    fn from(error: PrivateTouchpadReceiverBuildError) -> Self {
        Self::Receiver(error)
    }
}

#[derive(Debug)]
pub enum PrivateTouchpadTransportReceiverError<FactoryError, SinkError> {
    Inactive(PrivateTouchpadTransportReceiverState),
    Transport {
        primary: PrivateTouchpadTransportRecordError,
        cleanup: Option<PrivateTouchpadReceiverError<SinkError>>,
    },
    SinkFactory(FactoryError),
    ReceiverBuild(PrivateTouchpadReceiverBuildError),
    Receiver(PrivateTouchpadReceiverError<SinkError>),
}

impl<FactoryError: fmt::Display, SinkError: fmt::Display> fmt::Display
    for PrivateTouchpadTransportReceiverError<FactoryError, SinkError>
{
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inactive(state) => write!(
                formatter,
                "private touchpad transport receiver is not active: {state:?}"
            ),
            Self::Transport {
                primary,
                cleanup: None,
            } => write!(formatter, "private touchpad transport rejected: {primary}"),
            Self::Transport {
                primary,
                cleanup: Some(cleanup),
            } => write!(
                formatter,
                "private touchpad transport rejected: {primary}; Sink cleanup also failed: {cleanup}"
            ),
            Self::SinkFactory(error) => {
                write!(
                    formatter,
                    "private touchpad transport Sink factory failed: {error}"
                )
            }
            Self::ReceiverBuild(error) => error.fmt(formatter),
            Self::Receiver(error) => error.fmt(formatter),
        }
    }
}

impl<FactoryError: Error + 'static, SinkError: Error + 'static> Error
    for PrivateTouchpadTransportReceiverError<FactoryError, SinkError>
{
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Inactive(_) => None,
            Self::Transport { primary, .. } => Some(primary),
            Self::SinkFactory(error) => Some(error),
            Self::ReceiverBuild(error) => Some(error),
            Self::Receiver(error) => Some(error),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadTransportReceiveOutcome {
    pub receive: PrivateTouchpadReceiveOutcome,
    pub ack: PrivateTouchpadTransportRecordV1,
}

/// Validates one exact transport binding before opening and driving a Sink.
///
/// Construction validates only the bounded semantic contract. The caller must
/// first derive `binding`, `stream` and `descriptor` from a current authorized
/// Runtime Route; possession of those values is not peer authentication. The
/// platform Sink is not opened until the complete Hello record matches.
pub struct PrivateTouchpadTransportReceiver<F>
where
    F: PrivateTouchpadSinkFactory,
{
    transport: PrivateTouchpadTransportCodecV1,
    stream: InputStreamDescriptor,
    descriptor: TouchpadDescriptor,
    first_sequence: u64,
    limits: PrivateTouchpadReceiverLimits,
    factory: F,
    receiver: Option<PrivateTouchpadReceiver<F::Sink>>,
    state: PrivateTouchpadTransportReceiverState,
}

impl<F> PrivateTouchpadTransportReceiver<F>
where
    F: PrivateTouchpadSinkFactory,
{
    pub fn new(
        binding: PrivateTouchpadRouteBinding,
        stream: InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
        limits: PrivateTouchpadReceiverLimits,
        factory: F,
    ) -> Result<Self, PrivateTouchpadTransportReceiverBuildError> {
        if binding.route_epoch != stream.stream_epoch {
            return Err(PrivateTouchpadTransportReceiverBuildError::EpochMismatch {
                route: binding.route_epoch,
                stream: stream.stream_epoch,
            });
        }
        validate_private_touchpad_receiver_binding(&stream, descriptor, first_sequence, limits)?;
        Ok(Self {
            transport: PrivateTouchpadTransportCodecV1::new(binding),
            stream,
            descriptor,
            first_sequence,
            limits,
            factory,
            receiver: None,
            state: PrivateTouchpadTransportReceiverState::AwaitingHello,
        })
    }

    #[must_use]
    pub const fn state(&self) -> PrivateTouchpadTransportReceiverState {
        self.state
    }

    pub fn accept_hello(
        &mut self,
        record: &[u8],
    ) -> Result<
        (),
        PrivateTouchpadTransportReceiverError<F::Error, <F::Sink as PrivateTouchpadSink>::Error>,
    > {
        self.require_state(PrivateTouchpadTransportReceiverState::AwaitingHello)?;
        if let Err(primary) = self.transport.validate_hello(record) {
            self.state = PrivateTouchpadTransportReceiverState::Failed;
            return Err(PrivateTouchpadTransportReceiverError::Transport {
                primary,
                cleanup: None,
            });
        }
        let sink = match self
            .factory
            .open(&self.stream, self.descriptor, self.first_sequence)
        {
            Ok(sink) => sink,
            Err(error) => {
                self.state = PrivateTouchpadTransportReceiverState::Failed;
                return Err(PrivateTouchpadTransportReceiverError::SinkFactory(error));
            }
        };
        let receiver = match PrivateTouchpadReceiver::new(
            self.stream.clone(),
            self.descriptor,
            self.first_sequence,
            self.limits,
            sink,
        ) {
            Ok(receiver) => receiver,
            Err(error) => {
                self.state = PrivateTouchpadTransportReceiverState::Failed;
                return Err(PrivateTouchpadTransportReceiverError::ReceiverBuild(error));
            }
        };
        self.receiver = Some(receiver);
        self.state = PrivateTouchpadTransportReceiverState::Active;
        Ok(())
    }

    pub fn receive_data(
        &mut self,
        record: &[u8],
        arrival_nanos: u64,
    ) -> Result<
        PrivateTouchpadTransportReceiveOutcome,
        PrivateTouchpadTransportReceiverError<F::Error, <F::Sink as PrivateTouchpadSink>::Error>,
    > {
        self.require_state(PrivateTouchpadTransportReceiverState::Active)?;
        let packet = match self.transport.decode_data(record) {
            Ok(packet) => packet,
            Err(primary) => return self.fail_transport(primary),
        };
        let receive = match self
            .receiver_mut()
            .receive(packet.as_bytes(), arrival_nanos)
        {
            Ok(outcome) => outcome,
            Err(error) => {
                self.state = PrivateTouchpadTransportReceiverState::Failed;
                return Err(PrivateTouchpadTransportReceiverError::Receiver(error));
            }
        };
        Ok(PrivateTouchpadTransportReceiveOutcome {
            ack: self.transport.encode_ack(receive.sequence),
            receive,
        })
    }

    pub fn poll_timeout(
        &mut self,
        now_nanos: u64,
    ) -> Result<
        PrivateTouchpadPollOutcome,
        PrivateTouchpadTransportReceiverError<F::Error, <F::Sink as PrivateTouchpadSink>::Error>,
    > {
        self.require_state(PrivateTouchpadTransportReceiverState::Active)?;
        match self.receiver_mut().poll_timeout(now_nanos) {
            Ok(PrivateTouchpadPollOutcome::TimedOut) => {
                self.state = PrivateTouchpadTransportReceiverState::TimedOut;
                Ok(PrivateTouchpadPollOutcome::TimedOut)
            }
            Ok(outcome) => Ok(outcome),
            Err(error) => {
                self.state = PrivateTouchpadTransportReceiverState::Failed;
                Err(PrivateTouchpadTransportReceiverError::Receiver(error))
            }
        }
    }

    pub fn accept_close(
        &mut self,
        record: &[u8],
    ) -> Result<
        (),
        PrivateTouchpadTransportReceiverError<F::Error, <F::Sink as PrivateTouchpadSink>::Error>,
    > {
        self.require_state(PrivateTouchpadTransportReceiverState::Active)?;
        if let Err(primary) = self.transport.validate_close(record) {
            return self.fail_transport(primary);
        }
        self.disconnect()
    }

    pub fn disconnect(
        &mut self,
    ) -> Result<
        (),
        PrivateTouchpadTransportReceiverError<F::Error, <F::Sink as PrivateTouchpadSink>::Error>,
    > {
        self.require_state(PrivateTouchpadTransportReceiverState::Active)?;
        match self.receiver_mut().disconnect() {
            Ok(()) => {
                self.state = PrivateTouchpadTransportReceiverState::Closed;
                Ok(())
            }
            Err(error) => {
                self.state = PrivateTouchpadTransportReceiverState::Failed;
                Err(PrivateTouchpadTransportReceiverError::Receiver(error))
            }
        }
    }

    fn receiver_mut(&mut self) -> &mut PrivateTouchpadReceiver<F::Sink> {
        self.receiver
            .as_mut()
            .expect("active transport receiver owns a receiver")
    }

    fn require_state(
        &self,
        expected: PrivateTouchpadTransportReceiverState,
    ) -> Result<
        (),
        PrivateTouchpadTransportReceiverError<F::Error, <F::Sink as PrivateTouchpadSink>::Error>,
    > {
        if self.state == expected {
            Ok(())
        } else {
            Err(PrivateTouchpadTransportReceiverError::Inactive(self.state))
        }
    }

    fn fail_transport<T>(
        &mut self,
        primary: PrivateTouchpadTransportRecordError,
    ) -> Result<
        T,
        PrivateTouchpadTransportReceiverError<F::Error, <F::Sink as PrivateTouchpadSink>::Error>,
    > {
        let cleanup = self
            .receiver
            .as_mut()
            .filter(|receiver| receiver.state() == PrivateTouchpadReceiverState::Active)
            .and_then(|receiver| receiver.disconnect().err());
        self.state = PrivateTouchpadTransportReceiverState::Failed;
        Err(PrivateTouchpadTransportReceiverError::Transport { primary, cleanup })
    }
}
