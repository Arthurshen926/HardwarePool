use std::{collections::VecDeque, error::Error, fmt};

use capyio_core::{
    AuthorizationState, PortRef, ProfileId, Route, RouteBackend, RouteId, RouteState, SessionId,
};
use capyio_input::{InputStreamDescriptor, TouchpadDescriptor, touchpad_frames_profile};

use crate::{
    PRIVATE_TOUCHPAD_PACKET_MAX_BYTES, PrivateTouchpadPollOutcome, PrivateTouchpadReceiveOutcome,
    PrivateTouchpadReceiver, PrivateTouchpadReceiverBuildError, PrivateTouchpadReceiverError,
    PrivateTouchpadReceiverLimits, PrivateTouchpadSink,
    receiver::validate_private_touchpad_receiver_binding,
};

pub const DEFAULT_PRIVATE_TOUCHPAD_QUEUE_PACKETS: u8 = 16;
pub const MAX_PRIVATE_TOUCHPAD_QUEUE_PACKETS: u8 = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadIngressLimits {
    pub queue_packets: u8,
    pub receiver: PrivateTouchpadReceiverLimits,
}

impl Default for PrivateTouchpadIngressLimits {
    fn default() -> Self {
        Self {
            queue_packets: DEFAULT_PRIVATE_TOUCHPAD_QUEUE_PACKETS,
            receiver: PrivateTouchpadReceiverLimits::default(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadRouteBinding {
    pub route_id: RouteId,
    pub session_id: SessionId,
    pub source: PortRef,
    pub sink: PortRef,
    pub route_epoch: u64,
    pub authorization_expires_at_ms: Option<u64>,
}

impl PrivateTouchpadRouteBinding {
    pub(crate) fn bind_active_source(
        route: &Route,
        expected_route_id: RouteId,
        expected_source: PortRef,
        stream_epoch: u64,
        now_ms: u64,
    ) -> Result<Self, PrivateTouchpadRouteBindingError> {
        validate_route_contract(route)?;
        if route.id != expected_route_id {
            return Err(PrivateTouchpadRouteBindingError::WrongRoute {
                expected: expected_route_id,
                actual: route.id,
            });
        }
        if route.source != expected_source {
            return Err(PrivateTouchpadRouteBindingError::WrongSource {
                expected: expected_source,
                actual: route.source,
            });
        }
        if route.state != RouteState::Active {
            return Err(PrivateTouchpadRouteBindingError::WrongState {
                expected: "active",
                actual: route.state,
            });
        }
        let authorization_expires_at_ms = validate_authorization(route.authorization, now_ms)?;
        if route.epoch != stream_epoch {
            return Err(PrivateTouchpadRouteBindingError::EpochMismatch {
                expected: route.epoch,
                actual: stream_epoch,
            });
        }
        Ok(Self {
            route_id: route.id,
            session_id: route.session_id,
            source: route.source,
            sink: route.sink,
            route_epoch: route.epoch,
            authorization_expires_at_ms,
        })
    }

    fn bind(
        route: &Route,
        expected_sink: PortRef,
        stream_epoch: u64,
        now_ms: u64,
    ) -> Result<Self, PrivateTouchpadRouteBindingError> {
        validate_route_contract(route)?;
        if route.sink != expected_sink {
            return Err(PrivateTouchpadRouteBindingError::WrongSink {
                expected: expected_sink,
                actual: route.sink,
            });
        }
        if !matches!(route.state, RouteState::Starting | RouteState::Active) {
            return Err(PrivateTouchpadRouteBindingError::WrongState {
                expected: "starting or active",
                actual: route.state,
            });
        }
        let authorization_expires_at_ms = validate_authorization(route.authorization, now_ms)?;
        if route.epoch != stream_epoch {
            return Err(PrivateTouchpadRouteBindingError::EpochMismatch {
                expected: route.epoch,
                actual: stream_epoch,
            });
        }
        Ok(Self {
            route_id: route.id,
            session_id: route.session_id,
            source: route.source,
            sink: route.sink,
            route_epoch: route.epoch,
            authorization_expires_at_ms,
        })
    }

    fn validate_active(
        &self,
        route: &Route,
        now_ms: u64,
    ) -> Result<(), PrivateTouchpadRouteBindingError> {
        self.validate_identity(route)?;
        if route.state != RouteState::Active {
            return Err(PrivateTouchpadRouteBindingError::WrongState {
                expected: "active",
                actual: route.state,
            });
        }
        let expires_at_ms = validate_authorization(route.authorization, now_ms)?;
        if expires_at_ms != self.authorization_expires_at_ms {
            return Err(
                PrivateTouchpadRouteBindingError::AuthorizationBindingChanged {
                    expected: self.authorization_expires_at_ms,
                    actual: expires_at_ms,
                },
            );
        }
        if route.epoch != self.route_epoch {
            return Err(PrivateTouchpadRouteBindingError::EpochMismatch {
                expected: self.route_epoch,
                actual: route.epoch,
            });
        }
        Ok(())
    }

    fn validate_later_starting_epoch(
        &self,
        route: &Route,
        now_ms: u64,
    ) -> Result<Option<u64>, PrivateTouchpadRouteBindingError> {
        self.validate_identity_without_epoch(route)?;
        if route.state != RouteState::Starting {
            return Err(PrivateTouchpadRouteBindingError::WrongState {
                expected: "starting",
                actual: route.state,
            });
        }
        let expires_at_ms = validate_authorization(route.authorization, now_ms)?;
        if route.epoch <= self.route_epoch {
            return Err(PrivateTouchpadRouteBindingError::NonAdvancingEpoch {
                current: self.route_epoch,
                new: route.epoch,
            });
        }
        Ok(expires_at_ms)
    }

    fn validate_identity(&self, route: &Route) -> Result<(), PrivateTouchpadRouteBindingError> {
        self.validate_identity_without_epoch(route)
    }

    fn validate_identity_without_epoch(
        &self,
        route: &Route,
    ) -> Result<(), PrivateTouchpadRouteBindingError> {
        validate_route_contract(route)?;
        if route.id != self.route_id {
            return Err(PrivateTouchpadRouteBindingError::WrongRoute {
                expected: self.route_id,
                actual: route.id,
            });
        }
        if route.session_id != self.session_id {
            return Err(PrivateTouchpadRouteBindingError::WrongSession {
                expected: self.session_id,
                actual: route.session_id,
            });
        }
        if route.source != self.source {
            return Err(PrivateTouchpadRouteBindingError::WrongSource {
                expected: self.source,
                actual: route.source,
            });
        }
        if route.sink != self.sink {
            return Err(PrivateTouchpadRouteBindingError::WrongSink {
                expected: self.sink,
                actual: route.sink,
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadRouteBindingError {
    WrongBackend(RouteBackend),
    WrongProfile(ProfileId),
    WrongRoute {
        expected: RouteId,
        actual: RouteId,
    },
    WrongSession {
        expected: SessionId,
        actual: SessionId,
    },
    WrongSource {
        expected: PortRef,
        actual: PortRef,
    },
    WrongSink {
        expected: PortRef,
        actual: PortRef,
    },
    WrongState {
        expected: &'static str,
        actual: RouteState,
    },
    NotAuthorized(AuthorizationState),
    AuthorizationExpired {
        expires_at_ms: u64,
        now_ms: u64,
    },
    AuthorizationBindingChanged {
        expected: Option<u64>,
        actual: Option<u64>,
    },
    EpochMismatch {
        expected: u64,
        actual: u64,
    },
    NonAdvancingEpoch {
        current: u64,
        new: u64,
    },
}

impl fmt::Display for PrivateTouchpadRouteBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::WrongBackend(actual) => write!(
                formatter,
                "private touchpad Route must use AdapterManaged, received {actual:?}"
            ),
            Self::WrongProfile(actual) => write!(
                formatter,
                "private touchpad Route must use capyio.input.touchpad-frames/1, received {}/{}",
                actual.name, actual.major
            ),
            Self::WrongRoute { expected, actual } => write!(
                formatter,
                "private touchpad Route changed from {expected} to {actual}"
            ),
            Self::WrongSession { expected, actual } => write!(
                formatter,
                "private touchpad Session changed from {expected} to {actual}"
            ),
            Self::WrongSource { expected, actual } => write!(
                formatter,
                "private touchpad Source changed from {expected:?} to {actual:?}"
            ),
            Self::WrongSink { expected, actual } => write!(
                formatter,
                "private touchpad Sink changed from {expected:?} to {actual:?}"
            ),
            Self::WrongState { expected, actual } => write!(
                formatter,
                "private touchpad Route must be {expected}, received {actual:?}"
            ),
            Self::NotAuthorized(actual) => write!(
                formatter,
                "private touchpad Route is not authorized: {actual:?}"
            ),
            Self::AuthorizationExpired {
                expires_at_ms,
                now_ms,
            } => write!(
                formatter,
                "private touchpad Route authorization expired at {expires_at_ms} ms; current time is {now_ms} ms"
            ),
            Self::AuthorizationBindingChanged { expected, actual } => write!(
                formatter,
                "private touchpad Route authorization expiry changed from {expected:?} to {actual:?}"
            ),
            Self::EpochMismatch { expected, actual } => write!(
                formatter,
                "private touchpad Route epoch is {expected}; received stream/Route epoch {actual}"
            ),
            Self::NonAdvancingEpoch { current, new } => write!(
                formatter,
                "private touchpad Route epoch {new} does not advance current epoch {current}"
            ),
        }
    }
}

impl Error for PrivateTouchpadRouteBindingError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadRouteSessionBuildError {
    InvalidQueuePackets { actual: u8, maximum: u8 },
    Binding(PrivateTouchpadRouteBindingError),
    Receiver(PrivateTouchpadReceiverBuildError),
}

impl fmt::Display for PrivateTouchpadRouteSessionBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidQueuePackets { actual, maximum } => write!(
                formatter,
                "private touchpad ingress queue must contain 1..={maximum} packets; received {actual}"
            ),
            Self::Binding(error) => write!(formatter, "invalid touchpad Route binding: {error}"),
            Self::Receiver(error) => write!(formatter, "invalid touchpad receiver: {error}"),
        }
    }
}

impl Error for PrivateTouchpadRouteSessionBuildError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidQueuePackets { .. } => None,
            Self::Binding(error) => Some(error),
            Self::Receiver(error) => Some(error),
        }
    }
}

impl From<PrivateTouchpadRouteBindingError> for PrivateTouchpadRouteSessionBuildError {
    fn from(error: PrivateTouchpadRouteBindingError) -> Self {
        Self::Binding(error)
    }
}

impl From<PrivateTouchpadReceiverBuildError> for PrivateTouchpadRouteSessionBuildError {
    fn from(error: PrivateTouchpadReceiverBuildError) -> Self {
        Self::Receiver(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadRouteSessionState {
    Starting,
    Active,
    TimedOut,
    Failed,
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadEnqueueOutcome {
    pub queued_packets: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadPumpOutcome {
    pub packets_processed: u8,
    pub last_receive: Option<PrivateTouchpadReceiveOutcome>,
    pub timeout: PrivateTouchpadPollOutcome,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadEpochAdvanceOutcome {
    pub previous_epoch: u64,
    pub new_epoch: u64,
    pub discarded_packets: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadRouteSessionFault {
    Binding(PrivateTouchpadRouteBindingError),
    QueueFull {
        capacity: u8,
    },
    QueuedPacketExpired {
        arrival_nanos: u64,
        now_nanos: u64,
        maximum_age_nanos: u64,
    },
    PacketTooLong {
        actual: usize,
        maximum: usize,
    },
    LocalClockRegression {
        previous: u64,
        actual: u64,
    },
}

impl fmt::Display for PrivateTouchpadRouteSessionFault {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Binding(error) => error.fmt(formatter),
            Self::QueueFull { capacity } => write!(
                formatter,
                "private touchpad ingress queue is full at {capacity} packets"
            ),
            Self::QueuedPacketExpired {
                arrival_nanos,
                now_nanos,
                maximum_age_nanos,
            } => write!(
                formatter,
                "private touchpad queued packet arrived at {arrival_nanos} ns and was not pumped by {now_nanos} ns within the {maximum_age_nanos} ns bound"
            ),
            Self::PacketTooLong { actual, maximum } => write!(
                formatter,
                "private touchpad ingress packet is {actual} bytes; maximum is {maximum}"
            ),
            Self::LocalClockRegression { previous, actual } => write!(
                formatter,
                "private touchpad ingress clock regressed from {previous} to {actual} ns"
            ),
        }
    }
}

impl Error for PrivateTouchpadRouteSessionFault {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Binding(error) => Some(error),
            Self::QueueFull { .. }
            | Self::QueuedPacketExpired { .. }
            | Self::PacketTooLong { .. }
            | Self::LocalClockRegression { .. } => None,
        }
    }
}

#[derive(Debug)]
pub enum PrivateTouchpadRouteSessionError<E> {
    Inactive(PrivateTouchpadRouteSessionState),
    Fault {
        fault: PrivateTouchpadRouteSessionFault,
        cleanup: Option<Box<PrivateTouchpadReceiverError<E>>>,
    },
    Receiver(PrivateTouchpadReceiverError<E>),
}

impl<E: fmt::Display> fmt::Display for PrivateTouchpadRouteSessionError<E> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Inactive(state) => {
                write!(
                    formatter,
                    "private touchpad Route session is not active: {state:?}"
                )
            }
            Self::Fault {
                fault,
                cleanup: None,
            } => fault.fmt(formatter),
            Self::Fault {
                fault,
                cleanup: Some(cleanup),
            } => write!(
                formatter,
                "{fault}; receiver cleanup also failed: {cleanup}"
            ),
            Self::Receiver(error) => error.fmt(formatter),
        }
    }
}

impl<E: Error + 'static> Error for PrivateTouchpadRouteSessionError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Inactive(_) => None,
            Self::Fault { fault, .. } => Some(fault),
            Self::Receiver(error) => Some(error),
        }
    }
}

#[derive(Clone)]
struct PrivateTouchpadQueuedPacket {
    bytes: [u8; PRIVATE_TOUCHPAD_PACKET_MAX_BYTES],
    len: u16,
    arrival_nanos: u64,
}

impl PrivateTouchpadQueuedPacket {
    fn new(packet: &[u8], arrival_nanos: u64) -> Self {
        debug_assert!(packet.len() <= PRIVATE_TOUCHPAD_PACKET_MAX_BYTES);
        let mut bytes = [0; PRIVATE_TOUCHPAD_PACKET_MAX_BYTES];
        bytes[..packet.len()].copy_from_slice(packet);
        Self {
            bytes,
            len: packet.len() as u16,
            arrival_nanos,
        }
    }

    fn bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

pub struct PrivateTouchpadRouteSession<S: PrivateTouchpadSink> {
    binding: PrivateTouchpadRouteBinding,
    receiver: PrivateTouchpadReceiver<S>,
    queue: VecDeque<PrivateTouchpadQueuedPacket>,
    queue_capacity: u8,
    maximum_queue_age_nanos: u64,
    state: PrivateTouchpadRouteSessionState,
    last_local_clock_nanos: Option<u64>,
}

impl<S: PrivateTouchpadSink> PrivateTouchpadRouteSession<S> {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        route: &Route,
        expected_sink: PortRef,
        stream: InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
        now_ms: u64,
        limits: PrivateTouchpadIngressLimits,
        sink: S,
    ) -> Result<Self, PrivateTouchpadRouteSessionBuildError> {
        let binding = validate_private_touchpad_route_session_construction(
            route,
            expected_sink,
            &stream,
            descriptor,
            first_sequence,
            now_ms,
            limits,
        )?;
        let receiver = PrivateTouchpadReceiver::new(
            stream,
            descriptor,
            first_sequence,
            limits.receiver,
            sink,
        )?;
        let state = match route.state {
            RouteState::Starting => PrivateTouchpadRouteSessionState::Starting,
            RouteState::Active => PrivateTouchpadRouteSessionState::Active,
            _ => unreachable!("binding validated live Route state"),
        };
        Ok(Self {
            binding,
            receiver,
            queue: VecDeque::with_capacity(usize::from(limits.queue_packets)),
            queue_capacity: limits.queue_packets,
            maximum_queue_age_nanos: limits.receiver.active_idle_timeout_nanos,
            state,
            last_local_clock_nanos: None,
        })
    }

    #[must_use]
    pub const fn state(&self) -> PrivateTouchpadRouteSessionState {
        self.state
    }

    #[must_use]
    pub const fn binding(&self) -> &PrivateTouchpadRouteBinding {
        &self.binding
    }

    #[must_use]
    pub fn queued_packets(&self) -> u8 {
        self.queue.len() as u8
    }

    pub fn activate(
        &mut self,
        route: &Route,
        now_ms: u64,
    ) -> Result<(), PrivateTouchpadRouteSessionError<S::Error>> {
        if self.state == PrivateTouchpadRouteSessionState::Active {
            return self.validate_active_route(route, now_ms);
        }
        if self.state != PrivateTouchpadRouteSessionState::Starting {
            return Err(PrivateTouchpadRouteSessionError::Inactive(self.state));
        }
        self.validate_active_route(route, now_ms)?;
        self.state = PrivateTouchpadRouteSessionState::Active;
        Ok(())
    }

    pub fn enqueue(
        &mut self,
        route: &Route,
        now_ms: u64,
        packet: &[u8],
        arrival_nanos: u64,
    ) -> Result<PrivateTouchpadEnqueueOutcome, PrivateTouchpadRouteSessionError<S::Error>> {
        self.require_active()?;
        self.validate_active_route(route, now_ms)?;
        self.observe_local_clock(arrival_nanos)?;
        if packet.len() > PRIVATE_TOUCHPAD_PACKET_MAX_BYTES {
            return self.fail_fault(PrivateTouchpadRouteSessionFault::PacketTooLong {
                actual: packet.len(),
                maximum: PRIVATE_TOUCHPAD_PACKET_MAX_BYTES,
            });
        }
        if self.queue.len() >= usize::from(self.queue_capacity) {
            return self.fail_fault(PrivateTouchpadRouteSessionFault::QueueFull {
                capacity: self.queue_capacity,
            });
        }
        self.queue
            .push_back(PrivateTouchpadQueuedPacket::new(packet, arrival_nanos));
        Ok(PrivateTouchpadEnqueueOutcome {
            queued_packets: self.queued_packets(),
        })
    }

    pub fn pump(
        &mut self,
        route: &Route,
        now_ms: u64,
        now_nanos: u64,
    ) -> Result<PrivateTouchpadPumpOutcome, PrivateTouchpadRouteSessionError<S::Error>> {
        self.require_active()?;
        self.validate_active_route(route, now_ms)?;
        self.observe_local_clock(now_nanos)?;

        let mut packets_processed = 0_u8;
        let mut last_receive = None;
        while let Some(queued) = self.queue.pop_front() {
            if now_nanos.saturating_sub(queued.arrival_nanos) >= self.maximum_queue_age_nanos {
                return self.fail_fault(PrivateTouchpadRouteSessionFault::QueuedPacketExpired {
                    arrival_nanos: queued.arrival_nanos,
                    now_nanos,
                    maximum_age_nanos: self.maximum_queue_age_nanos,
                });
            }
            match self.receiver.receive(queued.bytes(), queued.arrival_nanos) {
                Ok(outcome) => {
                    packets_processed += 1;
                    last_receive = Some(outcome);
                }
                Err(error) => {
                    self.queue.clear();
                    self.state = PrivateTouchpadRouteSessionState::Failed;
                    return Err(PrivateTouchpadRouteSessionError::Receiver(error));
                }
            }
        }

        let timeout = match self.receiver.poll_timeout(now_nanos) {
            Ok(outcome) => outcome,
            Err(error) => {
                self.state = PrivateTouchpadRouteSessionState::Failed;
                return Err(PrivateTouchpadRouteSessionError::Receiver(error));
            }
        };
        if timeout == PrivateTouchpadPollOutcome::TimedOut {
            self.state = PrivateTouchpadRouteSessionState::TimedOut;
        }
        Ok(PrivateTouchpadPumpOutcome {
            packets_processed,
            last_receive,
            timeout,
        })
    }

    pub fn advance_epoch(
        &mut self,
        route: &Route,
        first_sequence: u64,
        now_ms: u64,
        now_nanos: u64,
    ) -> Result<PrivateTouchpadEpochAdvanceOutcome, PrivateTouchpadRouteSessionError<S::Error>>
    {
        if !matches!(
            self.state,
            PrivateTouchpadRouteSessionState::Starting | PrivateTouchpadRouteSessionState::Active
        ) {
            return Err(PrivateTouchpadRouteSessionError::Inactive(self.state));
        }
        self.observe_local_clock(now_nanos)?;
        let expires_at_ms = match self.binding.validate_later_starting_epoch(route, now_ms) {
            Ok(expires_at_ms) => expires_at_ms,
            Err(error) => {
                return self.fail_fault(PrivateTouchpadRouteSessionFault::Binding(error));
            }
        };
        let previous_epoch = self.binding.route_epoch;
        let discarded_packets = self.queued_packets();
        self.queue.clear();
        if let Err(error) = self.receiver.advance_epoch(route.epoch, first_sequence) {
            self.state = PrivateTouchpadRouteSessionState::Failed;
            return Err(PrivateTouchpadRouteSessionError::Receiver(error));
        }
        self.binding.route_epoch = route.epoch;
        self.binding.authorization_expires_at_ms = expires_at_ms;
        self.state = PrivateTouchpadRouteSessionState::Starting;
        Ok(PrivateTouchpadEpochAdvanceOutcome {
            previous_epoch,
            new_epoch: route.epoch,
            discarded_packets,
        })
    }

    pub fn disconnect(&mut self) -> Result<(), PrivateTouchpadRouteSessionError<S::Error>> {
        if !matches!(
            self.state,
            PrivateTouchpadRouteSessionState::Starting | PrivateTouchpadRouteSessionState::Active
        ) {
            return Err(PrivateTouchpadRouteSessionError::Inactive(self.state));
        }
        self.queue.clear();
        match self.receiver.disconnect() {
            Ok(()) => {
                self.state = PrivateTouchpadRouteSessionState::Closed;
                Ok(())
            }
            Err(error) => {
                self.state = PrivateTouchpadRouteSessionState::Failed;
                Err(PrivateTouchpadRouteSessionError::Receiver(error))
            }
        }
    }

    pub(crate) fn fail_closed(&mut self) -> Option<Box<PrivateTouchpadReceiverError<S::Error>>> {
        self.queue.clear();
        let cleanup = self.receiver.disconnect().err().map(Box::new);
        self.state = PrivateTouchpadRouteSessionState::Failed;
        cleanup
    }

    fn require_active(&self) -> Result<(), PrivateTouchpadRouteSessionError<S::Error>> {
        if self.state == PrivateTouchpadRouteSessionState::Active {
            Ok(())
        } else {
            Err(PrivateTouchpadRouteSessionError::Inactive(self.state))
        }
    }

    fn validate_active_route(
        &mut self,
        route: &Route,
        now_ms: u64,
    ) -> Result<(), PrivateTouchpadRouteSessionError<S::Error>> {
        match self.binding.validate_active(route, now_ms) {
            Ok(()) => Ok(()),
            Err(error) => self.fail_fault(PrivateTouchpadRouteSessionFault::Binding(error)),
        }
    }

    fn observe_local_clock(
        &mut self,
        actual: u64,
    ) -> Result<(), PrivateTouchpadRouteSessionError<S::Error>> {
        if let Some(previous) = self.last_local_clock_nanos
            && actual < previous
        {
            return self.fail_fault(PrivateTouchpadRouteSessionFault::LocalClockRegression {
                previous,
                actual,
            });
        }
        self.last_local_clock_nanos = Some(actual);
        Ok(())
    }

    fn fail_fault<T>(
        &mut self,
        fault: PrivateTouchpadRouteSessionFault,
    ) -> Result<T, PrivateTouchpadRouteSessionError<S::Error>> {
        let cleanup = self.fail_closed();
        Err(PrivateTouchpadRouteSessionError::Fault { fault, cleanup })
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn validate_private_touchpad_route_session_construction(
    route: &Route,
    expected_sink: PortRef,
    stream: &InputStreamDescriptor,
    descriptor: TouchpadDescriptor,
    first_sequence: u64,
    now_ms: u64,
    limits: PrivateTouchpadIngressLimits,
) -> Result<PrivateTouchpadRouteBinding, PrivateTouchpadRouteSessionBuildError> {
    if limits.queue_packets == 0 || limits.queue_packets > MAX_PRIVATE_TOUCHPAD_QUEUE_PACKETS {
        return Err(PrivateTouchpadRouteSessionBuildError::InvalidQueuePackets {
            actual: limits.queue_packets,
            maximum: MAX_PRIVATE_TOUCHPAD_QUEUE_PACKETS,
        });
    }
    let binding =
        PrivateTouchpadRouteBinding::bind(route, expected_sink, stream.stream_epoch, now_ms)?;
    validate_private_touchpad_receiver_binding(
        stream,
        descriptor,
        first_sequence,
        limits.receiver,
    )?;
    Ok(binding)
}

fn validate_route_contract(route: &Route) -> Result<(), PrivateTouchpadRouteBindingError> {
    if route.backend != RouteBackend::AdapterManaged {
        return Err(PrivateTouchpadRouteBindingError::WrongBackend(
            route.backend,
        ));
    }
    if route.profile != touchpad_frames_profile() {
        return Err(PrivateTouchpadRouteBindingError::WrongProfile(
            route.profile.clone(),
        ));
    }
    Ok(())
}

fn validate_authorization(
    authorization: AuthorizationState,
    now_ms: u64,
) -> Result<Option<u64>, PrivateTouchpadRouteBindingError> {
    match authorization {
        AuthorizationState::Authorized {
            expires_at_ms: Some(expires_at_ms),
        } if expires_at_ms <= now_ms => {
            Err(PrivateTouchpadRouteBindingError::AuthorizationExpired {
                expires_at_ms,
                now_ms,
            })
        }
        AuthorizationState::Authorized { expires_at_ms } => Ok(expires_at_ms),
        actual => Err(PrivateTouchpadRouteBindingError::NotAuthorized(actual)),
    }
}
