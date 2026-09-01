#![forbid(unsafe_code)]

//! Deterministic `touch-events/1` snapshot to `pointer-events/1` conversion.
//!
//! This crate owns only a bounded semantic conversion state machine. It does
//! not open a socket, capture hardware input, inject operating-system input,
//! implement HID/VIIPER, or recognize multi-finger gestures.

mod bounded_channel;
mod delivery;
mod delivery_worker;
mod ingress;
mod receiver;
mod source;
mod transport_receiver;
mod transport_record;
mod wire;
mod worker;

pub use ingress::{
    DEFAULT_PRIVATE_TOUCHPAD_QUEUE_PACKETS, MAX_PRIVATE_TOUCHPAD_QUEUE_PACKETS,
    PrivateTouchpadEnqueueOutcome, PrivateTouchpadEpochAdvanceOutcome,
    PrivateTouchpadIngressLimits, PrivateTouchpadPumpOutcome, PrivateTouchpadRouteBinding,
    PrivateTouchpadRouteBindingError, PrivateTouchpadRouteSession,
    PrivateTouchpadRouteSessionBuildError, PrivateTouchpadRouteSessionError,
    PrivateTouchpadRouteSessionFault, PrivateTouchpadRouteSessionState,
};

pub use bounded_channel::{
    MAX_PRIVATE_TOUCHPAD_HOST_CHANNEL_PACKETS, PrivateTouchpadHostChannel,
    PrivateTouchpadHostChannelAdmission, PrivateTouchpadHostChannelBuildError,
    PrivateTouchpadHostChannelMetrics, PrivateTouchpadHostChannelReceiveOutcome,
    PrivateTouchpadHostChannelReceiver, private_touchpad_host_channel,
};

pub use delivery::{
    PrivateTouchpadAdmittedChannel, PrivateTouchpadBindingMismatch,
    PrivateTouchpadChannelAdmissionError, PrivateTouchpadChannelSendOutcome,
    PrivateTouchpadDeliveryBuildError, PrivateTouchpadDeliveryError,
    PrivateTouchpadDeliveryMetrics, PrivateTouchpadDeliverySession, PrivateTouchpadDeliveryState,
};
pub use delivery_worker::{
    PrivateTouchpadRuntimeDeliveryBuildError, PrivateTouchpadRuntimeDeliveryError,
    PrivateTouchpadRuntimeDeliveryMetrics, PrivateTouchpadRuntimeDeliveryWorker,
};

pub use receiver::{
    DEFAULT_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS, DEFAULT_PRIVATE_TOUCHPAD_PACKETS_PER_SECOND,
    MAX_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS, MAX_PRIVATE_TOUCHPAD_PACKETS_PER_SECOND,
    MIN_PRIVATE_TOUCHPAD_IDLE_TIMEOUT_NANOS, PrivateTouchpadPollOutcome,
    PrivateTouchpadReceiveOutcome, PrivateTouchpadReceiver, PrivateTouchpadReceiverBuildError,
    PrivateTouchpadReceiverError, PrivateTouchpadReceiverFault, PrivateTouchpadReceiverLimits,
    PrivateTouchpadReceiverState, PrivateTouchpadSink,
};

pub use source::{
    PrivateTouchpadPacketSource, PrivateTouchpadPacketSourceError, PrivateTouchpadPacketSourceState,
};

pub use transport_receiver::{
    PrivateTouchpadTransportReceiveOutcome, PrivateTouchpadTransportReceiver,
    PrivateTouchpadTransportReceiverBuildError, PrivateTouchpadTransportReceiverError,
    PrivateTouchpadTransportReceiverState,
};
pub use transport_record::{
    PRIVATE_TOUCHPAD_TRANSPORT_ACK_BYTES, PRIVATE_TOUCHPAD_TRANSPORT_CLOSE_BYTES,
    PRIVATE_TOUCHPAD_TRANSPORT_HEADER_BYTES, PRIVATE_TOUCHPAD_TRANSPORT_HELLO_BYTES,
    PRIVATE_TOUCHPAD_TRANSPORT_MAGIC, PRIVATE_TOUCHPAD_TRANSPORT_MAX_BYTES,
    PRIVATE_TOUCHPAD_TRANSPORT_VERSION, PrivateTouchpadTransportCodecV1,
    PrivateTouchpadTransportPacketV1, PrivateTouchpadTransportRecordError,
    PrivateTouchpadTransportRecordV1,
};

pub use wire::{
    PRIVATE_TOUCHPAD_PACKET_HEADER_BYTES, PRIVATE_TOUCHPAD_PACKET_MAGIC,
    PRIVATE_TOUCHPAD_PACKET_MAX_BYTES, PRIVATE_TOUCHPAD_PACKET_RECORD_BYTES,
    PRIVATE_TOUCHPAD_PACKET_VERSION, PrivateTouchpadPacketCodecV1, PrivateTouchpadPacketError,
    PrivateTouchpadPacketV1,
};

pub use worker::{
    PrivateTouchpadClockSample, PrivateTouchpadMonotonicClock, PrivateTouchpadRouteProvider,
    PrivateTouchpadRuntimeWorker, PrivateTouchpadRuntimeWorkerBuildError,
    PrivateTouchpadRuntimeWorkerError, PrivateTouchpadRuntimeWorkerFactoryBuildError,
    PrivateTouchpadRuntimeWorkerMetrics, PrivateTouchpadSinkFactory,
};
#[cfg(windows)]
pub use worker::{WindowsSyntheticTouchpadSinkFactory, WindowsVhfTouchpadSinkFactory};

use std::{error::Error, fmt};

use capyio_input::{
    InputContractError, InputFrameHeader, InputSequenceOutcome, InputSequenceTracker,
    InputStreamDescriptor, NormalizedPosition, PointerButton, PointerButtonPhase, PointerEvent,
    PointerFrame, TouchContact, TouchFrame,
};

/// Maximum Pointer frames emitted for one accepted Touch snapshot.
pub const MAX_OUTPUT_FRAMES_PER_INPUT: usize = 2;

/// A contact released inside this duration and slop is a primary-button click.
pub const TAP_MAX_DURATION_NANOS: u64 = 250_000_000;

/// A stationary contact held this long enters primary-button drag mode.
pub const DRAG_HOLD_DURATION_NANOS: u64 = 500_000_000;

/// Maximum per-axis displacement from the initial contact position for tap/hold.
pub const TAP_SLOP_UNITS: u16 = 512;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TouchpadConversionError {
    Input(InputContractError),
    SharedStreamId,
    ClockDomainMismatch,
    NonAdvancingOutputEpoch { current_epoch: u64, new_epoch: u64 },
    OutputSequenceExhausted,
}

impl fmt::Display for TouchpadConversionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => error.fmt(formatter),
            Self::SharedStreamId => {
                formatter.write_str("touch and pointer streams must use different StreamIds")
            }
            Self::ClockDomainMismatch => formatter
                .write_str("touch and pointer streams must use the same source clock domain"),
            Self::NonAdvancingOutputEpoch {
                current_epoch,
                new_epoch,
            } => write!(
                formatter,
                "new pointer epoch {new_epoch} does not advance current epoch {current_epoch}"
            ),
            Self::OutputSequenceExhausted => {
                formatter.write_str("pointer output sequence is exhausted for the current epoch")
            }
        }
    }
}

impl Error for TouchpadConversionError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InputContractError> for TouchpadConversionError {
    fn from(value: InputContractError) -> Self {
        Self::Input(value)
    }
}

/// Bounded result of converting one Touch snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PointerConversion {
    None,
    One(PointerFrame),
    Two(PointerFrame, PointerFrame),
}

impl PointerConversion {
    #[must_use]
    pub const fn len(&self) -> usize {
        match self {
            Self::None => 0,
            Self::One(_) => 1,
            Self::Two(_, _) => 2,
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        matches!(self, Self::None)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ContactState {
    Idle,
    Tracking {
        contact_id: u16,
        start_position: NormalizedPosition,
        last_position: NormalizedPosition,
        started_at_nanos: u64,
        last_timestamp_nanos: u64,
        exceeded_tap_slop: bool,
        dragging: bool,
    },
    /// A gap or ambiguous contact set was reset. Ignore contacts until empty.
    SuppressedUntilEmpty,
}

/// Fixed-state converter for one Touch Sink stream and one Pointer Source stream.
///
/// Output relative motion uses one normalized-coordinate least-significant bit
/// as one semantic pointer count. Sensitivity and OS scaling remain Projection
/// policy. The converter is transactional: an error never advances sequence or
/// gesture state.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TouchpadConverter {
    input_sequence: InputSequenceTracker,
    output_stream: InputStreamDescriptor,
    next_output_sequence: Option<u64>,
    contact_state: ContactState,
}

impl TouchpadConverter {
    pub fn new(
        input_stream: InputStreamDescriptor,
        first_input_sequence: u64,
        output_stream: InputStreamDescriptor,
        first_output_sequence: u64,
    ) -> Result<Self, TouchpadConversionError> {
        input_stream.validate()?;
        output_stream.validate()?;
        if input_stream.stream_id == output_stream.stream_id {
            return Err(TouchpadConversionError::SharedStreamId);
        }
        if input_stream.clock_domain_id != output_stream.clock_domain_id {
            return Err(TouchpadConversionError::ClockDomainMismatch);
        }

        Ok(Self {
            input_sequence: InputSequenceTracker::new(
                input_stream.stream_id,
                input_stream.stream_epoch,
                first_input_sequence,
            )?,
            output_stream,
            next_output_sequence: Some(first_output_sequence),
            contact_state: ContactState::Idle,
        })
    }

    /// Converts one complete active-contact snapshot.
    ///
    /// A sequence gap emits one Reset and suppresses the gap-causing contact
    /// state until an empty snapshot arrives. Empty snapshots always emit Reset.
    pub fn convert(
        &mut self,
        frame: &TouchFrame,
    ) -> Result<PointerConversion, TouchpadConversionError> {
        let mut candidate = self.clone();
        let output = candidate.convert_inner(frame)?;
        *self = candidate;
        Ok(output)
    }

    /// Advances both sides of the converter to fresh epochs and emits Reset in
    /// the new Pointer epoch. The supplied timestamp belongs to the unchanged
    /// source clock domain.
    pub fn advance_epoch(
        &mut self,
        new_input_epoch: u64,
        first_input_sequence: u64,
        new_output_epoch: u64,
        first_output_sequence: u64,
        source_timestamp_nanos: u64,
    ) -> Result<PointerFrame, TouchpadConversionError> {
        let mut candidate = self.clone();
        if new_output_epoch <= candidate.output_stream.stream_epoch {
            return Err(TouchpadConversionError::NonAdvancingOutputEpoch {
                current_epoch: candidate.output_stream.stream_epoch,
                new_epoch: new_output_epoch,
            });
        }
        candidate
            .input_sequence
            .advance_epoch(new_input_epoch, first_input_sequence)?;
        candidate.output_stream.stream_epoch = new_output_epoch;
        candidate.next_output_sequence = Some(first_output_sequence);
        candidate.contact_state = ContactState::Idle;
        let reset = candidate.emit_frame(source_timestamp_nanos, vec![PointerEvent::Reset])?;
        *self = candidate;
        Ok(reset)
    }

    /// Lifecycle fail-safe for Route stop/offline, Adapter failure or peer loss.
    pub fn reset(
        &mut self,
        source_timestamp_nanos: u64,
    ) -> Result<PointerFrame, TouchpadConversionError> {
        let mut candidate = self.clone();
        candidate.contact_state = ContactState::Idle;
        let reset = candidate.emit_frame(source_timestamp_nanos, vec![PointerEvent::Reset])?;
        *self = candidate;
        Ok(reset)
    }

    fn convert_inner(
        &mut self,
        frame: &TouchFrame,
    ) -> Result<PointerConversion, TouchpadConversionError> {
        frame.validate()?;
        if matches!(
            self.input_sequence.observe(frame.header)?,
            InputSequenceOutcome::Gap(_)
        ) {
            self.contact_state = if frame.contacts.is_empty() {
                ContactState::Idle
            } else {
                ContactState::SuppressedUntilEmpty
            };
            return self.emit_reset(frame.header.source_timestamp_nanos);
        }

        match frame.contacts.as_slice() {
            [] => self.release_or_reset(frame.header),
            [contact] => self.accept_single_contact(frame.header, *contact),
            _ => {
                self.contact_state = ContactState::SuppressedUntilEmpty;
                self.emit_reset(frame.header.source_timestamp_nanos)
            }
        }
    }

    fn accept_single_contact(
        &mut self,
        header: InputFrameHeader,
        contact: TouchContact,
    ) -> Result<PointerConversion, TouchpadConversionError> {
        let ContactState::Tracking {
            contact_id,
            start_position,
            last_position,
            started_at_nanos,
            last_timestamp_nanos,
            mut exceeded_tap_slop,
            mut dragging,
        } = self.contact_state
        else {
            return match self.contact_state {
                ContactState::Idle => {
                    self.contact_state = ContactState::Tracking {
                        contact_id: contact.contact_id,
                        start_position: contact.position,
                        last_position: contact.position,
                        started_at_nanos: header.source_timestamp_nanos,
                        last_timestamp_nanos: header.source_timestamp_nanos,
                        exceeded_tap_slop: false,
                        dragging: false,
                    };
                    Ok(PointerConversion::None)
                }
                ContactState::SuppressedUntilEmpty => Ok(PointerConversion::None),
                ContactState::Tracking { .. } => unreachable!("matched above"),
            };
        };

        if contact.contact_id != contact_id || header.source_timestamp_nanos < last_timestamp_nanos
        {
            self.contact_state = ContactState::SuppressedUntilEmpty;
            return self.emit_reset(header.source_timestamp_nanos);
        }

        let delta_x = i32::from(contact.position.x) - i32::from(last_position.x);
        let delta_y = i32::from(contact.position.y) - i32::from(last_position.y);
        exceeded_tap_slop |= outside_tap_slop(start_position, contact.position);
        let elapsed_nanos = header.source_timestamp_nanos - started_at_nanos;

        let mut events = Vec::with_capacity(2);
        if !dragging && !exceeded_tap_slop && elapsed_nanos >= DRAG_HOLD_DURATION_NANOS {
            dragging = true;
            events.push(PointerEvent::Button {
                button: PointerButton::Left,
                phase: PointerButtonPhase::Pressed,
            });
        }
        if delta_x != 0 || delta_y != 0 {
            events.push(PointerEvent::RelativeMotion { delta_x, delta_y });
        }

        self.contact_state = ContactState::Tracking {
            contact_id,
            start_position,
            last_position: contact.position,
            started_at_nanos,
            last_timestamp_nanos: header.source_timestamp_nanos,
            exceeded_tap_slop,
            dragging,
        };

        if events.is_empty() {
            Ok(PointerConversion::None)
        } else {
            let pointer = self.emit_frame(header.source_timestamp_nanos, events)?;
            Ok(PointerConversion::One(pointer))
        }
    }

    fn release_or_reset(
        &mut self,
        header: InputFrameHeader,
    ) -> Result<PointerConversion, TouchpadConversionError> {
        let previous = self.contact_state;
        self.contact_state = ContactState::Idle;

        match previous {
            ContactState::Tracking {
                started_at_nanos,
                last_timestamp_nanos,
                exceeded_tap_slop,
                dragging,
                ..
            } if header.source_timestamp_nanos >= last_timestamp_nanos => {
                let elapsed_nanos = header.source_timestamp_nanos - started_at_nanos;
                if dragging {
                    let release = self.emit_frame(
                        header.source_timestamp_nanos,
                        vec![PointerEvent::Button {
                            button: PointerButton::Left,
                            phase: PointerButtonPhase::Released,
                        }],
                    )?;
                    let reset =
                        self.emit_frame(header.source_timestamp_nanos, vec![PointerEvent::Reset])?;
                    Ok(PointerConversion::Two(release, reset))
                } else if !exceeded_tap_slop && elapsed_nanos <= TAP_MAX_DURATION_NANOS {
                    let click = self.emit_frame(
                        header.source_timestamp_nanos,
                        vec![
                            PointerEvent::Button {
                                button: PointerButton::Left,
                                phase: PointerButtonPhase::Pressed,
                            },
                            PointerEvent::Button {
                                button: PointerButton::Left,
                                phase: PointerButtonPhase::Released,
                            },
                        ],
                    )?;
                    let reset =
                        self.emit_frame(header.source_timestamp_nanos, vec![PointerEvent::Reset])?;
                    Ok(PointerConversion::Two(click, reset))
                } else {
                    self.emit_reset(header.source_timestamp_nanos)
                }
            }
            _ => self.emit_reset(header.source_timestamp_nanos),
        }
    }

    fn emit_reset(
        &mut self,
        source_timestamp_nanos: u64,
    ) -> Result<PointerConversion, TouchpadConversionError> {
        Ok(PointerConversion::One(self.emit_frame(
            source_timestamp_nanos,
            vec![PointerEvent::Reset],
        )?))
    }

    fn emit_frame(
        &mut self,
        source_timestamp_nanos: u64,
        events: Vec<PointerEvent>,
    ) -> Result<PointerFrame, TouchpadConversionError> {
        debug_assert!(!events.is_empty() && events.len() <= 2);
        let sequence = self
            .next_output_sequence
            .ok_or(TouchpadConversionError::OutputSequenceExhausted)?;
        self.next_output_sequence = sequence.checked_add(1);
        let frame = PointerFrame {
            header: InputFrameHeader {
                stream_id: self.output_stream.stream_id,
                stream_epoch: self.output_stream.stream_epoch,
                sequence,
                source_timestamp_nanos,
            },
            events,
        };
        frame.validate()?;
        Ok(frame)
    }
}

fn outside_tap_slop(origin: NormalizedPosition, current: NormalizedPosition) -> bool {
    origin.x.abs_diff(current.x) > TAP_SLOP_UNITS || origin.y.abs_diff(current.y) > TAP_SLOP_UNITS
}
