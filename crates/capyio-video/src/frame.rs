use capyio_core::StreamId;
use serde::{Deserialize, Serialize};

use crate::spec::MAX_FRAME_PAYLOAD_BYTES;
use crate::{VideoContractError, VideoStreamSpec};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoFrameFlags {
    pub discontinuity: bool,
    pub end_of_stream: bool,
}

/// Metadata for one frame carried by a separately selected video data plane.
/// It intentionally contains no video payload bytes.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VideoFrameDescriptor {
    pub stream_id: StreamId,
    pub stream_epoch: u64,
    pub sequence: u64,
    pub source_timestamp_nanos: u64,
    pub duration_nanos: u64,
    pub payload_bytes: u64,
    pub flags: VideoFrameFlags,
}

impl VideoFrameDescriptor {
    pub fn validate(&self, selected: &VideoStreamSpec) -> Result<(), VideoContractError> {
        selected.validate()?;
        if self.stream_epoch == 0 {
            return Err(VideoContractError::InvalidFrameDescriptor(
                "stream epoch must be positive".to_owned(),
            ));
        }
        if self.duration_nanos == 0 {
            return Err(VideoContractError::InvalidFrameDescriptor(
                "frame duration must be positive".to_owned(),
            ));
        }
        if self.flags.end_of_stream {
            if self.payload_bytes != 0 {
                return Err(VideoContractError::InvalidFrameDescriptor(
                    "end-of-stream descriptors cannot carry payload bytes".to_owned(),
                ));
            }
            return Ok(());
        }
        if self.payload_bytes == 0 || self.payload_bytes > MAX_FRAME_PAYLOAD_BYTES {
            return Err(VideoContractError::InvalidFrameDescriptor(format!(
                "frame payload size must be inside 1..={MAX_FRAME_PAYLOAD_BYTES} bytes"
            )));
        }
        let expected_raw = selected.packed_frame_bytes();
        if expected_raw.is_some_and(|expected| expected != self.payload_bytes) {
            return Err(VideoContractError::InvalidFrameDescriptor(
                "raw frame payload size does not match the selected dimensions".to_owned(),
            ));
        }
        Ok(())
    }
}
