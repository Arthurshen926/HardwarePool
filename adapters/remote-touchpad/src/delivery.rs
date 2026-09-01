use std::{error::Error, fmt};

use capyio_core::{PortRef, RouteId, SessionId};
use capyio_input::TouchpadFrame;

use crate::{
    PrivateTouchpadPacketSource, PrivateTouchpadPacketSourceError, PrivateTouchpadPacketV1,
    PrivateTouchpadRouteBinding,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadChannelAdmissionError {
    Unavailable,
    Expired,
    Revoked,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadChannelSendOutcome {
    Delivered,
    RejectedBeforeWrite,
    DeliveryUnknown,
}

pub trait PrivateTouchpadAdmittedChannel {
    fn current_binding(
        &self,
    ) -> Result<PrivateTouchpadRouteBinding, PrivateTouchpadChannelAdmissionError>;

    fn send(&mut self, packet: &PrivateTouchpadPacketV1) -> PrivateTouchpadChannelSendOutcome;

    fn close(&mut self);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadDeliveryState {
    Active,
    Faulted,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct PrivateTouchpadDeliveryMetrics {
    pub attempts: u64,
    pub packets_delivered: u64,
    pub rejected_before_write: u64,
    pub delivery_unknown: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadBindingMismatch {
    Route {
        expected: RouteId,
        actual: RouteId,
    },
    Session {
        expected: SessionId,
        actual: SessionId,
    },
    Source {
        expected: PortRef,
        actual: PortRef,
    },
    Sink {
        expected: PortRef,
        actual: PortRef,
    },
    Epoch {
        expected: u64,
        actual: u64,
    },
    AuthorizationExpiry {
        expected: Option<u64>,
        actual: Option<u64>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadDeliveryBuildError {
    Admission(PrivateTouchpadChannelAdmissionError),
    BindingMismatch(PrivateTouchpadBindingMismatch),
    EpochMismatch {
        binding_epoch: u64,
        source_epoch: u64,
    },
}

impl fmt::Display for PrivateTouchpadDeliveryBuildError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Admission(error) => {
                write!(formatter, "touchpad channel is not admitted: {error:?}")
            }
            Self::BindingMismatch(_) => formatter
                .write_str("touchpad channel binding does not match the Runtime-provided binding"),
            Self::EpochMismatch {
                binding_epoch,
                source_epoch,
            } => write!(
                formatter,
                "touchpad channel Route epoch {binding_epoch} does not match Source epoch {source_epoch}"
            ),
        }
    }
}

impl Error for PrivateTouchpadDeliveryBuildError {}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadDeliveryError {
    Source(PrivateTouchpadPacketSourceError),
    Admission(PrivateTouchpadChannelAdmissionError),
    BindingChanged(PrivateTouchpadBindingMismatch),
    RejectedBeforeWrite,
    DeliveryUnknown,
    Faulted,
    Closed,
}

impl fmt::Display for PrivateTouchpadDeliveryError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source(error) => write!(formatter, "touchpad packet Source failed: {error}"),
            Self::Admission(error) => {
                write!(formatter, "touchpad channel admission failed: {error:?}")
            }
            Self::BindingChanged(_) => {
                formatter.write_str("touchpad channel binding changed after admission")
            }
            Self::RejectedBeforeWrite => {
                formatter.write_str("touchpad packet was rejected before any channel write")
            }
            Self::DeliveryUnknown => {
                formatter.write_str("touchpad packet delivery result is unknown")
            }
            Self::Faulted => formatter.write_str("touchpad delivery session is faulted"),
            Self::Closed => formatter.write_str("touchpad delivery session is closed"),
        }
    }
}

impl Error for PrivateTouchpadDeliveryError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Source(error) => Some(error),
            _ => None,
        }
    }
}

impl From<PrivateTouchpadPacketSourceError> for PrivateTouchpadDeliveryError {
    fn from(value: PrivateTouchpadPacketSourceError) -> Self {
        Self::Source(value)
    }
}

pub struct PrivateTouchpadDeliverySession<C: PrivateTouchpadAdmittedChannel> {
    expected_binding: PrivateTouchpadRouteBinding,
    source: PrivateTouchpadPacketSource,
    channel: C,
    channel_closed: bool,
    state: PrivateTouchpadDeliveryState,
    metrics: PrivateTouchpadDeliveryMetrics,
}

impl<C: PrivateTouchpadAdmittedChannel> PrivateTouchpadDeliverySession<C> {
    pub fn new(
        expected_binding: PrivateTouchpadRouteBinding,
        source: PrivateTouchpadPacketSource,
        mut channel: C,
    ) -> Result<Self, PrivateTouchpadDeliveryBuildError> {
        if expected_binding.route_epoch != source.epoch() {
            channel.close();
            return Err(PrivateTouchpadDeliveryBuildError::EpochMismatch {
                binding_epoch: expected_binding.route_epoch,
                source_epoch: source.epoch(),
            });
        }
        let actual = match channel.current_binding() {
            Ok(binding) => binding,
            Err(error) => {
                channel.close();
                return Err(PrivateTouchpadDeliveryBuildError::Admission(error));
            }
        };
        if let Some(mismatch) = binding_mismatch(&expected_binding, &actual) {
            channel.close();
            return Err(PrivateTouchpadDeliveryBuildError::BindingMismatch(mismatch));
        }
        Ok(Self {
            expected_binding,
            source,
            channel,
            channel_closed: false,
            state: PrivateTouchpadDeliveryState::Active,
            metrics: PrivateTouchpadDeliveryMetrics::default(),
        })
    }

    #[must_use]
    pub const fn state(&self) -> PrivateTouchpadDeliveryState {
        self.state
    }

    #[must_use]
    pub const fn metrics(&self) -> PrivateTouchpadDeliveryMetrics {
        self.metrics
    }

    pub fn deliver(&mut self, frame: &TouchpadFrame) -> Result<(), PrivateTouchpadDeliveryError> {
        match self.state {
            PrivateTouchpadDeliveryState::Faulted => {
                return Err(PrivateTouchpadDeliveryError::Faulted);
            }
            PrivateTouchpadDeliveryState::Closed => {
                return Err(PrivateTouchpadDeliveryError::Closed);
            }
            PrivateTouchpadDeliveryState::Active => {}
        }

        self.validate_current_binding()?;
        let mut tentative_source = self.source.clone();
        let packet = tentative_source.encode(frame)?;
        self.metrics.attempts = self.metrics.attempts.saturating_add(1);
        match self.channel.send(&packet) {
            PrivateTouchpadChannelSendOutcome::Delivered => {
                self.source = tentative_source;
                self.metrics.packets_delivered = self.metrics.packets_delivered.saturating_add(1);
                Ok(())
            }
            PrivateTouchpadChannelSendOutcome::RejectedBeforeWrite => {
                self.metrics.rejected_before_write =
                    self.metrics.rejected_before_write.saturating_add(1);
                Err(PrivateTouchpadDeliveryError::RejectedBeforeWrite)
            }
            PrivateTouchpadChannelSendOutcome::DeliveryUnknown => {
                self.metrics.delivery_unknown = self.metrics.delivery_unknown.saturating_add(1);
                self.fail_closed();
                Err(PrivateTouchpadDeliveryError::DeliveryUnknown)
            }
        }
    }

    pub fn close(&mut self) -> Result<(), PrivateTouchpadDeliveryError> {
        match self.state {
            PrivateTouchpadDeliveryState::Closed => Ok(()),
            PrivateTouchpadDeliveryState::Faulted => Ok(()),
            PrivateTouchpadDeliveryState::Active => {
                self.validate_current_binding()?;
                self.source.close()?;
                self.channel.close();
                self.channel_closed = true;
                self.state = PrivateTouchpadDeliveryState::Closed;
                Ok(())
            }
        }
    }

    fn validate_current_binding(&mut self) -> Result<(), PrivateTouchpadDeliveryError> {
        let actual = match self.channel.current_binding() {
            Ok(binding) => binding,
            Err(error) => {
                self.fail_closed();
                return Err(PrivateTouchpadDeliveryError::Admission(error));
            }
        };
        if let Some(mismatch) = binding_mismatch(&self.expected_binding, &actual) {
            self.fail_closed();
            return Err(PrivateTouchpadDeliveryError::BindingChanged(mismatch));
        }
        Ok(())
    }

    pub(crate) fn fail_closed(&mut self) {
        if !self.channel_closed {
            self.channel.close();
            self.channel_closed = true;
        }
        self.state = PrivateTouchpadDeliveryState::Faulted;
    }
}

impl<C: PrivateTouchpadAdmittedChannel> Drop for PrivateTouchpadDeliverySession<C> {
    fn drop(&mut self) {
        if !self.channel_closed {
            self.channel.close();
            self.channel_closed = true;
        }
    }
}

pub(crate) fn binding_mismatch(
    expected: &PrivateTouchpadRouteBinding,
    actual: &PrivateTouchpadRouteBinding,
) -> Option<PrivateTouchpadBindingMismatch> {
    if actual.route_id != expected.route_id {
        return Some(PrivateTouchpadBindingMismatch::Route {
            expected: expected.route_id,
            actual: actual.route_id,
        });
    }
    if actual.session_id != expected.session_id {
        return Some(PrivateTouchpadBindingMismatch::Session {
            expected: expected.session_id,
            actual: actual.session_id,
        });
    }
    if actual.source != expected.source {
        return Some(PrivateTouchpadBindingMismatch::Source {
            expected: expected.source,
            actual: actual.source,
        });
    }
    if actual.sink != expected.sink {
        return Some(PrivateTouchpadBindingMismatch::Sink {
            expected: expected.sink,
            actual: actual.sink,
        });
    }
    if actual.route_epoch != expected.route_epoch {
        return Some(PrivateTouchpadBindingMismatch::Epoch {
            expected: expected.route_epoch,
            actual: actual.route_epoch,
        });
    }
    if actual.authorization_expires_at_ms != expected.authorization_expires_at_ms {
        return Some(PrivateTouchpadBindingMismatch::AuthorizationExpiry {
            expected: expected.authorization_expires_at_ms,
            actual: actual.authorization_expires_at_ms,
        });
    }
    None
}
