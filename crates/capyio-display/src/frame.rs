use capyio_core::StreamId;
use serde::{Deserialize, Serialize};

use crate::{DisplayContractError, DisplayStreamSpec};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayFrameFlags {
    pub discontinuity: bool,
    pub end_of_stream: bool,
    pub clear_surface: bool,
}

/// Metadata for one frame on a separately selected display data plane.
/// It intentionally contains no frame payload bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DisplayFrameDescriptor {
    pub stream_id: StreamId,
    pub stream_epoch: u64,
    pub sequence: u64,
    pub source_timestamp_nanos: u64,
    pub duration_nanos: u64,
    pub payload_bytes: u64,
    pub flags: DisplayFrameFlags,
}

impl DisplayFrameDescriptor {
    pub fn validate(&self, selected: &DisplayStreamSpec) -> Result<(), DisplayContractError> {
        selected.validate()?;
        if self.stream_epoch == 0 {
            return Err(DisplayContractError::InvalidFrame(
                "stream epoch must be positive".to_owned(),
            ));
        }
        if self.duration_nanos == 0 {
            return Err(DisplayContractError::InvalidFrame(
                "frame duration must be positive".to_owned(),
            ));
        }
        if self.flags.end_of_stream && self.flags.clear_surface {
            return Err(DisplayContractError::InvalidFrame(
                "end-of-stream and clear-surface are distinct terminal intents".to_owned(),
            ));
        }
        if self.flags.end_of_stream || self.flags.clear_surface {
            if self.payload_bytes != 0 {
                return Err(DisplayContractError::InvalidFrame(
                    "terminal or clear-surface frames cannot carry payload bytes".to_owned(),
                ));
            }
            return Ok(());
        }
        let expected = selected.packed_frame_bytes().ok_or_else(|| {
            DisplayContractError::InvalidFrame("selected frame size overflowed".to_owned())
        })?;
        if self.payload_bytes != expected {
            return Err(DisplayContractError::InvalidFrame(
                "payload size does not match the selected display mode".to_owned(),
            ));
        }
        Ok(())
    }
}

/// Owned bytes used by hardware-free tests and bounded user-mode handoff.
///
/// This is not a wire format and is intentionally not serializable. Production
/// transports require separate framing, authentication and codec contracts.
#[derive(Debug, Eq, PartialEq)]
pub struct OwnedDisplayFrame {
    pub descriptor: DisplayFrameDescriptor,
    pub payload: Vec<u8>,
}

impl OwnedDisplayFrame {
    pub fn validate(&self, selected: &DisplayStreamSpec) -> Result<(), DisplayContractError> {
        self.descriptor.validate(selected)?;
        let actual = u64::try_from(self.payload.len()).map_err(|_| {
            DisplayContractError::InvalidFrame("payload length cannot fit in u64".to_owned())
        })?;
        if actual != self.descriptor.payload_bytes {
            return Err(DisplayContractError::InvalidFrame(
                "owned payload length does not match its descriptor".to_owned(),
            ));
        }
        Ok(())
    }
}
