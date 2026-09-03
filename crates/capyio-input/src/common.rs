use capyio_core::StreamId;
use serde::{Deserialize, Serialize};

use crate::InputContractError;

const MAX_CLOCK_DOMAIN_BYTES: usize = 128;

/// Stream-scoped timing identity carried by a Port/data-plane setup, not repeated per event.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputStreamDescriptor {
    pub stream_id: StreamId,
    pub stream_epoch: u64,
    pub clock_domain_id: String,
}

impl InputStreamDescriptor {
    pub fn validate(&self) -> Result<(), InputContractError> {
        if self.stream_epoch == 0 {
            return Err(InputContractError::InvalidStream(
                "stream epoch must be positive".to_owned(),
            ));
        }
        validate_input_text(
            &self.clock_domain_id,
            "clock domain ID",
            MAX_CLOCK_DOMAIN_BYTES,
        )
        .map_err(InputContractError::InvalidStream)
    }
}

/// Allocation-free per-frame identity and source-monotonic timestamp.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFrameHeader {
    pub stream_id: StreamId,
    pub stream_epoch: u64,
    pub sequence: u64,
    pub source_timestamp_nanos: u64,
}

impl InputFrameHeader {
    pub fn validate(&self) -> Result<(), InputContractError> {
        if self.stream_epoch == 0 {
            return Err(InputContractError::InvalidHeader(
                "stream epoch must be positive".to_owned(),
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SequenceGap {
    pub first_missing: u64,
    pub last_missing: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InputSequenceOutcome {
    InOrder,
    Gap(SequenceGap),
}

/// Allocation-free epoch/sequence guard shared by pointer, touch, keyboard,
/// gamepad and haptics consumers.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct InputSequenceTracker {
    stream_id: StreamId,
    stream_epoch: u64,
    next_sequence: Option<u64>,
}

impl InputSequenceTracker {
    pub fn new(
        stream_id: StreamId,
        stream_epoch: u64,
        first_sequence: u64,
    ) -> Result<Self, InputContractError> {
        if stream_epoch == 0 {
            return Err(InputContractError::InvalidStream(
                "stream epoch must be positive".to_owned(),
            ));
        }
        Ok(Self {
            stream_id,
            stream_epoch,
            next_sequence: Some(first_sequence),
        })
    }

    pub fn observe(
        &mut self,
        header: InputFrameHeader,
    ) -> Result<InputSequenceOutcome, InputContractError> {
        header.validate()?;
        if header.stream_id != self.stream_id {
            return Err(InputContractError::WrongStream {
                expected: self.stream_id,
                actual: header.stream_id,
            });
        }
        if header.stream_epoch < self.stream_epoch {
            return Err(InputContractError::StaleEpoch {
                current: self.stream_epoch,
                actual: header.stream_epoch,
            });
        }
        if header.stream_epoch > self.stream_epoch {
            return Err(InputContractError::FutureEpoch {
                current: self.stream_epoch,
                actual: header.stream_epoch,
            });
        }
        let expected = self
            .next_sequence
            .ok_or(InputContractError::SequenceExhausted)?;
        if header.sequence < expected {
            return Err(InputContractError::DuplicateOrLate {
                expected,
                actual: header.sequence,
            });
        }
        let outcome = if header.sequence == expected {
            InputSequenceOutcome::InOrder
        } else {
            InputSequenceOutcome::Gap(SequenceGap {
                first_missing: expected,
                last_missing: header.sequence - 1,
            })
        };
        self.next_sequence = header.sequence.checked_add(1);
        Ok(outcome)
    }

    pub fn advance_epoch(
        &mut self,
        new_epoch: u64,
        first_sequence: u64,
    ) -> Result<(), InputContractError> {
        if new_epoch <= self.stream_epoch {
            return Err(InputContractError::NonAdvancingEpoch {
                current_epoch: self.stream_epoch,
                new_epoch,
            });
        }
        self.stream_epoch = new_epoch;
        self.next_sequence = Some(first_sequence);
        Ok(())
    }
}

/// Position in the closed unit square: origin at top-left, X right, Y down.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NormalizedPosition {
    pub x: u16,
    pub y: u16,
}

impl NormalizedPosition {
    #[must_use]
    pub const fn new(x: u16, y: u16) -> Self {
        Self { x, y }
    }
}

/// Signed normalized axis where -32767 and 32767 are full-scale and zero is neutral.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SignedAxis(i16);

impl SignedAxis {
    pub fn new(value: i16) -> Result<Self, InputContractError> {
        let axis = Self(value);
        axis.validate()?;
        Ok(axis)
    }

    #[must_use]
    pub const fn neutral() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn get(self) -> i16 {
        self.0
    }

    pub fn validate(self) -> Result<(), InputContractError> {
        if self.0 == i16::MIN {
            return Err(InputContractError::InvalidGamepadState(
                "signed axis reserves -32768 and accepts -32767..=32767".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Generic unsigned normalized magnitude where 0 is idle and 65535 is full-scale.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NormalizedMagnitude(u16);

impl NormalizedMagnitude {
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(value)
    }

    #[must_use]
    pub const fn idle() -> Self {
        Self(0)
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TriggerValue(NormalizedMagnitude);

impl TriggerValue {
    #[must_use]
    pub const fn new(value: u16) -> Self {
        Self(NormalizedMagnitude::new(value))
    }

    #[must_use]
    pub const fn idle() -> Self {
        Self(NormalizedMagnitude::idle())
    }

    #[must_use]
    pub const fn get(self) -> u16 {
        self.0.get()
    }
}

pub fn validate_input_text(value: &str, label: &str, maximum: usize) -> Result<(), String> {
    if value.is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(format!(
            "{label} must contain 1..={maximum} bytes without control characters"
        ));
    }
    Ok(())
}
