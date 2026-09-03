use std::{error::Error, fmt};

use capyio_input::{
    InputContractError, InputSequenceOutcome, InputSequenceTracker, InputStreamDescriptor,
    TouchpadDescriptor, TouchpadFrame, TouchpadFrameKind,
};

use crate::{PrivateTouchpadPacketCodecV1, PrivateTouchpadPacketError, PrivateTouchpadPacketV1};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadPacketSourceState {
    AwaitingCancellation,
    Active,
    Closed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum PrivateTouchpadPacketSourceError {
    Input(InputContractError),
    Packet(PrivateTouchpadPacketError),
    InitialCancellationRequired,
    SequenceGap { expected: u64, actual: u64 },
    ActiveContactsAtClose,
    Closed,
}

impl fmt::Display for PrivateTouchpadPacketSourceError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Input(error) => error.fmt(formatter),
            Self::Packet(error) => error.fmt(formatter),
            Self::InitialCancellationRequired => {
                formatter.write_str("private touchpad packet source requires initial cancel_all")
            }
            Self::SequenceGap { expected, actual } => write!(
                formatter,
                "private touchpad packet source expected sequence {expected}, got {actual}"
            ),
            Self::ActiveContactsAtClose => formatter
                .write_str("private touchpad packet source cannot close with active contacts"),
            Self::Closed => formatter.write_str("private touchpad packet source is closed"),
        }
    }
}

impl Error for PrivateTouchpadPacketSourceError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Input(error) => Some(error),
            Self::Packet(error) => Some(error),
            _ => None,
        }
    }
}

impl From<InputContractError> for PrivateTouchpadPacketSourceError {
    fn from(value: InputContractError) -> Self {
        Self::Input(value)
    }
}

impl From<PrivateTouchpadPacketError> for PrivateTouchpadPacketSourceError {
    fn from(value: PrivateTouchpadPacketError) -> Self {
        Self::Packet(value)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PrivateTouchpadPacketSource {
    codec: PrivateTouchpadPacketCodecV1,
    sequence: InputSequenceTracker,
    state: PrivateTouchpadPacketSourceState,
    has_active_contacts: bool,
    packets_encoded: u64,
}

impl PrivateTouchpadPacketSource {
    pub fn new(
        stream: InputStreamDescriptor,
        descriptor: TouchpadDescriptor,
        first_sequence: u64,
    ) -> Result<Self, PrivateTouchpadPacketSourceError> {
        Ok(Self {
            codec: PrivateTouchpadPacketCodecV1::new(stream.clone(), descriptor)?,
            sequence: InputSequenceTracker::new(
                stream.stream_id,
                stream.stream_epoch,
                first_sequence,
            )?,
            state: PrivateTouchpadPacketSourceState::AwaitingCancellation,
            has_active_contacts: false,
            packets_encoded: 0,
        })
    }

    #[must_use]
    pub const fn state(&self) -> PrivateTouchpadPacketSourceState {
        self.state
    }

    #[must_use]
    pub const fn packets_encoded(&self) -> u64 {
        self.packets_encoded
    }

    #[must_use]
    pub const fn epoch(&self) -> u64 {
        self.codec.epoch()
    }

    pub fn encode(
        &mut self,
        frame: &TouchpadFrame,
    ) -> Result<PrivateTouchpadPacketV1, PrivateTouchpadPacketSourceError> {
        if self.state == PrivateTouchpadPacketSourceState::Closed {
            return Err(PrivateTouchpadPacketSourceError::Closed);
        }
        if self.state == PrivateTouchpadPacketSourceState::AwaitingCancellation
            && frame.kind != TouchpadFrameKind::CancelAll
        {
            return Err(PrivateTouchpadPacketSourceError::InitialCancellationRequired);
        }

        let packet = self.codec.encode(frame)?;
        let mut next_sequence = self.sequence;
        match next_sequence.observe(frame.header)? {
            InputSequenceOutcome::InOrder => {}
            InputSequenceOutcome::Gap(gap) => {
                return Err(PrivateTouchpadPacketSourceError::SequenceGap {
                    expected: gap.first_missing,
                    actual: frame.header.sequence,
                });
            }
        }

        let next_packets_encoded = self
            .packets_encoded
            .checked_add(1)
            .ok_or(InputContractError::SequenceExhausted)?;
        self.sequence = next_sequence;
        self.state = PrivateTouchpadPacketSourceState::Active;
        self.has_active_contacts =
            frame.kind == TouchpadFrameKind::Update && !frame.contacts.is_empty();
        self.packets_encoded = next_packets_encoded;
        Ok(packet)
    }

    pub fn close(&mut self) -> Result<(), PrivateTouchpadPacketSourceError> {
        match self.state {
            PrivateTouchpadPacketSourceState::AwaitingCancellation => {
                Err(PrivateTouchpadPacketSourceError::InitialCancellationRequired)
            }
            PrivateTouchpadPacketSourceState::Active if self.has_active_contacts => {
                Err(PrivateTouchpadPacketSourceError::ActiveContactsAtClose)
            }
            PrivateTouchpadPacketSourceState::Active => {
                self.state = PrivateTouchpadPacketSourceState::Closed;
                Ok(())
            }
            PrivateTouchpadPacketSourceState::Closed => Ok(()),
        }
    }
}
