use capyio_core::StreamId;

use crate::{EncodedDisplayError, EncodedDisplayStreamSpec};

const ANNEX_B_START_CODE: [u8; 4] = [0, 0, 0, 1];

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EncodedAccessUnitFlags {
    pub key_frame: bool,
    pub discontinuity: bool,
    pub end_of_stream: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EncodedAccessUnitDescriptor {
    pub stream_id: StreamId,
    pub stream_epoch: u64,
    pub sequence: u64,
    pub presentation_time_micros: u64,
    pub duration_nanos: u64,
    pub payload_bytes: u32,
    pub flags: EncodedAccessUnitFlags,
}

impl EncodedAccessUnitDescriptor {
    pub fn validate(&self, selected: &EncodedDisplayStreamSpec) -> Result<(), EncodedDisplayError> {
        selected.validate()?;
        if self.stream_epoch == 0 {
            return Err(EncodedDisplayError::InvalidAccessUnit(
                "stream epoch must be positive".to_owned(),
            ));
        }
        if self.duration_nanos == 0 {
            return Err(EncodedDisplayError::InvalidAccessUnit(
                "duration must be positive".to_owned(),
            ));
        }
        if self.flags.end_of_stream {
            if self.payload_bytes != 0 || self.flags.key_frame || self.flags.discontinuity {
                return Err(EncodedDisplayError::InvalidAccessUnit(
                    "end-of-stream is a distinct empty terminal access unit".to_owned(),
                ));
            }
            return Ok(());
        }
        if self.payload_bytes == 0 || self.payload_bytes > selected.maximum_access_unit_bytes {
            return Err(EncodedDisplayError::InvalidAccessUnit(format!(
                "payload bytes must be inside 1..={} for the selected stream",
                selected.maximum_access_unit_bytes
            )));
        }
        Ok(())
    }
}

/// One complete encoded frame access unit with bounded owned bytes.
///
/// Encoding is available only through the explicit private display record
/// functions. It must never enter JSON-RPC or derive a generic serialization
/// contract.
#[derive(Debug, Eq, PartialEq)]
pub struct OwnedEncodedAccessUnit {
    pub descriptor: EncodedAccessUnitDescriptor,
    pub payload: Vec<u8>,
}

impl OwnedEncodedAccessUnit {
    pub fn validate(&self, selected: &EncodedDisplayStreamSpec) -> Result<(), EncodedDisplayError> {
        self.descriptor.validate(selected)?;
        let actual = u32::try_from(self.payload.len()).map_err(|_| {
            EncodedDisplayError::InvalidAccessUnit(
                "payload length cannot fit in the descriptor".to_owned(),
            )
        })?;
        if actual != self.descriptor.payload_bytes {
            return Err(EncodedDisplayError::InvalidAccessUnit(
                "owned payload length does not match its descriptor".to_owned(),
            ));
        }
        if !self.descriptor.flags.end_of_stream
            && (self.payload.len() <= ANNEX_B_START_CODE.len()
                || !self.payload.starts_with(&ANNEX_B_START_CODE))
        {
            return Err(EncodedDisplayError::InvalidAccessUnit(
                "AVC access unit must start with a four-byte Annex-B start code".to_owned(),
            ));
        }
        Ok(())
    }
}
